use api_types::{Edge, InputNodeKind, Node, NodeKind, Position, ProcessNodeKind, ProjectDetail, TaskStatus};
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
    let edges = create_rw_signal::<Vec<Edge>>(Vec::new());
    let error = create_rw_signal::<Option<String>>(None);
    let add_modal_open = create_rw_signal(false);
    let player_src = create_rw_signal::<Option<String>>(None);
    let connecting_from = create_rw_signal::<Option<Uuid>>(None);
    let connect_mouse = create_rw_signal::<Option<(f32, f32)>>(None);
    let json_modal = create_rw_signal::<Option<(String, &'static str)>>(None);
    let placing_kind = create_rw_signal::<Option<NodeKind>>(None);
    let node_list_open = create_rw_signal(false);
    let placing_pos = create_rw_signal::<Option<(f32, f32)>>(None);
    let drag_state = create_rw_signal::<Option<DragState>>(None);
    let drag_pos = create_rw_signal::<Option<(Uuid, Position)>>(None);
    let canvas_ref = create_node_ref::<html::Div>();
    let cam = create_rw_signal({
        let key = project_id.get_untracked()
            .map(|id| format!("cam_{id}"))
            .unwrap_or_default();
        load_cam(&key).unwrap_or(CanvasTransform { offset_x: 0.0, offset_y: 0.0, scale: 1.0 })
    });

    // Persist cam on change (debounced via effect)
    create_effect(move |_| {
        let t = cam.get();
        let Some(id) = project_id.get() else { return };
        let key = format!("cam_{id}");
        if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
            let val = format!("{},{},{}", t.offset_x, t.offset_y, t.scale);
            let _ = storage.set_item(&key, &val);
        }
    });

    let reload = move || {
        let Some(id) = project_id.get_untracked() else {
            return;
        };
        spawn_local(async move {
            match project_service::get_project(id).await {
                Ok(out) => {
                    nodes.set(out.project.nodes.clone());
                    edges.set(out.project.edges.clone());
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

    let on_create_node = move |kind: NodeKind| {
        add_modal_open.set(false);
        placing_kind.set(Some(kind));
        placing_pos.set(None);
    };

    let confirm_placement = move || {
        let Some(kind) = placing_kind.get_untracked() else { return };
        let Some((cx, cy)) = placing_pos.get_untracked() else { return };
        let Some(pid) = project_id.get_untracked() else { return };
        placing_kind.set(None);
        placing_pos.set(None);
        let position = Position { x: cx, y: cy };
        spawn_local(async move {
            match project_service::create_node(pid, kind, position).await {
                Ok(out) => {
                    nodes.update(|ns| ns.push(out.node));
                    error.set(None);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    let cancel_placement = move || {
        placing_kind.set(None);
        placing_pos.set(None);
    };

    // Esc cancels placement
    create_effect(move |_| {
        let cb = wasm_bindgen::closure::Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
            if ev.key() == "Escape" && placing_kind.get_untracked().is_some() {
                cancel_placement();
            }
        });
        leptos::document()
            .add_event_listener_with_callback("keydown", cb.as_ref().unchecked_ref())
            .ok();
        cb.forget();
    });

    // Prevent browser zoom (Ctrl+wheel / pinch) — must be non-passive
    create_effect(move |_| {
        let cb = wasm_bindgen::closure::Closure::<dyn Fn(web_sys::WheelEvent)>::new(move |ev: web_sys::WheelEvent| {
            if ev.ctrl_key() {
                ev.prevent_default();
            }
        });
        let opts = web_sys::AddEventListenerOptions::new();
        opts.set_passive(false);
        leptos::document()
            .add_event_listener_with_callback_and_add_event_listener_options(
                "wheel", cb.as_ref().unchecked_ref(), &opts
            )
            .ok();
        cb.forget();
    });

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

    let on_disconnect = move |from: Uuid, from_port: String, to: Uuid, to_port: String| {
        let Some(pid) = project_id.get_untracked() else { return };
        spawn_local(async move {
            match project_service::disconnect_nodes(pid, from, from_port, to, to_port).await {
                Ok(_) => {
                    error.set(None);
                    reload();
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    // (from_node, from_port) stored when dragging starts
    let connecting_from_port = create_rw_signal(String::new());

    let on_connect_complete = move |from: Uuid, from_port: String, to: Uuid, to_port: String| {
        let Some(pid) = project_id.get_untracked() else { return };
        connecting_from.set(None);
        connect_mouse.set(None);
        spawn_local(async move {
            match project_service::connect_nodes(pid, from, from_port, to, to_port).await {
                Ok(_) => {
                    error.set(None);
                    reload();
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
        if placing_kind.get_untracked().is_some() {
            let (cx, cy) = screen_to_canvas(ev.client_x(), ev.client_y());
            placing_pos.set(Some((cx, cy)));
            return;
        }
        if connecting_from.get_untracked().is_some() {
            let (cx, cy) = screen_to_canvas(ev.client_x(), ev.client_y());
            connect_mouse.set(Some((cx, cy)));
            return;
        }
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
        if placing_kind.get_untracked().is_some() {
            confirm_placement();
            return;
        }
        if connecting_from.get_untracked().is_some() {
            connecting_from.set(None);
            connect_mouse.set(None);
            return;
        }
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
                <button on:click=move |_| node_list_open.set(true)>
                    "Список нод"
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
                on:dragover=move |ev: web_sys::DragEvent| {
                    ev.prevent_default();
                }
                on:drop=move |ev: web_sys::DragEvent| {
                    ev.prevent_default();
                    let Some(dt) = ev.data_transfer() else { return };
                    let Some(files) = dt.files() else { return };
                    let (cx, cy) = screen_to_canvas(ev.client_x(), ev.client_y());
                    let mut drop_items: Vec<(InputNodeKind, web_sys::File, Position)> = Vec::new();
                    for i in 0..files.length() {
                        let Some(file) = files.item(i) else { continue };
                        let mime = file.type_();
                        let name = file.name().to_lowercase();
                        let kind = if mime.starts_with("video/") || name.ends_with(".mov") || name.ends_with(".mp4") || name.ends_with(".avi") || name.ends_with(".mkv") {
                            InputNodeKind::Video
                        } else if mime.starts_with("audio/") || name.ends_with(".mp3") || name.ends_with(".wav") || name.ends_with(".aac") || name.ends_with(".ogg") {
                            InputNodeKind::Audio
                        } else if mime.starts_with("image/") || name.ends_with(".png") || name.ends_with(".jpg") || name.ends_with(".jpeg") || name.ends_with(".webp") {
                            InputNodeKind::Image
                        } else {
                            continue;
                        };
                        let pos = Position { x: cx, y: cy + i as f32 * 300.0 };
                        drop_items.push((kind, file, pos));
                    }
                    if !drop_items.is_empty() {
                        let Some(pid) = project_id.get_untracked() else { return };
                        spawn_local(async move {
                            for (kind, file, pos) in drop_items {
                                let node_kind = NodeKind::Input(kind);
                                match project_service::create_node(pid, node_kind, pos).await {
                                    Ok(out) => {
                                        let node_id = out.node.id;
                                        if let Err(e) = upload_service::upload_file(pid, node_id, kind, file, |_| {}).await {
                                            error.set(Some(e.to_string()));
                                        }
                                    }
                                    Err(e) => { error.set(Some(e.to_string())); break; }
                                }
                            }
                            reload();
                        });
                    }
                }
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
                <svg class="edges-layer">
                    {move || {
                        let _ns = nodes.get();
                        let es = edges.get();
                        let _dp = drag_pos.get();
                        let _cm = cam.get(); // re-trigger on pan/zoom
                        let canvas_el = canvas_ref.get();
                        es.iter().filter_map(|edge| {
                            let out_id = handle_id("out", edge.from_node, &edge.from_port);
                            let in_id = handle_id("in", edge.to_node, &edge.to_port);
                            let (x1, y1) = handle_center(&canvas_el, cam, &out_id)?;
                            let (x2, y2) = handle_center(&canvas_el, cam, &in_id)?;
                            let cpx = (x1 + x2) / 2.0;
                            let d = format!("M{x1},{y1} C{cpx},{y1} {cpx},{y2} {x2},{y2}");
                            let from_id = edge.from_node;
                            let from_p = edge.from_port.clone();
                            let to_id = edge.to_node;
                            let to_p = edge.to_port.clone();
                            Some(view! {
                                <path
                                    class="edge-line-hit"
                                    d=d.clone()
                                    on:click=move |ev: MouseEvent| {
                                        ev.stop_propagation();
                                        on_disconnect(from_id, from_p.clone(), to_id, to_p.clone());
                                    }
                                />
                                <path class="edge-line" d=d/>
                            })
                        }).collect_view()
                    }}
                    // Temp connection line while dragging
                    {move || {
                        let _from_id = connecting_from.get()?;
                        let (mx, my) = connect_mouse.get()?;
                        let canvas_el = canvas_ref.get();
                        let from_port = connecting_from_port.get_untracked();
                        let (x1, y1) = handle_center(&canvas_el, cam, &handle_id("out", _from_id, &from_port))?;
                        let cpx = (x1 + mx) / 2.0;
                        let d = format!("M{x1},{y1} C{cpx},{y1} {cpx},{my} {mx},{my}");
                        Some(view! { <path class="edge-line temp" d=d/> })
                    }}
                </svg>
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
                                edges=edges
                                connecting_from=connecting_from
                                connecting_from_port=connecting_from_port
                                player_src=player_src
                                json_modal=json_modal
                                on_drag_start=start_drag
                                on_delete=on_delete_node
                                on_connect_complete=on_connect_complete
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

                {move || {
                    let kind = placing_kind.get()?;
                    let (cx, cy) = placing_pos.get()?;
                    let label = kind_label(kind);
                    Some(view! {
                        <div
                            class="node ghost"
                            style=format!("left: {}px; top: {}px;", cx, cy)
                        >
                            <div class="node-header">
                                <span class="node-kind-badge">{label}</span>
                            </div>
                        </div>
                    })
                }}
                </div>
            </div>

            <Show when=move || add_modal_open.get()>
                <AddNodeModal
                    on_select=on_create_node
                    on_close=move || add_modal_open.set(false)
                />
            </Show>

            {move || player_src.get().map(|src| view! {
                <VideoPlayerModal
                    src=src
                    on_close=move || player_src.set(None)
                />
            })}

            {move || json_modal.get().map(|(url, label)| view! {
                <JsonModal url=url label=label on_close=move || json_modal.set(None) />
            })}

            {move || node_list_open.get().then(|| {
                let on_del = move |ids: Vec<Uuid>| {
                    let Some(pid) = project_id.get_untracked() else { return };
                    spawn_local(async move {
                        for id in ids {
                            let _ = project_service::delete_node(pid, id).await;
                        }
                        reload();
                    });
                    node_list_open.set(false);
                };
                view! {
                    <NodeListModal
                        nodes=nodes
                        on_delete=on_del
                        on_close=move || node_list_open.set(false)
                    />
                }
            })}
        </div>
    }
}

#[component]
fn AddNodeModal(
    on_select: impl Fn(NodeKind) + Copy + 'static,
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
                    <button
                        class:active=move || active_tab.get() == 1
                        on:click=move |_| active_tab.set(1)
                    >"Обработка"</button>
                    <button
                        class:active=move || active_tab.get() == 2
                        on:click=move |_| active_tab.set(2)
                    >"Композиция"</button>
                </div>
                <div class="modal-body">
                    <Show when=move || active_tab.get() == 0>
                        <div class="node-type-grid">
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Input(InputNodeKind::Video))>
                                <div class="node-type-icon">"🎬"</div>
                                <div class="node-type-label">"Видео"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Input(InputNodeKind::Audio))>
                                <div class="node-type-icon">"🔊"</div>
                                <div class="node-type-label">"Аудио"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Input(InputNodeKind::Image))>
                                <div class="node-type-icon">"🖼"</div>
                                <div class="node-type-label">"Изображение"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Input(InputNodeKind::VideoArray))>
                                <div class="node-type-icon">"📁"</div>
                                <div class="node-type-label">"Видео (массив)"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::Scalar))>
                                <div class="node-type-icon">"🔢"</div>
                                <div class="node-type-label">"Число"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::Spline))>
                                <div class="node-type-icon">"📈"</div>
                                <div class="node-type-label">"Сплайн"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::MathAdd))>
                                <div class="node-type-icon">"+"</div>
                                <div class="node-type-label">"A + B"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::MathSubtract))>
                                <div class="node-type-icon">"-"</div>
                                <div class="node-type-label">"A - B"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::MathMultiply))>
                                <div class="node-type-icon">"×"</div>
                                <div class="node-type-label">"A × B"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::MathDivide))>
                                <div class="node-type-icon">"÷"</div>
                                <div class="node-type-label">"A ÷ B"</div>
                            </button>
                        </div>
                    </Show>
                    <Show when=move || active_tab.get() == 1>
                        <div class="node-type-grid">
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::ExtractAudio))>
                                <div class="node-type-icon">"🎵"</div>
                                <div class="node-type-label">"Извлечь аудио"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::DetectSilence))>
                                <div class="node-type-icon">"🔇"</div>
                                <div class="node-type-label">"Детекция тишины"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::DetectSubtitles))>
                                <div class="node-type-icon">"💬"</div>
                                <div class="node-type-label">"Субтитры"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::Map))>
                                <div class="node-type-icon">"🔄"</div>
                                <div class="node-type-label">"Map"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::Reduce))>
                                <div class="node-type-icon">"📊"</div>
                                <div class="node-type-label">"Reduce"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::TrimVideo))>
                                <div class="node-type-icon">"✂️"</div>
                                <div class="node-type-label">"Обрезка видео"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::SpeechBounds))>
                                <div class="node-type-icon">"📐"</div>
                                <div class="node-type-label">"Края речи"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::TrimAudio))>
                                <div class="node-type-icon">"✂️"</div>
                                <div class="node-type-label">"Обрезка аудио"</div>
                            </button>
                        </div>
                    </Show>
                    <Show when=move || active_tab.get() == 2>
                        <div class="node-type-grid">
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::Clip))>
                                <div class="node-type-icon">"🎞"</div>
                                <div class="node-type-label">"Клип"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::Mux))>
                                <div class="node-type-icon">"🎬"</div>
                                <div class="node-type-label">"Композитор"</div>
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
    edges: RwSignal<Vec<Edge>>,
    connecting_from: RwSignal<Option<Uuid>>,
    connecting_from_port: RwSignal<String>,
    player_src: RwSignal<Option<String>>,
    json_modal: RwSignal<Option<(String, &'static str)>>,
    on_drag_start: impl Fn(Uuid, MouseEvent) + Copy + 'static,
    on_delete: impl Fn(Uuid) + Copy + 'static,
    on_connect_complete: impl Fn(Uuid, String, Uuid, String) + Copy + 'static,
    on_uploaded: impl Fn(Node) + Copy + 'static,
    on_upload_error: impl Fn(String) + Copy + 'static,
) -> impl IntoView {
    let node_signal = create_rw_signal(node);
    let upload_progress = create_rw_signal::<Option<(u64, u64)>>(None);
    let confirm_delete = create_rw_signal(false);
    let last_drag_pos = create_rw_signal::<Option<Position>>(None);
    let active_task_id = create_rw_signal::<Option<Uuid>>(None);

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
        let NodeKind::Input(kind) = n.kind else { return };
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

    let is_process = matches!(node_signal.get_untracked().kind, NodeKind::Process(_));
    let has_multi_inputs = {
        let n = node_signal.get_untracked();
        if let NodeKind::Process(pk) = n.kind {
            pk.input_ports_with_settings(n.settings.as_ref()).len() > 1
        } else { false }
    };
    let has_multi_outputs = {
        let n = node_signal.get_untracked();
        n.kind.output_ports().len() > 1
    };
    let missing_ports = Signal::derive(move || {
        let n = node_signal.get();
        let NodeKind::Process(pk) = n.kind else { return vec![] };
        let required = pk.required_input_ports();
        let connected: Vec<String> = edges.get().iter()
            .filter(|e| e.to_node == id_for_drag)
            .map(|e| e.to_port.clone())
            .collect();
        required.into_iter().filter(|p| !connected.contains(p)).collect::<Vec<_>>()
    });
    let all_required_connected = Signal::derive(move || missing_ports.get().is_empty());

    let on_run = move || {
        spawn_local(async move {
            match project_service::run_node(project_id, id_for_drag).await {
                Ok(out) => {
                    active_task_id.set(Some(out.task_id));
                    node_signal.update(|n| n.task_status = Some(TaskStatus::Queued));
                    poll_task(active_task_id, node_signal, out.task_id, project_id);
                }
                Err(e) => on_upload_error(e.to_string()),
            }
        });
    };

    view! {
        <div
            class=move || {
                let mut cls = if is_process { "node process".to_string() } else { "node".to_string() };
                if has_multi_inputs { cls.push_str(" multi-inputs"); }
                if has_multi_outputs { cls.push_str(" multi-outputs"); }
                cls
            }
            style=move || {
                let pos = drag_pos
                    .get()
                    .filter(|(id, _)| *id == id_for_drag)
                    .map(|(_, p)| p)
                    .unwrap_or(node_signal.get().position);
                format!("left: {}px; top: {}px;", pos.x, pos.y)
            }
        >
            {is_process.then(move || {
                let n = node_signal.get_untracked();
                let NodeKind::Process(pk) = n.kind else { return ().into_view() };
                let ports = pk.input_ports_with_settings(n.settings.as_ref());
                if ports.is_empty() {
                    return ().into_view();
                }
                if ports.len() <= 1 {
                    let port_name = ports.first().map(|p| p.name.clone()).unwrap_or_default();
                    let hid = handle_id("in", id_for_drag, &port_name);
                    let pn = port_name.clone();
                    view! {
                        <div
                            class="input-handle"
                            id=hid
                            on:mouseup=move |ev: MouseEvent| {
                                ev.stop_propagation();
                                if let Some(from) = connecting_from.get_untracked() {
                                    let from_port = connecting_from_port.get_untracked();
                                    on_connect_complete(from, from_port, id_for_drag, pn.clone());
                                }
                            }
                        ></div>
                    }.into_view()
                } else {
                    view! {
                        <div class="input-handles">
                            {ports.into_iter().map(|port| {
                                let hid = handle_id("in", id_for_drag, &port.name);
                                let pn = port.name.clone();
                                let label = port.name.clone();
                                view! {
                                    <div class="input-handle-row">
                                        <div
                                            class="input-handle multi"
                                            id=hid
                                            on:mouseup=move |ev: MouseEvent| {
                                                ev.stop_propagation();
                                                if let Some(from) = connecting_from.get_untracked() {
                                                    let from_port = connecting_from_port.get_untracked();
                                                    on_connect_complete(from, from_port, id_for_drag, pn.clone());
                                                }
                                            }
                                        ></div>
                                        <span class="port-label">{label}</span>
                                    </div>
                                }
                            }).collect_view()}
                            {matches!(pk, ProcessNodeKind::Mux).then(|| {
                                let cur_clips = match &node_signal.get_untracked().settings {
                                    Some(api_types::NodeSettings::Mux { num_clips, .. }) => *num_clips,
                                    _ => 1,
                                };
                                let cur_fps = match &node_signal.get_untracked().settings {
                                    Some(api_types::NodeSettings::Mux { fps, .. }) => *fps,
                                    _ => 30,
                                };
                                view! {
                                    <div
                                        class="add-port-btn"
                                        on:click=move |ev: MouseEvent| {
                                            ev.stop_propagation();
                                            let new_n = cur_clips + 1;
                                            let settings = api_types::NodeSettings::Mux { num_clips: new_n, fps: cur_fps };
                                            node_signal.update(|n| n.settings = Some(settings.clone()));
                                            spawn_local(async move {
                                                let _ = project_service::update_node_settings(
                                                    project_id, id_for_drag, settings
                                                ).await;
                                            });
                                        }
                                    >
                                        "+"
                                    </div>
                                }
                            })}
                        </div>
                    }.into_view()
                }
            })}
            <div
                class="node-header"
                on:mousedown=move |ev: MouseEvent| {
                    ev.prevent_default();
                    on_drag_start(id_for_drag, ev);
                }
            >
                <span class="node-kind-badge">{move || kind_label(node_signal.get().kind)}</span>
                <div class="spacer"></div>
                {is_process.then(|| view! {
                    <button class="header-btn refresh" title="Принудительно пересчитать"
                        on:click=move |ev: MouseEvent| {
                            ev.stop_propagation();
                            spawn_local(async move {
                                match project_service::invalidate_node(project_id, id_for_drag).await {
                                    Ok(out) => {
                                        if out.task_id != uuid::Uuid::nil() {
                                            active_task_id.set(Some(out.task_id));
                                            node_signal.update(|n| {
                                                n.output = None;
                                                n.task_status = Some(TaskStatus::Queued);
                                            });
                                            poll_task(active_task_id, node_signal, out.task_id, project_id);
                                        }
                                    }
                                    Err(e) => on_upload_error(e.to_string()),
                                }
                            });
                        }
                    >"↻"</button>
                })}
                <div style="position: relative;">
                    <button class="header-btn delete" on:click=move |ev: MouseEvent| {
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
            <div class="node-body" style=move || {
                if has_multi_inputs {
                    let n = node_signal.get();
                    let num_ports = if let NodeKind::Process(pk) = n.kind {
                        pk.input_ports_with_settings(n.settings.as_ref()).len()
                    } else { 0 };
                    // ~20px per port row
                    format!("min-height: {}px;", num_ports * 20)
                } else {
                    String::new()
                }
            }>
                {move || {
                    let n = node_signal.get();
                    match n.kind {
                        NodeKind::Input(InputNodeKind::VideoArray) => {
                            let count = n.assets.len();
                            view! {
                                <div class="video-array-count">
                                    {format!("{} видео", count)}
                                </div>
                                <label class="video-array-add">
                                    "+"
                                    <input type="file" accept="video/*" multiple=true style="display:none"
                                        on:change=move |ev: Event| {
                                            let target = ev.target().and_then(|t| t.dyn_into::<HtmlInputElement>().ok());
                                            let Some(input_el) = target else { return };
                                            let Some(files) = input_el.files() else { return };
                                            let pid = project_id;
                                            let nid = id_for_drag;
                                            let mut file_list = Vec::new();
                                            for i in 0..files.length() {
                                                if let Some(f) = files.item(i) { file_list.push(f); }
                                            }
                                            spawn_local(async move {
                                                for file in file_list {
                                                    if let Err(e) = upload_service::upload_file(pid, nid, InputNodeKind::Video, file, |_| {}).await {
                                                        on_upload_error(e.to_string());
                                                        break;
                                                    }
                                                }
                                                // Refresh node from server
                                                if let Ok(proj) = project_service::get_project(pid).await {
                                                    if let Some(updated) = proj.project.nodes.iter().find(|n| n.id == nid) {
                                                        node_signal.set(updated.clone());
                                                    }
                                                }
                                            });
                                        }
                                    />
                                </label>
                            }.into_view()
                        }
                        NodeKind::Input(_) => {
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
                                    if let NodeKind::Input(k) = n.kind {
                                        view! { <UploadInput kind=k on_file=on_file_picked/> }.into_view()
                                    } else {
                                        ().into_view()
                                    }
                                },
                            }
                        }
                        NodeKind::Process(ProcessNodeKind::Scalar) => {
                            // Inline number editor — no "Обновить" button
                            let current_val = match &n.settings {
                                Some(api_types::NodeSettings::Scalar { value }) => *value,
                                _ => 0.0,
                            };
                            let val_str = create_rw_signal(format!("{}", current_val));
                            view! {
                                <input
                                    type="text"
                                    class="scalar-input"
                                    prop:value=move || val_str.get()
                                    on:input=move |ev| val_str.set(event_target_value(&ev))
                                    on:change=move |_| {
                                        let text = val_str.get_untracked();
                                        if let Ok(v) = text.parse::<f64>() {
                                            let settings = api_types::NodeSettings::Scalar { value: v };
                                            node_signal.update(|n| n.settings = Some(settings.clone()));
                                            spawn_local(async move {
                                                let _ = project_service::update_node_settings(
                                                    project_id, id_for_drag, settings
                                                ).await;
                                                let _ = project_service::run_node(project_id, id_for_drag).await;
                                            });
                                        }
                                    }
                                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                                        if ev.key() == "Enter" {
                                            ev.prevent_default();
                                            let target = ev.target().unwrap();
                                            let el = target.unchecked_ref::<web_sys::HtmlElement>();
                                            el.blur().ok();
                                        }
                                    }
                                />
                            }.into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::Spline) => {
                            // Spline just shows "Готово" or "Обновить" via normal flow
                            // but auto-runs on settings change (no manual button needed for now)
                            let has_output = n.output.is_some();
                            let needs_update = n.needs_update;
                            if has_output && !needs_update {
                                view! { <div class="process-done">"Готово"</div> }.into_view()
                            } else {
                                // Auto-run spline
                                let nid = n.id;
                                spawn_local(async move {
                                    let _ = project_service::run_node(project_id, nid).await;
                                });
                                view! { <div class="process-hint">"Вычисление..."</div> }.into_view()
                            }
                        }
                        NodeKind::Process(pk) => {
                            let status = n.task_status;
                            let has_output = n.output.is_some();
                            let needs_update = n.needs_update;
                            let missing = missing_ports.get();
                            let all_connected = missing.is_empty();
                            let no_inputs_needed = !pk.has_inputs();

                            let can_run = all_connected || no_inputs_needed;
                            let show_update = can_run && (needs_update || !has_output);

                            let status_view = if !can_run {
                                let names = missing.join(", ");
                                view! { <div class="process-hint">{format!("Подключите: {}", names)}</div> }.into_view()
                            } else if let Some(TaskStatus::Running { progress_pct }) = status {
                                view! {
                                    <div class="process-progress">
                                        <div class="filename">{format!("Обработка... {}%", progress_pct)}</div>
                                        <div class="progress">
                                            <div style=format!("width: {}%;", progress_pct)></div>
                                        </div>
                                    </div>
                                }.into_view()
                            } else if let Some(TaskStatus::Queued) = status {
                                view! { <div class="process-hint">"В очереди..."</div> }.into_view()
                            } else if let Some(TaskStatus::Failed) = status {
                                view! {
                                    <div class="process-hint error-text">"Ошибка обработки"</div>
                                    <button class="run-btn" on:click=move |_| on_run()>"Повторить"</button>
                                }.into_view()
                            } else if show_update {
                                view! {
                                    {(has_output && needs_update).then(|| view! { <div class="process-hint">"Результат устарел"</div> })}
                                    <button class="run-btn" on:click=move |_| on_run()>"Обновить"</button>
                                }.into_view()
                            } else {
                                ().into_view()
                            };

                            let output_view = if has_output && !needs_update {
                                let slug = pk.url_slug();
                                let cache_bust = n.output.as_ref()
                                    .map(|o| format!("{:x}-{}", {
                                        let mut h: u64 = 0;
                                        for b in o.cache_key.bytes() { h = h.wrapping_mul(31).wrapping_add(b as u64); }
                                        h
                                    }, o.size_bytes))
                                    .unwrap_or_default();
                                let out_kind = pk.produced_output();

                                match out_kind {
                                    api_types::NodeOutputKind::Video => {
                                        let thumb_url = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/thumbnail?v={}",
                                            project_id, slug, n.id, cache_bust
                                        ));
                                        let file_src = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/file?v={}",
                                            project_id, slug, n.id, cache_bust
                                        ));
                                        let loop_base = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/loop-clip?v={}",
                                            project_id, slug, n.id, cache_bust
                                        ));
                                        view! {
                                            <VideoPlayer thumb_url=thumb_url file_url=file_src loop_clip_base=loop_base />
                                        }.into_view()
                                    }
                                    api_types::NodeOutputKind::Audio => {
                                        let wave_url = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/thumbnail?v={}",
                                            project_id, slug, n.id, cache_bust
                                        ));
                                        let file_src = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/file?v={}",
                                            project_id, slug, n.id, cache_bust
                                        ));
                                        let loop_base = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/loop-clip?v={}",
                                            project_id, slug, n.id, cache_bust
                                        ));
                                        view! {
                                            <AudioPlayer wave_url=wave_url file_url=file_src loop_clip_base=loop_base />
                                        }.into_view()
                                    }
                                    api_types::NodeOutputKind::Json => {
                                        match pk {
                                    ProcessNodeKind::Clip => {
                                        // Clip has video preview despite Json output
                                        let file_src = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/file?v={}",
                                            project_id, slug, n.id, cache_bust
                                        ));
                                        let thumb_url = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/thumbnail?v={}",
                                            project_id, slug, n.id, cache_bust
                                        ));
                                        let loop_base = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/loop-clip?v={}",
                                            project_id, slug, n.id, cache_bust
                                        ));
                                        view! {
                                            <VideoPlayer thumb_url=thumb_url file_url=file_src loop_clip_base=loop_base />
                                        }.into_view()
                                    }
                                    ProcessNodeKind::MathAdd | ProcessNodeKind::MathSubtract
                                    | ProcessNodeKind::MathMultiply | ProcessNodeKind::MathDivide => {
                                        let file_url = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/file",
                                            project_id, slug, n.id
                                        ));
                                        let result_val = create_rw_signal("...".to_string());
                                        {
                                            let file_url = file_url.clone();
                                            spawn_local(async move {
                                                let window = web_sys::window().unwrap();
                                                if let Ok(resp) = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(&file_url)).await {
                                                    let resp: web_sys::Response = resp.unchecked_into();
                                                    if let Ok(text_p) = resp.text() {
                                                        if let Ok(text_v) = wasm_bindgen_futures::JsFuture::from(text_p).await {
                                                            if let Some(s) = text_v.as_string() {
                                                                if let Ok(parsed) = js_sys::JSON::parse(&s) {
                                                                    let val = js_sys::Reflect::get(&parsed, &"value".into()).ok();
                                                                    if let Some(v) = val.and_then(|v| v.as_f64()) {
                                                                        result_val.set(format!("{}", v));
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            });
                                        }
                                        view! {
                                            <div class="math-result">{move || result_val.get()}</div>
                                        }.into_view()
                                    }
                                    _ => {
                                        let json_url = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/file",
                                            project_id, slug, n.id
                                        ));
                                        let label = kind_label(n.kind);
                                        view! {
                                            <div class="json-output">
                                                <div class="process-done">"Готово"</div>
                                                <button class="run-btn" style="background: var(--accent);"
                                                    on:click=move |_| json_modal.set(Some((json_url.clone(), label)))
                                                >"Посмотреть"</button>
                                            </div>
                                        }.into_view()
                                    }
                                }
                                    }
                                    api_types::NodeOutputKind::Image => {
                                        view! { <div class="process-done">"Готово"</div> }.into_view()
                                    }
                                }
                            } else {
                                ().into_view()
                            };

                            view! {
                                {output_view}
                                {status_view}
                            }.into_view()
                        }
                    }
                }}
            </div>
            {move || {
                let n = node_signal.get();
                let ports = match n.kind {
                    _ => n.kind.output_ports(),
                };
                if ports.len() <= 1 {
                    let port_name = ports.first().map(|p| p.name.clone()).unwrap_or_default();
                    let hid = handle_id("out", id_for_drag, &port_name);
                    let pn = port_name.clone();
                    view! {
                        <div
                            class="output-handle"
                            id=hid
                            on:mousedown=move |ev: MouseEvent| {
                                ev.stop_propagation();
                                ev.prevent_default();
                                connecting_from.set(Some(id_for_drag));
                                connecting_from_port.set(pn.clone());
                            }
                        ></div>
                    }.into_view()
                } else {
                    view! {
                        <div class="output-handles">
                            {ports.into_iter().enumerate().map(|(i, port)| {
                                let hid = handle_id("out", id_for_drag, &port.name);
                                let pn = port.name.clone();
                                let label = port.name.clone();
                                view! {
                                    <div class="output-handle-row">
                                        <span class="port-label">{label}</span>
                                        <div
                                            class="output-handle multi"
                                            id=hid
                                            on:mousedown=move |ev: MouseEvent| {
                                                ev.stop_propagation();
                                                ev.prevent_default();
                                                connecting_from.set(Some(id_for_drag));
                                                connecting_from_port.set(pn.clone());
                                            }
                                        ></div>
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    }.into_view()
                }
            }}
        </div>
    }
}

#[component]
fn VideoPlayer(
    thumb_url: String,
    file_url: String,
    loop_clip_base: String,
) -> impl IntoView {
    let playing = create_rw_signal(false);
    let playhead_pct = create_rw_signal(0.0_f64);
    let video_ref = create_node_ref::<html::Video>();

    let selection = create_rw_signal::<Option<(f64, f64)>>(None);
    let selecting_from = create_rw_signal::<Option<f64>>(None);

    let on_thumb_mousedown = move |ev: MouseEvent| {
        ev.prevent_default();
        let target = ev.target().unwrap();
        let el = target.unchecked_ref::<web_sys::HtmlElement>();
        let rect = el.get_bounding_client_rect();
        let pct = ((ev.client_x() as f64 - rect.left()) / rect.width()).clamp(0.0, 1.0);
        selecting_from.set(Some(pct));
        selection.set(None);
        if playing.get_untracked() {
            if let Some(v) = video_ref.get_untracked() {
                let el: &web_sys::HtmlMediaElement = v.unchecked_ref();
                el.pause().ok();
                playing.set(false);
            }
        }
    };

    let on_thumb_mousemove = move |ev: MouseEvent| {
        let Some(start) = selecting_from.get_untracked() else { return };
        let target = ev.target().unwrap();
        let el = target.unchecked_ref::<web_sys::HtmlElement>();
        let rect = el.get_bounding_client_rect();
        let pct = ((ev.client_x() as f64 - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let (a, b) = if pct < start { (pct, start) } else { (start, pct) };
        if (b - a) > 0.01 { selection.set(Some((a, b))); }
    };

    let on_thumb_mouseup = move |_| { selecting_from.set(None); };

    let toggle = {
        let file_url = file_url.clone();
        let loop_clip_base = loop_clip_base.clone();
        move |_| {
            let Some(video) = video_ref.get_untracked() else { return };
            let el: web_sys::HtmlMediaElement = video.unchecked_ref::<web_sys::HtmlMediaElement>().clone();
            if playing.get_untracked() {
                el.pause().ok();
                playing.set(false);
                return;
            }
            // If already has src (paused) — just resume
            let current_src = el.src();
            let want_src = if let Some((a, b)) = selection.get_untracked() {
                format!("{}&start={:.4}&end={:.4}", loop_clip_base, a, b)
            } else {
                file_url.clone()
            };
            let need_reload = current_src.is_empty() || !current_src.ends_with(&want_src);
            if need_reload {
                el.set_src(&want_src);
                el.set_loop(selection.get_untracked().is_some());
                el.load();
            }
            el.set_volume(1.0);
            el.set_muted(false);
            playing.set(true);
            if let Ok(p) = el.play() {
                spawn_local(async move {
                    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
                });
            }
        }
    };

    let on_timeupdate = move |_| {
        let Some(video) = video_ref.get_untracked() else { return };
        let el: &web_sys::HtmlMediaElement = video.unchecked_ref();
        let dur = el.duration();
        let cur = el.current_time();
        if dur.is_finite() && dur > 0.0 {
            if let Some((a, b)) = selection.get_untracked() {
                playhead_pct.set((a + (cur / dur) * (b - a)) * 100.0);
            } else {
                playhead_pct.set(cur / dur * 100.0);
            }
        }
    };

    let on_ended = move |_| { playing.set(false); playhead_pct.set(0.0); };

    let clear_selection = move |ev: MouseEvent| {
        ev.stop_propagation();
        selection.set(None);
        if playing.get_untracked() {
            if let Some(v) = video_ref.get_untracked() {
                let el: &web_sys::HtmlMediaElement = v.unchecked_ref();
                el.pause().ok();
                playing.set(false);
            }
        }
    };

    view! {
        <div class="video-player" class:playing=move || playing.get()>
            <div
                class="video-thumb-wrap"
                on:mousedown=on_thumb_mousedown
                on:mousemove=on_thumb_mousemove
                on:mouseup=on_thumb_mouseup
                on:mouseleave=move |_| selecting_from.set(None)
            >
                <video
                    node_ref=video_ref
                    class="node-video"
                    poster=thumb_url
                    src=file_url
                    preload="metadata"
                    on:timeupdate=on_timeupdate
                    on:ended=on_ended
                />
                {move || selection.get().map(|(a, b)| {
                    let left = format!("{:.2}%", a * 100.0);
                    let width = format!("{:.2}%", (b - a) * 100.0);
                    view! { <div class="selection-highlight" style:left=left style:width=width></div> }
                })}
                <div class="playhead" style=move || format!("left: {:.2}%;", playhead_pct.get())></div>
            </div>
            <div class="audio-controls">
                <button class="play-btn-inline" on:click=toggle>
                    {move || if playing.get() {
                        view! {
                            <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
                                <rect x="6" y="4" width="4" height="16"/>
                                <rect x="14" y="4" width="4" height="16"/>
                            </svg>
                        }.into_view()
                    } else {
                        view! {
                            <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
                                <polygon points="6,3 20,12 6,21"/>
                            </svg>
                        }.into_view()
                    }}
                </button>
                {move || selection.get().map(|_| view! {
                    <button class="clear-sel-btn" on:click=clear_selection title="Снять выделение">"✕"</button>
                })}
            </div>
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
        InputNodeKind::VideoArray => "video/*",
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

fn load_cam(key: &str) -> Option<CanvasTransform> {
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

fn handle_id(dir: &str, node_id: Uuid, port: &str) -> String {
    if port.is_empty() {
        format!("handle-{dir}-{node_id}")
    } else {
        format!("handle-{dir}-{node_id}-{port}")
    }
}

fn handle_center(
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

fn file_url(project_id: Uuid, node_id: Uuid, kind: InputNodeKind) -> String {
    let slug = kind.url_slug();
    absolute_url(&format!(
        "/api/projects/{project_id}/nodes/{slug}/{node_id}/file"
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
            let thumb = thumbnail_url(project_id, node_id, kind);
            let file = file_url(project_id, node_id, kind);
            let slug = kind.url_slug();
            let loop_base = absolute_url(&format!(
                "/api/projects/{project_id}/nodes/{slug}/{node_id}/loop-clip?v=0"
            ));
            view! {
                <VideoPlayer thumb_url=thumb file_url=file loop_clip_base=loop_base />
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
            let wave_url = thumbnail_url(project_id, node_id, kind);
            let audio_src = file_url(project_id, node_id, kind);
            let slug = kind.url_slug();
            let loop_base = absolute_url(&format!(
                "/api/projects/{project_id}/nodes/{slug}/{node_id}/loop-clip?v=0"
            ));
            view! {
                <AudioPlayer wave_url=wave_url file_url=audio_src loop_clip_base=loop_base />
                <div class="meta-row">
                    <span>{original}</span>
                    <span>{size}</span>
                </div>
            }.into_view()
        }
        InputNodeKind::VideoArray => {
            // Should not reach here — VideoArray uses assets vec, not single asset
            ().into_view()
        }
    }
}

#[component]
fn VideoPlayerModal(
    src: String,
    on_close: impl Fn() + Copy + 'static,
) -> impl IntoView {
    view! {
        <div class="modal-backdrop player-backdrop" on:click=move |_| on_close()>
            <div class="player-modal" on:click=|ev| ev.stop_propagation()>
                <div class="player-header">
                    <button class="modal-close" on:click=move |_| on_close()>"✕"</button>
                </div>
                <video
                    class="player-video"
                    src=src
                    controls=true
                    autoplay=true
                />
            </div>
        </div>
    }
}

#[component]
fn AudioPlayer(
    wave_url: String,
    file_url: String,
    loop_clip_base: String,
) -> impl IntoView {
    let playing = create_rw_signal(false);
    let playhead_pct = create_rw_signal(0.0_f64);
    let audio_ref = create_node_ref::<html::Audio>();

    // Selection: (start_pct, end_pct) in 0..1
    let selection = create_rw_signal::<Option<(f64, f64)>>(None);
    let selecting_from = create_rw_signal::<Option<f64>>(None);

    let on_wave_mousedown = move |ev: MouseEvent| {
        ev.prevent_default();
        let target = ev.target().unwrap();
        let el = target.unchecked_ref::<web_sys::HtmlElement>();
        let rect = el.get_bounding_client_rect();
        let pct = ((ev.client_x() as f64 - rect.left()) / rect.width()).clamp(0.0, 1.0);
        selecting_from.set(Some(pct));
        selection.set(None);
        // Stop playback on new selection
        if playing.get_untracked() {
            if let Some(audio) = audio_ref.get_untracked() {
                let el: &web_sys::HtmlMediaElement = audio.unchecked_ref();
                el.pause().ok();
                playing.set(false);
            }
        }
    };

    let on_wave_mousemove = move |ev: MouseEvent| {
        let Some(start) = selecting_from.get_untracked() else { return };
        let target = ev.target().unwrap();
        let el = target.unchecked_ref::<web_sys::HtmlElement>();
        let rect = el.get_bounding_client_rect();
        let pct = ((ev.client_x() as f64 - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let (a, b) = if pct < start { (pct, start) } else { (start, pct) };
        if (b - a) > 0.01 {
            selection.set(Some((a, b)));
        }
    };

    let on_wave_mouseup = move |_ev: MouseEvent| {
        selecting_from.set(None);
    };

    let toggle = {
        let file_url = file_url.clone();
        let loop_clip_base = loop_clip_base.clone();
        move |_| {
            let Some(audio) = audio_ref.get_untracked() else { return };
            let el: web_sys::HtmlMediaElement = audio.unchecked_ref::<web_sys::HtmlMediaElement>().clone();
            if playing.get_untracked() {
                el.pause().ok();
                playing.set(false);
                return;
            }
            let needs_new_src = if let Some((a, b)) = selection.get_untracked() {
                let clip_url = format!(
                    "{}&start={:.4}&end={:.4}",
                    loop_clip_base, a, b
                );
                el.set_src(&clip_url);
                el.set_loop(true);
                true
            } else {
                let full = &file_url;
                if !el.src().ends_with(full.split('?').next().unwrap_or(full)) {
                    el.set_src(full);
                    el.set_loop(false);
                    true
                } else {
                    el.set_current_time(0.0);
                    false
                }
            };
            playing.set(true);
            if needs_new_src {
                // Wait for the new source to load, then play
                spawn_local(async move {
                    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
                        let el2 = el.clone();
                        let cb = wasm_bindgen::closure::Closure::once(move || {
                            el2.set_oncanplaythrough(None);
                            resolve.call0(&wasm_bindgen::JsValue::NULL).ok();
                        });
                        el.set_oncanplaythrough(Some(cb.as_ref().unchecked_ref()));
                        cb.forget();
                    });
                    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                    let audio2 = audio_ref.get_untracked();
                    if let Some(a) = audio2 {
                        let m: &web_sys::HtmlMediaElement = a.unchecked_ref();
                        let _ = wasm_bindgen_futures::JsFuture::from(
                            m.play().unwrap()
                        ).await;
                    }
                });
            } else {
                let play_promise = el.play().unwrap();
                spawn_local(async move {
                    let _ = wasm_bindgen_futures::JsFuture::from(play_promise).await;
                });
            }
        }
    };

    let on_timeupdate = move |_| {
        let Some(audio) = audio_ref.get_untracked() else { return };
        let el: &web_sys::HtmlMediaElement = audio.unchecked_ref();
        let dur = el.duration();
        let cur = el.current_time();
        if dur.is_finite() && dur > 0.0 {
            if let Some((a, b)) = selection.get_untracked() {
                // Map clip time to selection position
                let sel_pct = a + (cur / dur) * (b - a);
                playhead_pct.set(sel_pct * 100.0);
            } else {
                playhead_pct.set(cur / dur * 100.0);
            }
        }
    };

    let on_ended = move |_| {
        playing.set(false);
        playhead_pct.set(0.0);
    };

    let clear_selection = move |ev: MouseEvent| {
        ev.stop_propagation();
        selection.set(None);
        if playing.get_untracked() {
            if let Some(audio) = audio_ref.get_untracked() {
                let el: &web_sys::HtmlMediaElement = audio.unchecked_ref();
                el.pause().ok();
                playing.set(false);
            }
        }
    };

    view! {
        <div class="audio-player" class:playing=move || playing.get()>
            <div
                class="waveform-wrap"
                on:mousedown=on_wave_mousedown
                on:mousemove=on_wave_mousemove
                on:mouseup=on_wave_mouseup
                on:mouseleave=move |_| selecting_from.set(None)
            >
                <img class="media-waveform" src=wave_url draggable="false"/>
                {move || selection.get().map(|(a, b)| {
                    let left = format!("{:.2}%", a * 100.0);
                    let width = format!("{:.2}%", (b - a) * 100.0);
                    view! {
                        <div class="selection-highlight" style:left=left style:width=width></div>
                    }
                })}
                <div
                    class="playhead"
                    style=move || format!("left: {:.2}%;", playhead_pct.get())
                ></div>
            </div>
            <div class="audio-controls">
                <button class="play-btn-inline" on:click=toggle>
                    {move || if playing.get() {
                        view! {
                            <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
                                <rect x="6" y="4" width="4" height="16"/>
                                <rect x="14" y="4" width="4" height="16"/>
                            </svg>
                        }.into_view()
                    } else {
                        view! {
                            <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
                                <polygon points="6,3 20,12 6,21"/>
                            </svg>
                        }.into_view()
                    }}
                </button>
                {move || selection.get().map(|_| view! {
                    <button class="clear-sel-btn" on:click=clear_selection title="Снять выделение">
                        "✕"
                    </button>
                })}
            </div>
            <audio
                node_ref=audio_ref
                src=file_url
                preload="metadata"
                on:timeupdate=on_timeupdate
                on:ended=on_ended
            />
        </div>
    }
}

#[component]
fn NodeListModal(
    nodes: RwSignal<Vec<Node>>,
    on_delete: impl Fn(Vec<Uuid>) + Copy + 'static,
    on_close: impl Fn() + Copy + 'static,
) -> impl IntoView {
    let filter = create_rw_signal(String::new());
    let selected = create_rw_signal::<Vec<Uuid>>(Vec::new());

    let filtered_nodes = Signal::derive(move || {
        let ns = nodes.get();
        let f = filter.get().to_lowercase();
        if f.is_empty() {
            ns
        } else {
            ns.into_iter()
                .filter(|n| kind_label(n.kind).to_lowercase().contains(&f))
                .collect()
        }
    });

    let toggle_select = move |id: Uuid| {
        selected.update(|s| {
            if let Some(pos) = s.iter().position(|&x| x == id) {
                s.remove(pos);
            } else {
                s.push(id);
            }
        });
    };

    let select_all = move |_| {
        let ids: Vec<Uuid> = filtered_nodes.get_untracked().iter().map(|n| n.id).collect();
        selected.set(ids);
    };

    let select_none = move |_| {
        selected.set(Vec::new());
    };

    let delete_selected = move |_| {
        let ids = selected.get_untracked();
        if !ids.is_empty() {
            on_delete(ids);
        }
    };

    view! {
        <div class="modal-backdrop" on:click=move |_| on_close()>
            <div class="modal node-list-modal" on:click=|ev| ev.stop_propagation()>
                <div class="modal-header">
                    <span class="modal-title">"Список нод"</span>
                    <button class="modal-close" on:click=move |_| on_close()>"✕"</button>
                </div>
                <div class="node-list-toolbar">
                    <input type="text"
                        placeholder="Фильтр..."
                        prop:value=move || filter.get()
                        on:input=move |ev| filter.set(event_target_value(&ev))
                    />
                    <button class="ghost" on:click=select_all>"Все"</button>
                    <button class="ghost" on:click=select_none>"Ничего"</button>
                    <button class="danger" on:click=delete_selected>
                        {move || {
                            let count = selected.get().len();
                            if count > 0 { format!("Удалить ({})", count) } else { "Удалить".to_string() }
                        }}
                    </button>
                </div>
                <div class="node-list-body">
                    <For
                        each=move || filtered_nodes.get()
                        key=|n| n.id
                        children=move |node| {
                            let id = node.id;
                            let label = kind_label(node.kind);
                            let name = node.asset.as_ref()
                                .map(|a| a.original_name.clone())
                                .or(node.output.as_ref().map(|o| o.file_name.clone()))
                                .unwrap_or_default();
                            let is_selected = Signal::derive(move || selected.get().contains(&id));
                            view! {
                                <div
                                    class="node-list-row"
                                    class:selected=is_selected
                                    on:click=move |_| toggle_select(id)
                                >
                                    {if is_selected.get() {
                                        view! { <span>"☑"</span> }.into_view()
                                    } else {
                                        view! { <span>"☐"</span> }.into_view()
                                    }}
                                    <span class="node-list-type">{label}</span>
                                    <span class="node-list-name">{name}</span>
                                </div>
                            }
                        }
                    />
                </div>
            </div>
        </div>
    }
}

#[component]
fn JsonModal(
    url: String,
    label: &'static str,
    on_close: impl Fn() + Copy + 'static,
) -> impl IntoView {
    let json_text = create_rw_signal::<Option<String>>(None);

    {
        let url = url.clone();
        spawn_local(async move {
            let window = web_sys::window().unwrap();
            let resp = wasm_bindgen_futures::JsFuture::from(
                window.fetch_with_str(&url)
            ).await;
            if let Ok(resp_val) = resp {
                let resp: web_sys::Response = resp_val.unchecked_into();
                if let Ok(text_promise) = resp.text() {
                    if let Ok(text_val) = wasm_bindgen_futures::JsFuture::from(text_promise).await {
                        if let Some(s) = text_val.as_string() {
                            json_text.set(Some(s));
                        }
                    }
                }
            }
        });
    }

    view! {
        <div class="modal-backdrop" style="z-index: 200;" on:click=move |_| on_close()>
            <div class="json-modal" on:click=|ev| ev.stop_propagation()>
                <div class="modal-header">
                    <span class="modal-title">{label}</span>
                    <button class="modal-close" on:click=move |_| on_close()>"✕"</button>
                </div>
                <pre class="json-content">
                    {move || json_text.get().unwrap_or_else(|| "Загрузка...".to_string())}
                </pre>
            </div>
        </div>
    }
}

fn kind_label(kind: NodeKind) -> &'static str {
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
    }
}

fn poll_task(
    active_task_id: RwSignal<Option<Uuid>>,
    node_signal: RwSignal<Node>,
    task_id: Uuid,
    project_id: Uuid,
) {
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

async fn gloo_timers_sleep(ms: u32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32)
            .unwrap();
    });
    wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
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
