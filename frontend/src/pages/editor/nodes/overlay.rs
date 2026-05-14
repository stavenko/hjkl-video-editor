use api_types::{Edge, Node, NodeKind};
use leptos::*;
use uuid::Uuid;
use web_sys::MouseEvent;

use crate::components::overlay::{OverlayKfEditor, OverlayPreviewAnim};
use crate::services::api::absolute_url;
use crate::services::project_service;

pub fn overlay_body(
    node_signal: RwSignal<Node>,
    project_id: Uuid,
    id_for_drag: Uuid,
    edges: RwSignal<Vec<Edge>>,
    nodes: RwSignal<Vec<Node>>,
) -> impl IntoView {
    let n = node_signal.get_untracked();
    let kfs = match &n.settings {
        Some(api_types::NodeSettings::Overlay { keyframes }) => keyframes.clone(),
        _ => Vec::new(),
    };
    let keyframes = create_rw_signal(kfs);
    let editing_idx = create_rw_signal::<Option<usize>>(None);
    let overlay_node_id = n.id;

    let find_connected = move |port: &str| -> Option<(Uuid, String, Option<f64>)> {
        let es = edges.get();
        let ns = nodes.get();
        let port = port.to_string();
        es.iter()
            .find(|e| e.to_node == overlay_node_id && e.to_port == port)
            .and_then(|e| {
                let src = ns.iter().find(|n| n.id == e.from_node)?;
                let resolved = match src.kind {
                    NodeKind::Reference { source } =>
                        api_types::resolve_reference(&ns, source)?,
                    _ => src,
                };
                let slug = match resolved.kind {
                    NodeKind::Input(ik) => ik.url_slug().to_string(),
                    NodeKind::Process(pk) => pk.url_slug().to_string(),
                    _ => return None,
                };
                let dur = resolved.asset.as_ref()
                    .and_then(|a| a.duration_secs);
                Some((resolved.id, slug, dur))
            })
    };

    let save_kfs = move || {
        let kfs = keyframes.get_untracked();
        let settings = api_types::NodeSettings::Overlay { keyframes: kfs };
        spawn_local(async move {
            let _ = project_service::update_node_settings(project_id, id_for_drag, settings).await;
        });
    };

    view! {
        <div class="overlay-editor"
            on:mousedown=|ev: MouseEvent| ev.stop_propagation()
            on:mousemove=|ev: MouseEvent| ev.stop_propagation()
        >
            {move || {
                let kfs = keyframes.get();
                if kfs.is_empty() {
                    view! { <div class="overlay-empty">"Запустите ноду для загрузки точек"</div> }.into_view()
                } else {
                    let bg_info = find_connected("background");
                    let img_info = find_connected("image");
                    let img_url: Option<String> = img_info.map(|(id, slug, _)| {
                        absolute_url(&format!(
                            "/api/projects/{}/nodes/{}/{}/file",
                            project_id, slug, id
                        ))
                    });
                    let bg_stored = store_value(bg_info);
                    let img_stored = store_value(img_url);
                    let total = kfs.len();
                    kfs.into_iter().enumerate().map(move |(i, kf)| {
                        let interp_label = match kf.interpolation {
                            api_types::Interpolation::Linear => "linear",
                            api_types::Interpolation::EaseIn => "ease-in",
                            api_types::Interpolation::EaseOut => "ease-out",
                            api_types::Interpolation::EaseInOut => "ease-in-out",
                            api_types::Interpolation::Step => "step",
                            api_types::Interpolation::CatmullRom => "smooth",
                        };
                        let has_next = i + 1 < total;
                        view! {
                            <div class="overlay-kf-row"
                                class:active=move || editing_idx.get() == Some(i)
                                on:click=move |_| {
                                    editing_idx.set(if editing_idx.get_untracked() == Some(i) { None } else { Some(i) });
                                }
                            >
                                <span class="overlay-kf-time">{format!("{:.0}ms", kf.t_ms)}</span>
                                <span class="overlay-kf-summary">{format!("x:{:.2} y:{:.2} s:{:.1} a:{:.1}", kf.x, kf.y, kf.scale, kf.alpha)}</span>
                            </div>
                            <Show when=move || editing_idx.get() == Some(i)>
                                <OverlayKfEditor
                                    index=i
                                    keyframes=keyframes
                                    bg_info=bg_stored.get_value()
                                    image_url=img_stored.get_value()
                                    project_id=project_id
                                    on_change=save_kfs
                                />
                            </Show>
                            {has_next.then(move || {
                                view! {
                                    <div class="overlay-transition"
                                        on:mousedown=|ev: MouseEvent| ev.stop_propagation()
                                    >
                                        <span class="overlay-transition-arrow">"↕"</span>
                                        <select class="overlay-interp-select"
                                            on:change=move |ev| {
                                                let v = event_target_value(&ev);
                                                let interp = match v.as_str() {
                                                    "Linear" => api_types::Interpolation::Linear,
                                                    "EaseIn" => api_types::Interpolation::EaseIn,
                                                    "EaseOut" => api_types::Interpolation::EaseOut,
                                                    "EaseInOut" => api_types::Interpolation::EaseInOut,
                                                    "Step" => api_types::Interpolation::Step,
                                                    _ => api_types::Interpolation::Linear,
                                                };
                                                keyframes.update(|kfs| {
                                                    if let Some(kf) = kfs.get_mut(i) {
                                                        kf.interpolation = interp;
                                                    }
                                                });
                                                save_kfs();
                                            }
                                        >
                                            <option value="Linear" selected=interp_label == "linear">"Linear"</option>
                                            <option value="EaseIn" selected=interp_label == "ease-in">"Ease In"</option>
                                            <option value="EaseOut" selected=interp_label == "ease-out">"Ease Out"</option>
                                            <option value="EaseInOut" selected=interp_label == "ease-in-out">"Ease In/Out"</option>
                                            <option value="Step" selected=interp_label == "step">"Step"</option>
                                        </select>
                                    </div>
                                }
                            })}
                        }
                    }).collect_view()
                }
            }}
            <OverlayPreviewAnim
                keyframes=keyframes
                bg_info=find_connected("background")
                image_url={
                    let img = find_connected("image");
                    img.map(|(id, slug, _)| absolute_url(&format!(
                        "/api/projects/{}/nodes/{}/{}/file",
                        project_id, slug, id
                    )))
                }
                project_id=project_id
            />
        </div>
    }.into_view()
}
