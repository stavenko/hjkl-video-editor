use std::sync::Arc;
use actix_web::{web, HttpResponse};
use serde::Deserialize;
use uuid::Uuid;
use api_types::NodeKind;

use crate::providers::{Ffmpeg, ProjectStorage};

#[derive(Deserialize)]
pub struct Params {
    pub project_id: Uuid,
}

#[derive(Deserialize)]
pub struct Query {
    pub video_node: Uuid,
    pub video_slug: String,
    pub subs_node: Uuid,
    #[serde(default)]
    pub t: f32,
    #[serde(default = "default_w")]
    pub w: u32,
}

fn default_w() -> u32 { 640 }

pub async fn handler(
    params: web::Path<Params>,
    query: web::Query<Query>,
    storage: web::Data<Arc<ProjectStorage>>,
    ffmpeg: web::Data<Ffmpeg>,
) -> HttpResponse {
    let graph = match storage.read_graph(params.project_id).await {
        Ok(g) => g,
        Err(_) => return HttpResponse::NotFound().body("Project not found"),
    };

    // Find video node and get its output file
    let video_node = graph.nodes.iter().find(|n| n.id == query.video_node);
    let video_node = match video_node {
        Some(n) => match n.kind {
            NodeKind::Reference { source } =>
                crate::models::project::resolve_reference(&graph.nodes, source).unwrap_or(n),
            _ => n,
        },
        None => return HttpResponse::NotFound().body("Video node not found"),
    };

    let video_path = match &video_node.output {
        Some(o) => storage.assets_dir(params.project_id).join(&o.file_name),
        None => {
            // Try asset
            match &video_node.asset {
                Some(a) => storage.asset_file_path(params.project_id, a),
                None => return HttpResponse::NotFound().body("No video output"),
            }
        }
    };

    let duration = video_node.output.as_ref().and_then(|o| o.duration_ms.map(|d| d / 1000.0))
        .or(video_node.asset.as_ref().and_then(|a| a.duration_secs))
        .unwrap_or(1.0);

    // Find subs node and get its ASS output
    let subs_node = graph.nodes.iter().find(|n| n.id == query.subs_node);
    let subs_node = match subs_node {
        Some(n) => n,
        None => return HttpResponse::NotFound().body("Subs node not found"),
    };
    let subs_output = match &subs_node.output {
        Some(o) => o,
        None => return HttpResponse::NotFound().body("Subs has no output"),
    };

    let ass_path = storage.assets_dir(params.project_id).join(&subs_output.file_name);

    // Seek time
    let t = query.t.clamp(0.0, 1.0);
    let seek_secs = t as f64 * duration;

    // ffmpeg: extract frame at t, burn ASS subtitles, output PNG
    let w = query.w.clamp(50, 1920);
    let ass_str = ass_path.to_string_lossy().replace('\\', "/");

    let output = tokio::process::Command::new(ffmpeg.binary())
        .arg("-y").arg("-hide_banner").arg("-loglevel").arg("error")
        .arg("-ss").arg(format!("{:.3}", seek_secs))
        .arg("-copyts")
        .arg("-i").arg(&video_path)
        .arg("-frames:v").arg("1")
        .arg("-vf").arg(format!("ass='{}',scale={}:-2", ass_str, w))
        .arg("-f").arg("image2pipe")
        .arg("-vcodec").arg("png")
        .arg("pipe:1")
        .output()
        .await;

    match output {
        Ok(out) if out.status.success() => {
            HttpResponse::Ok()
                .content_type("image/png")
                .body(out.stdout)
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            tracing::error!("subtitle preview failed: {}", stderr);
            HttpResponse::InternalServerError().body(stderr.to_string())
        }
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}
