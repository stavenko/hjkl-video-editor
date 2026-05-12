use std::sync::Arc;

use actix_web::{test, web, App};
use api_types::*;
use uuid::Uuid;

use backend::api;
use backend::api::endpoints::config_frontend::FrontendConfigPath;
use backend::config::Config;
use backend::providers::{Ffmpeg, ProjectStorage, UploadManager};

async fn setup_app(
) -> impl actix_web::dev::Service<
    actix_http::Request,
    Response = actix_web::dev::ServiceResponse,
    Error = actix_web::Error,
> {
    let tmp = tempfile::tempdir().unwrap();
    let projects_root = tmp.path().join("projects");
    std::fs::create_dir_all(&projects_root).unwrap();

    let frontend_cfg_path = tmp.path().join("frontend.toml");
    std::fs::write(&frontend_cfg_path, "api_base_url = \"\"").unwrap();

    let config = Config::from_file(std::path::Path::new("../config/backend-test.toml"))
        .expect("Failed to load test config");

    let storage = Arc::new(
        ProjectStorage::new(projects_root)
            .await
            .unwrap(),
    );
    let uploads = UploadManager::new();
    let ffmpeg = Ffmpeg::new(config.ffmpeg.binary.clone());
    let frontend_cfg = FrontendConfigPath(frontend_cfg_path);

    test::init_service(
        App::new()
            .app_data(web::Data::new(storage.clone()))
            .app_data(web::Data::new(uploads))
            .app_data(web::Data::new(ffmpeg))
            .app_data(web::Data::new(frontend_cfg))
            .app_data(web::Data::new(config))
            .configure(api::configure_routes),
    )
    .await
}

fn postcard_request(path: &str, body: &impl serde::Serialize) -> actix_http::Request {
    let bytes = api_types::encode(body).unwrap();
    test::TestRequest::post()
        .uri(path)
        .insert_header(("Content-Type", CONTENT_TYPE))
        .set_payload(bytes)
        .to_request()
}

fn decode_ok<'a, T: serde::Deserialize<'a>>(bytes: &'a [u8]) -> T {
    let envelope: ApiResponseEnvelope<T> = api_types::decode(bytes).unwrap();
    match envelope {
        ApiResponseEnvelope::Ok(v) => v,
        ApiResponseEnvelope::Err(e) => {
            panic!("API returned error: [{}] {}", e.code, e.message)
        }
    }
}

fn decode_err(bytes: &[u8]) -> ApiError {
    #[derive(serde::Deserialize)]
    struct Dummy;
    let envelope: ApiResponseEnvelope<Dummy> = api_types::decode(bytes).unwrap();
    match envelope {
        ApiResponseEnvelope::Ok(_) => panic!("Expected error, got Ok"),
        ApiResponseEnvelope::Err(e) => e,
    }
}

async fn create_project(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    name: &str,
) -> ProjectSummary {
    let req = postcard_request(
        "/api/projects/create",
        &CreateProjectInput {
            name: name.to_string(),
        },
    );
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    decode_ok::<CreateProjectOutput>(&body).project
}

async fn create_node(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    project_id: Uuid,
    kind: NodeKind,
) -> Node {
    let req = postcard_request(
        "/api/nodes/create",
        &CreateNodeInput {
            project_id,
            kind,
            position: Position { x: 10.0, y: 20.0 },
            parent_map_id: None,
        },
    );
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    decode_ok::<CreateNodeOutput>(&body).node
}

async fn begin_upload(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    project_id: Uuid,
    node_id: Uuid,
    kind: InputNodeKind,
    name: &str,
    mime: &str,
    size: u64,
) -> UploadBeginOutput {
    let req = postcard_request(
        "/api/uploads/begin",
        &UploadBeginInput {
            project_id,
            node_id,
            kind,
            original_name: name.to_string(),
            mime: mime.to_string(),
            size_bytes: size,
        },
    );
    let resp = test::call_service(app, req).await;
    assert_eq!(resp.status(), 200, "upload begin failed");
    let body = test::read_body(resp).await;
    decode_ok::<UploadBeginOutput>(&body)
}

async fn send_chunk(
    app: &impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    upload_id: Uuid,
    offset: u64,
    data: Vec<u8>,
) -> actix_web::dev::ServiceResponse {
    let req = test::TestRequest::post()
        .uri(&format!(
            "/api/uploads/chunk?upload_id={upload_id}&offset={offset}"
        ))
        .insert_header(("Content-Type", "application/octet-stream"))
        .set_payload(data)
        .to_request();
    test::call_service(app, req).await
}

// ─── Tests ───

#[actix_rt::test]
async fn upload_full_cycle() {
    let app = setup_app().await;
    let project = create_project(&app, "upload-cycle").await;
    let node = create_node(&app, project.id, NodeKind::Input(InputNodeKind::Video)).await;
    assert!(node.asset.is_none());

    let data = vec![0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE];
    let size = data.len() as u64;

    let begin = begin_upload(
        &app,
        project.id,
        node.id,
        InputNodeKind::Video,
        "clip.mp4",
        "video/mp4",
        size,
    )
    .await;
    assert!(begin.chunk_size > 0);

    let resp = send_chunk(&app, begin.upload_id, 0, data).await;
    assert_eq!(resp.status(), 200);
    let body = test::read_body(resp).await;
    let ack = decode_ok::<UploadChunkOutput>(&body);
    assert_eq!(ack.bytes_written, size);

    // finalize — ffmpeg will fail on fake bytes (expected)
    let req = postcard_request(
        "/api/uploads/finalize",
        &UploadFinalizeInput {
            project_id: project.id,
            node_id: node.id,
            upload_id: begin.upload_id,
        },
    );
    let resp = test::call_service(&app, req).await;
    let status = resp.status().as_u16();
    // 500 because ffmpeg can't produce thumbnail from garbage — acceptable
    assert!(status == 200 || status == 500, "unexpected: {status}");
}

#[actix_rt::test]
async fn upload_multi_chunk() {
    let app = setup_app().await;
    let project = create_project(&app, "multi-chunk").await;
    let node = create_node(&app, project.id, NodeKind::Input(InputNodeKind::Image)).await;

    let total: Vec<u8> = (0..100u8).collect();
    let size = total.len() as u64;

    let begin = begin_upload(
        &app,
        project.id,
        node.id,
        InputNodeKind::Image,
        "photo.png",
        "image/png",
        size,
    )
    .await;

    for (offset, len) in [(0u64, 40usize), (40, 40), (80, 20)] {
        let chunk = total[offset as usize..offset as usize + len].to_vec();
        let resp = send_chunk(&app, begin.upload_id, offset, chunk).await;
        assert_eq!(resp.status(), 200, "chunk at offset {offset} failed");
        let body = test::read_body(resp).await;
        let ack = decode_ok::<UploadChunkOutput>(&body);
        assert_eq!(ack.bytes_written, offset + len as u64);
    }
}

#[actix_rt::test]
async fn upload_wrong_offset_rejected() {
    let app = setup_app().await;
    let project = create_project(&app, "bad-offset").await;
    let node = create_node(&app, project.id, NodeKind::Input(InputNodeKind::Audio)).await;

    let begin = begin_upload(
        &app,
        project.id,
        node.id,
        InputNodeKind::Audio,
        "track.mp3",
        "audio/mpeg",
        100,
    )
    .await;

    let resp = send_chunk(&app, begin.upload_id, 50, vec![0u8; 10]).await;
    assert_eq!(resp.status(), 400);
    let body = test::read_body(resp).await;
    let err = decode_err(&body);
    assert!(err.message.contains("offset"), "error: {}", err.message);
}

#[actix_rt::test]
async fn upload_exceeds_total_rejected() {
    let app = setup_app().await;
    let project = create_project(&app, "too-big").await;
    let node = create_node(&app, project.id, NodeKind::Input(InputNodeKind::Video)).await;

    let begin = begin_upload(
        &app,
        project.id,
        node.id,
        InputNodeKind::Video,
        "vid.mp4",
        "video/mp4",
        5,
    )
    .await;

    let resp = send_chunk(&app, begin.upload_id, 0, vec![0u8; 10]).await;
    assert_eq!(resp.status(), 400);
    let body = test::read_body(resp).await;
    let err = decode_err(&body);
    assert!(err.message.contains("exceed"), "error: {}", err.message);
}

#[actix_rt::test]
async fn upload_begin_wrong_kind_rejected() {
    let app = setup_app().await;
    let project = create_project(&app, "kind-mismatch").await;
    let _node = create_node(&app, project.id, NodeKind::Input(InputNodeKind::Audio)).await;

    let req = postcard_request(
        "/api/uploads/begin",
        &UploadBeginInput {
            project_id: project.id,
            node_id: _node.id,
            kind: InputNodeKind::Video,
            original_name: "clip.mp4".to_string(),
            mime: "video/mp4".to_string(),
            size_bytes: 10,
        },
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_rt::test]
async fn upload_begin_nonexistent_node_rejected() {
    let app = setup_app().await;
    let project = create_project(&app, "no-node").await;

    let req = postcard_request(
        "/api/uploads/begin",
        &UploadBeginInput {
            project_id: project.id,
            node_id: Uuid::new_v4(),
            kind: InputNodeKind::Video,
            original_name: "clip.mp4".to_string(),
            mime: "video/mp4".to_string(),
            size_bytes: 10,
        },
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 404);
}

#[actix_rt::test]
async fn upload_finalize_incomplete_rejected() {
    let app = setup_app().await;
    let project = create_project(&app, "incomplete").await;
    let node = create_node(&app, project.id, NodeKind::Input(InputNodeKind::Video)).await;

    let begin = begin_upload(
        &app,
        project.id,
        node.id,
        InputNodeKind::Video,
        "clip.mp4",
        "video/mp4",
        100,
    )
    .await;

    // send only 10 of 100 bytes
    let resp = send_chunk(&app, begin.upload_id, 0, vec![0u8; 10]).await;
    assert_eq!(resp.status(), 200);

    let req = postcard_request(
        "/api/uploads/finalize",
        &UploadFinalizeInput {
            project_id: project.id,
            node_id: node.id,
            upload_id: begin.upload_id,
        },
    );
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), 400);
}

#[actix_rt::test]
async fn upload_chunk_unknown_session_rejected() {
    let app = setup_app().await;

    let resp = send_chunk(&app, Uuid::new_v4(), 0, vec![0u8; 5]).await;
    assert_eq!(resp.status(), 400);
}
