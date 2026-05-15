use api_types::{InputNodeKind, Node, NodeKind, Position, ProcessNodeKind};
use leptos::*;
use leptos::html;
use uuid::Uuid;

use crate::services::api::absolute_url;
use crate::services::project_service;

use super::CanvasTransform;

pub fn wrap_text(ctx: &web_sys::CanvasRenderingContext2d, text: &str, max_width: f64) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() { return vec![text.to_string()]; }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in &words {
        let test = if current.is_empty() { word.to_string() } else { format!("{} {}", current, word) };
        let metrics = ctx.measure_text(&test).unwrap();
        if metrics.width() > max_width && !current.is_empty() {
            lines.push(current);
            current = word.to_string();
        } else {
            current = test;
        }
    }
    if !current.is_empty() { lines.push(current); }
    if lines.is_empty() { lines.push(text.to_string()); }
    lines
}

pub fn parse_ass_time(s: &str) -> f64 {
    // "H:MM:SS.CC" -> milliseconds
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() == 3 {
        let h = parts[0].parse::<f64>().unwrap_or(0.0);
        let m = parts[1].parse::<f64>().unwrap_or(0.0);
        let sec_parts: Vec<&str> = parts[2].split('.').collect();
        let s_val = sec_parts.first().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        let cs = sec_parts.get(1).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        (h * 3600.0 + m * 60.0 + s_val + cs / 100.0) * 1000.0
    } else {
        0.0
    }
}

pub fn kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Input(InputNodeKind::Video) => "Видео",
        NodeKind::Input(InputNodeKind::Audio) => "Аудио",
        NodeKind::Input(InputNodeKind::Image) => "Изображение",
        NodeKind::Input(InputNodeKind::VideoArray) => "Видео (массив)",
        NodeKind::Process(ProcessNodeKind::ExtractAudio) => "Извлечь аудио",
        NodeKind::Process(ProcessNodeKind::DetectSilence) => "Тишина",
        NodeKind::Process(ProcessNodeKind::DetectSubtitles) => "Субтитры",
        NodeKind::Process(ProcessNodeKind::SpeechBounds) => "Края речи",
        NodeKind::Process(ProcessNodeKind::TrimAudio) => "Обрезка аудио",
        NodeKind::Process(ProcessNodeKind::TrimVideo) => "Обрезка видео",
        NodeKind::Process(ProcessNodeKind::Scalar) => "Число",
        NodeKind::Process(ProcessNodeKind::Spline) => "Сплайн",
        NodeKind::Process(ProcessNodeKind::Clip) => "Клип",
        NodeKind::Process(ProcessNodeKind::Mux) => "Композитор",
        NodeKind::Process(ProcessNodeKind::MathAdd) => "A + B",
        NodeKind::Process(ProcessNodeKind::MathSubtract) => "A − B",
        NodeKind::Process(ProcessNodeKind::MathMultiply) => "A × B",
        NodeKind::Process(ProcessNodeKind::MathDivide) => "A ÷ B",
        NodeKind::Process(ProcessNodeKind::Map) => "Map",
        NodeKind::Process(ProcessNodeKind::SubgraphInput) => "Вход",
        NodeKind::Process(ProcessNodeKind::SubgraphOutput) => "Выход",
        NodeKind::Process(ProcessNodeKind::Reduce) => "Reduce",
        NodeKind::Process(ProcessNodeKind::AssBuilder) => "ASS субтитры",
        NodeKind::Process(ProcessNodeKind::SubtitlePiece) => "Subtitle piece",
        NodeKind::Process(ProcessNodeKind::Overlay) => "Оверлей",
        NodeKind::Process(ProcessNodeKind::RemoveBackground) => "Убрать фон",
        NodeKind::Process(ProcessNodeKind::ResizeImage) => "Ресайз",
        NodeKind::Process(ProcessNodeKind::AddBorder) => "Обводка",
        NodeKind::Process(ProcessNodeKind::SubtitleTrack) => "Дорожка СТ",
        NodeKind::Process(ProcessNodeKind::NamedInput) => "Вход →",
        NodeKind::Process(ProcessNodeKind::NamedOutput) => "→ Выход",
        NodeKind::Process(ProcessNodeKind::Template) => "Шаблон",
        NodeKind::Reference { .. } => "&",
    }
}

pub fn poll_task(
    active_task_id: RwSignal<Option<Uuid>>,
    node_signal: RwSignal<Node>,
    task_id: Uuid,
    project_id: Uuid,
) {
    use api_types::TaskStatus;

    spawn_local(async move {
        loop {
            gloo_timers_sleep(1500).await;
            let Some(tid) = active_task_id.get_untracked() else {
                break;
            };
            if tid != task_id {
                break;
            }
            match project_service::get_task_status(task_id).await {
                Ok(out) => {
                    node_signal.update(|n| n.task_status = Some(out.status));
                    match out.status {
                        TaskStatus::Done | TaskStatus::Failed => {
                            active_task_id.set(None);
                            // Reload node from server to get updated output
                            let node_id = out.node_id;
                            if let Ok(proj) = project_service::get_project(project_id).await {
                                if let Some(updated) = proj.project.nodes.iter().find(|n| n.id == node_id) {
                                    node_signal.set(updated.clone());
                                }
                            }
                            break;
                        }
                        _ => {}
                    }
                }
                Err(_) => {
                    active_task_id.set(None);
                    break;
                }
            }
        }
    });
}

pub async fn gloo_timers_sleep(ms: u32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32)
            .unwrap();
    });
    wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
}

pub fn format_progress(sent: u64, total: u64) -> String {
    format!("Загрузка: {} / {}", format_size(sent), format_size(total))
}

pub fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

pub fn next_position(existing: &[Node]) -> Position {
    let count = existing.len() as f32;
    Position {
        x: 80.0 + (count % 5.0) * 60.0,
        y: 80.0 + (count / 5.0).floor() * 60.0,
    }
}

pub fn load_cam(key: &str) -> Option<CanvasTransform> {
    let storage = web_sys::window()?.local_storage().ok()??;
    let val = storage.get_item(key).ok()??;
    let parts: Vec<&str> = val.split(',').collect();
    if parts.len() == 3 {
        Some(CanvasTransform {
            offset_x: parts[0].parse().ok()?,
            offset_y: parts[1].parse().ok()?,
            scale: parts[2].parse().ok()?,
        })
    } else {
        None
    }
}

pub fn handle_id(dir: &str, node_id: Uuid, port: &str) -> String {
    if port.is_empty() {
        format!("handle-{dir}-{node_id}")
    } else {
        format!("handle-{dir}-{node_id}-{port}")
    }
}

pub fn handle_center(
    canvas_el: &Option<leptos::HtmlElement<html::Div>>,
    cam: RwSignal<CanvasTransform>,
    handle_id: &str,
) -> Option<(f32, f32)> {
    let canvas = canvas_el.as_ref()?;
    let handle = leptos::document().get_element_by_id(handle_id)?;
    let hr = handle.get_bounding_client_rect();
    let cr = canvas.get_bounding_client_rect();
    let t = cam.get_untracked();
    let screen_x = hr.left() + hr.width() / 2.0;
    let screen_y = hr.top() + hr.height() / 2.0;
    let cx = ((screen_x - cr.left()) - t.offset_x) / t.scale;
    let cy = ((screen_y - cr.top()) - t.offset_y) / t.scale;
    Some((cx as f32, cy as f32))
}

pub fn thumbnail_url(project_id: Uuid, node_id: Uuid, kind: InputNodeKind) -> String {
    let slug = kind.url_slug();
    absolute_url(&format!(
        "/api/projects/{project_id}/nodes/{slug}/{node_id}/thumbnail"
    ))
}

pub fn thumbnail_url_with_t(project_id: Uuid, node_id: Uuid, kind: InputNodeKind, t: f32) -> String {
    let slug = kind.url_slug();
    absolute_url(&format!(
        "/api/projects/{project_id}/nodes/{slug}/{node_id}/thumbnail?t={t:.4}"
    ))
}

pub fn file_url(project_id: Uuid, node_id: Uuid, kind: InputNodeKind) -> String {
    let slug = kind.url_slug();
    absolute_url(&format!(
        "/api/projects/{project_id}/nodes/{slug}/{node_id}/file"
    ))
}
