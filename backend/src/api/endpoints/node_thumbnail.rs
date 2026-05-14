use std::path::Path;
use std::sync::Arc;

use actix_web::{web, HttpResponse, Responder};
use api_types::{InputNodeKind, NodeKind, ProcessNodeKind};
use serde::Deserialize;
use tokio::fs;
use uuid::Uuid;

use crate::providers::{Ffmpeg, ProjectStorage};

#[derive(Deserialize)]
pub struct PathParams {
    pub project_id: Uuid,
    pub node_type: String,
    pub node_id: Uuid,
}

#[derive(Deserialize)]
pub struct Query {
    #[serde(default)]
    pub t: Option<f32>,
    #[serde(default)]
    pub w: Option<u32>,
}

pub async fn handler(
    storage: web::Data<Arc<ProjectStorage>>,
    ffmpeg: web::Data<Ffmpeg>,
    path: web::Path<PathParams>,
    query: web::Query<Query>,
) -> impl Responder {
    let params = path.into_inner();

    let graph = match storage.read_graph(params.project_id).await {
        Ok(g) => g,
        Err(e) => return HttpResponse::InternalServerError().body(e.to_string()),
    };

    let Some(node) = graph.nodes.iter().find(|n| n.id == params.node_id) else {
        return HttpResponse::NotFound().body("Node not found");
    };

    match node.kind {
        NodeKind::Input(input_kind) => {
            let Some(asset) = &node.asset else {
                return HttpResponse::NotFound().body("Node has no asset");
            };
            match input_kind {
                InputNodeKind::Video => {
                    let t = query.t.unwrap_or(0.0).clamp(0.0, 1.0);
                    if t == 0.0 {
                        let thumb = storage.asset_thumbnail_path(params.project_id, asset.id);
                        serve_static_png(&thumb).await
                    } else {
                        let duration = asset.duration_secs.unwrap_or(1.0);
                        let seek_secs = t as f64 * duration;
                        let file_path = storage.asset_file_path(params.project_id, asset);
                        let width = query.w.unwrap_or(100).clamp(50, 1920);
                        match ffmpeg.generate_frame_at_width(&file_path, seek_secs, width).await {
                            Ok(png_bytes) => HttpResponse::Ok()
                                .content_type("image/png")
                                .insert_header(("Cache-Control", "public, max-age=86400"))
                                .body(png_bytes),
                            Err(e) => {
                                tracing::error!("Frame extraction failed: {}", e);
                                HttpResponse::InternalServerError().body(e.to_string())
                            }
                        }
                    }
                }
                InputNodeKind::Image => {
                    let thumb = storage.asset_thumbnail_path(params.project_id, asset.id);
                    serve_static_png(&thumb).await
                }
                InputNodeKind::VideoArray => {
                    HttpResponse::NotFound().body("No thumbnail for video array")
                }
                InputNodeKind::Audio => {
                    let wave = storage.asset_waveform_path(params.project_id, asset.id);
                    serve_static_png(&wave).await
                }
            }
        }
        NodeKind::Process(pk) => {
            match pk {
                ProcessNodeKind::Clip => {
                    // First frame of preview as thumbnail
                    let preview = storage.node_output_path(params.project_id, params.node_id, "preview.mp4");
                    if preview.exists() {
                        match ffmpeg.generate_frame_at(&preview, 0.0).await {
                            Ok(png) => return HttpResponse::Ok().content_type("image/png").body(png),
                            Err(_) => {}
                        }
                    }
                    HttpResponse::NotFound().body("No clip preview")
                }
                ProcessNodeKind::TrimVideo | ProcessNodeKind::Mux => {
                    let t = query.t.unwrap_or(0.0).clamp(0.0, 1.0);
                    if let Some(out) = node.output.as_ref() {
                        let video_path = storage.assets_dir(params.project_id).join(&out.file_name);
                        if t > 0.0 {
                            let dur = out.duration_ms.unwrap_or(1000.0) / 1000.0;
                            let seek = t as f64 * dur;
                            let width = query.w.unwrap_or(100).clamp(50, 1920);
                            match ffmpeg.generate_frame_at_width(&video_path, seek, width).await {
                                Ok(png) => return HttpResponse::Ok()
                                    .content_type("image/png")
                                    .body(png),
                                Err(_) => {}
                            }
                        } else {
                            match ffmpeg.generate_frame_at(&video_path, 0.0).await {
                                Ok(png) => return HttpResponse::Ok()
                                    .content_type("image/png")
                                    .body(png),
                                Err(_) => {}
                            }
                        }
                    }
                    HttpResponse::NotFound().body("No thumbnail")
                }
                ProcessNodeKind::ExtractAudio | ProcessNodeKind::TrimAudio => {
                    let wave = storage.node_output_waveform_path(params.project_id, params.node_id);
                    serve_static_png(&wave).await
                }
                _ => HttpResponse::NotFound().body("No thumbnail for this node type"),
            }
        }
        NodeKind::Reference { .. } => HttpResponse::NotFound().body("No thumbnail for reference"),
    }
}

async fn serve_static_png(path: &Path) -> HttpResponse {
    match fs::read(path).await {
        Ok(bytes) => HttpResponse::Ok()
            .content_type("image/png")
            .insert_header(("Cache-Control", "public, max-age=31536000, immutable"))
            .body(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            HttpResponse::NotFound().body("Not found")
        }
        Err(e) => {
            tracing::error!("Failed to read {:?}: {}", path, e);
            HttpResponse::InternalServerError().body(e.to_string())
        }
    }
}
