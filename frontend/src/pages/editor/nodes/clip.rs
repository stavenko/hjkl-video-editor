use api_types::{Edge, Node, NodeKind};
use leptos::*;
use uuid::Uuid;
use web_sys::MouseEvent;

use crate::components::clip::ClipKfEditor;
use crate::services::project_service;

pub fn clip_body(
    node_signal: RwSignal<Node>,
    project_id: Uuid,
    id_for_drag: Uuid,
    edges: RwSignal<Vec<Edge>>,
    nodes: RwSignal<Vec<Node>>,
) -> impl IntoView {
    let n = node_signal.get_untracked();
    let kfs = match &n.settings {
        Some(api_types::NodeSettings::Clip { keyframes, .. }) => keyframes.clone(),
        _ => Vec::new(),
    };
    let keyframes = create_rw_signal(kfs);
    let editing_idx = create_rw_signal::<Option<usize>>(None);


    let save_kfs = move || {
        let kfs = keyframes.get_untracked();
        let ns = node_signal.get_untracked();
        let (ts, te, ti, to, pw) = match &ns.settings {
            Some(api_types::NodeSettings::Clip { trim_start_ms, trim_end_ms, time_in, time_out, preview_width, .. }) =>
                (*trim_start_ms, *trim_end_ms, *time_in, *time_out, *preview_width),
            _ => (0.0, 0.0, 0.0, 1.0, 320),
        };
        let settings = api_types::NodeSettings::Clip {
            trim_start_ms: ts, trim_end_ms: te, time_in: ti, time_out: to,
            preview_width: pw, keyframes: kfs,
        };
        spawn_local(async move {
            let _ = project_service::update_node_settings(project_id, id_for_drag, settings).await;
            let _ = project_service::run_node(project_id, id_for_drag).await;
        });
    };

    // Find background video via edges (reactive — re-checks when edges/nodes change)
    let node_id = n.id;
    let clip_bg = create_memo(move |_| {
        let es = edges.get();
        let ns_all = nodes.get();
        es.iter()
            .find(|e| e.to_node == node_id && (e.to_port == "media" || e.to_port.is_empty()))
            .and_then(|e| {
                let src = ns_all.iter().find(|n| n.id == e.from_node)?;
                let resolved = match src.kind {
                    NodeKind::Reference { source } => api_types::resolve_reference(&ns_all, source)?,
                    _ => src,
                };
                let slug = match resolved.kind {
                    NodeKind::Input(ik) => ik.url_slug().to_string(),
                    NodeKind::Process(pk) => pk.url_slug().to_string(),
                    _ => return None,
                };
                let dur = resolved.asset.as_ref().and_then(|a| a.duration_secs)
                    .or_else(|| resolved.output.as_ref().and_then(|o| o.duration_ms.map(|d| d / 1000.0)));
                Some((resolved.id, slug, dur))
            })
    });

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
                                <ClipKfEditor
                                    index=i
                                    keyframes=keyframes
                                    bg_info=clip_bg.get()
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
        </div>
    }.into_view()
}
