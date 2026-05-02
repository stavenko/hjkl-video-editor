use api_types::{InputNodeKind, Node, NodeKind, Position, ProjectDetail};
use leptos::*;
use leptos_router::{use_params_map, A};
use uuid::Uuid;
use wasm_bindgen::JsCast;
use web_sys::{Event, FileList, HtmlInputElement, MouseEvent, WheelEvent};

use crate::services::api::absolute_url;
use crate::services::{project_service, upload_service};

#[derive(Clone, Copy)]
struct DragState {
    node_id: Uuid,
    pointer_offset_x: f32,
    pointer_offset_y: f32,
}

#[derive(Clone, Copy)]
struct CanvasTransform {
    offset_x: f64,
    offset_y: f64,
    scale: f64,
}

#[component]
pub fn EditorPage() -> impl IntoView {
    let params = use_params_map();
    let project_id = Signal::derive(move || {
        params
            .with(|p| p.get("id").cloned())
            .and_then(|s| Uuid::parse_str(&s).ok())
    });

    let project = create_rw_signal::<Option<ProjectDetail>>(None);
    let nodes = create_rw_signal::<Vec<Node>>(Vec::new());
    let error = create_rw_signal::<Option<String>>(None);
    let add_modal_open = create_rw_signal(false);
    let drag_state = create_rw_signal::<Option<DragState>>(None);
    let drag_pos = create_rw_signal::<Option<(Uuid, Position)>>(None);
    let canvas_ref = create_node_ref::<html::Div>();
    let cam = create_rw_signal(CanvasTransform {
        offset_x: 0.0,
        offset_y: 0.0,
        scale: 1.0,
    });

    let reload = move || {
        let Some(id) = project_id.get_untracked() else {
            return;
        };
        spawn_local(async move {
            match project_service::get_project(id).await {
                Ok(out) => {
                    nodes.set(out.project.nodes.clone());
                    project.set(Some(out.project));
                    error.set(None);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    create_effect(move |_| {
        project_id.get();
        reload();
    });

    let on_create_node = move |kind: InputNodeKind| {
        add_modal_open.set(false);
        let Some(pid) = project_id.get_untracked() else {
            return;
        };
        let position = next_position(&nodes.get_untracked());
        spawn_local(async move {
            match project_service::create_node(pid, NodeKind::Input(kind), position).await {
                Ok(out) => {
                    nodes.update(|ns| ns.push(out.node));
                    error.set(None);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    let on_delete_node = move |node_id: Uuid| {
        let Some(pid) = project_id.get_untracked() else {
            return;
        };
        spawn_local(async move {
            match project_service::delete_node(pid, node_id).await {
                Ok(_) => {
                    nodes.update(|ns| ns.retain(|n| n.id != node_id));
                    error.set(None);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    let screen_to_canvas = move |client_x: i32, client_y: i32| -> (f32, f32) {
        let rect = canvas_ref
            .get_untracked()
            .map(|c| c.get_bounding_client_rect());
        let (rx, ry) = rect
            .map(|r| (r.left(), r.top()))
            .unwrap_or((0.0, 0.0));
        let t = cam.get_untracked();
        let sx = client_x as f64 - rx;
        let sy = client_y as f64 - ry;
        let cx = (sx - t.offset_x) / t.scale;
        let cy = (sy - t.offset_y) / t.scale;
        (cx as f32, cy as f32)
    };

    let start_drag = move |node_id: Uuid, ev: MouseEvent| {
        let (cx, cy) = screen_to_canvas(ev.client_x(), ev.client_y());
        let node_pos = nodes
            .get_untracked()
            .iter()
            .find(|n| n.id == node_id)
            .map(|n| n.position)
            .unwrap_or(Position { x: 0.0, y: 0.0 });
        drag_state.set(Some(DragState {
            node_id,
            pointer_offset_x: cx - node_pos.x,
            pointer_offset_y: cy - node_pos.y,
        }));
    };

    let on_canvas_mouse_move = move |ev: MouseEvent| {
        let Some(state) = drag_state.get_untracked() else {
            return;
        };
        let (cx, cy) = screen_to_canvas(ev.client_x(), ev.client_y());
        let new_pos = Position {
            x: cx - state.pointer_offset_x,
            y: cy - state.pointer_offset_y,
        };
        drag_pos.set(Some((state.node_id, new_pos)));
    };

    let on_mouse_up = move |_ev: MouseEvent| {
        let Some(state) = drag_state.get_untracked() else {
            return;
        };
        let final_pos = drag_pos
            .get_untracked()
            .filter(|(id, _)| *id == state.node_id)
            .map(|(_, p)| p);
        drag_state.set(None);
        // Keep drag_pos alive until server confirms
        let Some(pos) = final_pos else {
            drag_pos.set(None);
            return;
        };
        let Some(pid) = project_id.get_untracked() else {
            drag_pos.set(None);
            return;
        };
        let node_id = state.node_id;
        spawn_local(async move {
            match project_service::update_node_position(pid, node_id, pos).await {
                Ok(_) => {
                    // Update nodes so NodeView picks up new position
                    nodes.update(|ns| {
                        if let Some(n) = ns.iter_mut().find(|n| n.id == node_id) {
                            n.position = pos;
                        }
                    });
                }
                Err(e) => error.set(Some(e.to_string())),
            }
            drag_pos.set(None);
        });
    };

    view! {
        <div style="position: fixed; inset: 0; display: flex; flex-direction: column; background: var(--bg);">
            <div class="editor-toolbar">
                <A href="/" attr:class="back">"← Проекты"</A>
                <div class="title">
                    {move || project.get().map(|p| p.project.name).unwrap_or_default()}
                </div>
                <button on:click=move |_| add_modal_open.set(true)>
                    "Добавить ноду"
                </button>
            </div>

            {move || error.get().map(|m| view! {
                <div class="error" style="margin: 8px 16px;">{m}</div>
            })}

            <div
                class="canvas"
                node_ref=canvas_ref
                on:mousemove=on_canvas_mouse_move
                on:mouseup=on_mouse_up
                on:mouseleave=on_mouse_up
                on:wheel=move |ev: WheelEvent| {
                    ev.prevent_default();
                    let Some(canvas) = canvas_ref.get_untracked() else { return };
                    let rect = canvas.get_bounding_client_rect();
                    let mx = ev.client_x() as f64 - rect.left();
                    let my = ev.client_y() as f64 - rect.top();

                    if ev.ctrl_key() {
                        // Pinch zoom — ctrlKey is set by macOS trackpad pinch
                        let delta = -ev.delta_y() * 0.01;
                        cam.update(|t| {
                            let old_scale = t.scale;
                            t.scale = (t.scale * (1.0 + delta)).clamp(0.1, 5.0);
                            let ratio = t.scale / old_scale;
                            // Zoom towards cursor
                            t.offset_x = mx - (mx - t.offset_x) * ratio;
                            t.offset_y = my - (my - t.offset_y) * ratio;
                        });
                    } else {
                        // Two-finger scroll → pan
                        cam.update(|t| {
                            t.offset_x -= ev.delta_x();
                            t.offset_y -= ev.delta_y();
                        });
                    }
                }
            >
                <div
                    class="canvas-content"
                    style=move || {
                        let t = cam.get();
                        format!(
                            "transform: translate({:.1}px, {:.1}px) scale({:.4});",
                            t.offset_x, t.offset_y, t.scale
                        )
                    }
                >
                <For
                    each=move || nodes.get()
                    key=|n| n.id
                    children=move |node| {
                        let pid = project_id.get_untracked().unwrap_or(Uuid::nil());
                        view! {
                            <NodeView
                                node=node
                                project_id=pid
                                drag_pos=drag_pos
                                on_drag_start=start_drag
                                on_delete=on_delete_node
                                on_uploaded=move |updated: Node| {
                                    let copy = updated.clone();
                                    nodes.update(|ns| {
                                        if let Some(n) = ns.iter_mut().find(|x| x.id == copy.id) {
                                            *n = copy;
                                        }
                                    });
                                }
                                on_upload_error=move |msg: String| error.set(Some(msg))
                            />
                        }
                    }
                />
                </div>
            </div>

            <Show when=move || add_modal_open.get()>
                <AddNodeModal
                    on_select=on_create_node
                    on_close=move || add_modal_open.set(false)
                />
            </Show>
        </div>
    }
}

#[component]
fn AddNodeModal(
    on_select: impl Fn(InputNodeKind) + Copy + 'static,
    on_close: impl Fn() + Copy + 'static,
) -> impl IntoView {
    let active_tab = create_rw_signal(0u8);

    view! {
        <div class="modal-backdrop" on:click=move |_| on_close()>
            <div class="modal" on:click=|ev| ev.stop_propagation()>
                <div class="modal-header">
                    <span class="modal-title">"Добавить ноду"</span>
                    <button class="modal-close" on:click=move |_| on_close()>"✕"</button>
                </div>
                <div class="modal-tabs">
                    <button
                        class:active=move || active_tab.get() == 0
                        on:click=move |_| active_tab.set(0)
                    >"Входные"</button>
                </div>
                <div class="modal-body">
                    <Show when=move || active_tab.get() == 0>
                        <div class="node-type-grid">
                            <button class="node-type-card" on:click=move |_| on_select(InputNodeKind::Video)>
                                <div class="node-type-icon">"🎬"</div>
                                <div class="node-type-label">"Видео"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(InputNodeKind::Audio)>
                                <div class="node-type-icon">"🔊"</div>
                                <div class="node-type-label">"Аудио"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(InputNodeKind::Image)>
                                <div class="node-type-icon">"🖼"</div>
                                <div class="node-type-label">"Изображение"</div>
                            </button>
                        </div>
                    </Show>
                </div>
            </div>
        </div>
    }
}

fn next_position(existing: &[Node]) -> Position {
    let count = existing.len() as f32;
    Position {
        x: 80.0 + (count % 5.0) * 60.0,
        y: 80.0 + (count / 5.0).floor() * 60.0,
    }
}

#[component]
fn NodeView(
    node: Node,
    project_id: Uuid,
    drag_pos: RwSignal<Option<(Uuid, Position)>>,
    on_drag_start: impl Fn(Uuid, MouseEvent) + Copy + 'static,
    on_delete: impl Fn(Uuid) + Copy + 'static,
    on_uploaded: impl Fn(Node) + Copy + 'static,
    on_upload_error: impl Fn(String) + Copy + 'static,
) -> impl IntoView {
    let node_signal = create_rw_signal(node);
    let upload_progress = create_rw_signal::<Option<(u64, u64)>>(None);
    let confirm_delete = create_rw_signal(false);
    let last_drag_pos = create_rw_signal::<Option<Position>>(None);

    let nid = node_signal.with_untracked(|n| n.id);
    create_effect(move |_| {
        let dp = drag_pos.get();
        match dp {
            Some((id, pos)) if id == nid => {
                last_drag_pos.set(Some(pos));
            }
            _ => {
                // drag ended or moved to another node — commit last position
                if let Some(pos) = last_drag_pos.get_untracked() {
                    node_signal.update(|n| n.position = pos);
                    last_drag_pos.set(None);
                }
            }
        }
    });

    let on_file_picked = move |files: FileList| {
        let Some(file) = files.item(0) else { return };
        let n = node_signal.get_untracked();
        let NodeKind::Input(kind) = n.kind;
        upload_progress.set(Some((0, file.size() as u64)));
        spawn_local(async move {
            let result = upload_service::upload_file(project_id, n.id, kind, file, |p| {
                upload_progress.set(Some((p.bytes_sent, p.total_bytes)));
            })
            .await;
            upload_progress.set(None);
            match result {
                Ok(updated) => {
                    node_signal.set(updated.clone());
                    on_uploaded(updated);
                }
                Err(e) => on_upload_error(e.to_string()),
            }
        });
    };

    let id_for_drag = node_signal.with_untracked(|n| n.id);

    view! {
        <div
            class="node"
            style=move || {
                let pos = drag_pos
                    .get()
                    .filter(|(id, _)| *id == id_for_drag)
                    .map(|(_, p)| p)
                    .unwrap_or(node_signal.get().position);
                format!("left: {}px; top: {}px;", pos.x, pos.y)
            }
        >
            <div
                class="node-header"
                on:mousedown=move |ev: MouseEvent| {
                    ev.prevent_default();
                    on_drag_start(id_for_drag, ev);
                }
            >
                <span class="node-kind-badge">{move || kind_label(node_signal.get().kind)}</span>
                <div class="spacer"></div>
                <div style="position: relative;">
                    <button class="delete" on:click=move |ev: MouseEvent| {
                        ev.stop_propagation();
                        confirm_delete.update(|v| *v = !*v);
                    }>
                        "\u{1F5D1}"
                    </button>
                    <Show when=move || confirm_delete.get()>
                        <div class="delete-confirm" on:click=|ev| ev.stop_propagation()>
                            <span>"Удалить ноду?"</span>
                            <button class="danger" on:click=move |_| {
                                confirm_delete.set(false);
                                on_delete(id_for_drag);
                            }>"Да"</button>
                            <button class="ghost" on:click=move |_| confirm_delete.set(false)>"Нет"</button>
                        </div>
                    </Show>
                </div>
            </div>
            <div class="node-body">
                {move || {
                    let n = node_signal.get();
                    match (n.asset.clone(), upload_progress.get()) {
                        (Some(asset), _) => {
                            let nid = n.id;
                            view! { <AssetView project_id=project_id node_id=nid asset=asset /> }.into_view()
                        }
                        (None, Some((sent, total))) => view! {
                            <div class="upload-form">
                                <div class="filename">{format_progress(sent, total)}</div>
                                <div class="progress">
                                    <div style=move || {
                                        let (s, t) = upload_progress.get().unwrap_or((0, 1));
                                        let pct = if t == 0 { 0.0 } else { (s as f64 / t as f64) * 100.0 };
                                        format!("width: {:.1}%;", pct)
                                    }></div>
                                </div>
                            </div>
                        }.into_view(),
                        (None, None) => {
                            let NodeKind::Input(k) = node_signal.get().kind;
                            view! {
                                <UploadInput kind=k on_file=on_file_picked/>
                            }
                            .into_view()
                        },
                    }
                }}
            </div>
            <div class="output-handle"></div>
        </div>
    }
}

#[component]
fn UploadInput(
    kind: InputNodeKind,
    on_file: impl Fn(FileList) + Copy + 'static,
) -> impl IntoView {
    let accept = match kind {
        InputNodeKind::Video => "video/*",
        InputNodeKind::Audio => "audio/*",
        InputNodeKind::Image => "image/*",
    };
    view! {
        <input
            type="file"
            accept=accept
            on:change=move |ev: Event| {
                let target = ev.target().and_then(|t| t.dyn_into::<HtmlInputElement>().ok());
                if let Some(input) = target {
                    if let Some(files) = input.files() {
                        on_file(files);
                    }
                }
            }
        />
    }
}

fn thumbnail_url(project_id: Uuid, node_id: Uuid, kind: InputNodeKind) -> String {
    let slug = kind.url_slug();
    absolute_url(&format!(
        "/api/projects/{project_id}/nodes/{slug}/{node_id}/thumbnail"
    ))
}

fn thumbnail_url_with_t(project_id: Uuid, node_id: Uuid, kind: InputNodeKind, t: f32) -> String {
    let slug = kind.url_slug();
    absolute_url(&format!(
        "/api/projects/{project_id}/nodes/{slug}/{node_id}/thumbnail?t={t:.4}"
    ))
}

#[component]
fn AssetView(
    project_id: Uuid,
    node_id: Uuid,
    asset: api_types::Asset,
) -> impl IntoView {
    let original = asset.original_name.clone();
    let size = format_size(asset.size_bytes);
    let kind = asset.kind;

    match kind {
        InputNodeKind::Video => {
            let debounced_t = create_rw_signal::<Option<f32>>(None);
            let cursor_x_pct = create_rw_signal::<Option<f64>>(None);
            let debounce_handle = create_rw_signal::<Option<i32>>(None);

            let base_url = thumbnail_url(project_id, node_id, kind);
            let img_src = Signal::derive(move || {
                match debounced_t.get() {
                    Some(t) => thumbnail_url_with_t(project_id, node_id, kind, t),
                    None => base_url.clone(),
                }
            });

            let quantize = |v: f32| -> f32 {
                (v * 50.0).round() / 50.0
            };

            let on_move = move |ev: MouseEvent| {
                let target = ev.target().unwrap();
                let el = target.unchecked_ref::<web_sys::HtmlElement>();
                let rect = el.get_bounding_client_rect();
                let x = ev.client_x() as f64 - rect.left();
                let w = rect.width();
                let pct = if w > 0.0 { (x / w).clamp(0.0, 1.0) } else { 0.0 };

                cursor_x_pct.set(Some(pct));

                // Cancel previous debounce timer
                if let Some(h) = debounce_handle.get_untracked() {
                    web_sys::window().unwrap().clear_timeout_with_handle(h);
                }

                let q = quantize(pct as f32);
                let handle = web_sys::window()
                    .unwrap()
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        &wasm_bindgen::closure::Closure::<dyn Fn()>::new(move || {
                            if debounced_t.get_untracked() != Some(q) {
                                debounced_t.set(Some(q));
                            }
                        })
                        .into_js_value()
                        .unchecked_ref(),
                        150,
                    )
                    .unwrap();
                debounce_handle.set(Some(handle));
            };

            let on_leave = move |_| {
                if let Some(h) = debounce_handle.get_untracked() {
                    web_sys::window().unwrap().clear_timeout_with_handle(h);
                }
                debounce_handle.set(None);
                cursor_x_pct.set(None);
                debounced_t.set(None);
            };

            view! {
                <div class="video-thumb-wrap">
                    <img
                        class="media-thumb"
                        src=img_src
                        alt=original.clone()
                        on:mousemove=on_move
                        on:mouseleave=on_leave
                    />
                    {move || cursor_x_pct.get().map(|pct| {
                        let left = format!("{:.2}%", pct * 100.0);
                        view! {
                            <div class="scrub-line" style:left=left></div>
                        }
                    })}
                </div>
                <div class="meta-row">
                    <span>{original}</span>
                    <span>{size}</span>
                </div>
            }.into_view()
        }
        InputNodeKind::Image => {
            let url = thumbnail_url(project_id, node_id, kind);
            view! {
                <img class="media-thumb" src=url alt=original.clone()/>
                <div class="meta-row">
                    <span>{original}</span>
                    <span>{size}</span>
                </div>
            }.into_view()
        }
        InputNodeKind::Audio => {
            let url = thumbnail_url(project_id, node_id, kind);
            view! {
                <img class="media-waveform" src=url alt=original.clone()/>
                <div class="meta-row">
                    <span>{original}</span>
                    <span>{size}</span>
                </div>
            }.into_view()
        }
    }
}

fn kind_label(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Input(InputNodeKind::Video) => "Видео",
        NodeKind::Input(InputNodeKind::Audio) => "Аудио",
        NodeKind::Input(InputNodeKind::Image) => "Изображение",
    }
}

fn format_progress(sent: u64, total: u64) -> String {
    format!("Загрузка: {} / {}", format_size(sent), format_size(total))
}

fn format_size(bytes: u64) -> String {
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
