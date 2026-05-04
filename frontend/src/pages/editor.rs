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
        let Some(pid) = project_id.get_untracked() else {
            return;
        };
        let position = next_position(&nodes.get_untracked());
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
    let has_input_edge = Signal::derive(move || {
        edges.get().iter().any(|e| e.to_node == id_for_drag)
    });

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
            class=move || if is_process { "node process" } else { "node" }
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
                let ports = pk.input_ports();
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
                    match n.kind {
                        NodeKind::Input(_) => {
                            match (n.asset.clone(), upload_progress.get()) {
                                (Some(asset), _) => {
                                    let nid = n.id;
                                    view! { <AssetView project_id=project_id node_id=nid asset=asset player_src=player_src /> }.into_view()
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
                        NodeKind::Process(pk) => {
                            let status = n.task_status;
                            let has_output = n.output.is_some();
                            let needs_update = n.needs_update;
                            let connected = has_input_edge.get();

                            let show_update = needs_update || (connected && !has_output);

                            let status_view = if !connected {
                                view! { <div class="process-hint">"Подключите вход"</div> }.into_view()
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
                                match pk {
                                    ProcessNodeKind::ExtractAudio | ProcessNodeKind::TrimAudio => {
                                        let slug = pk.url_slug();
                                        let cache_bust = n.output.as_ref()
                                            .map(|o| {
                                                let mut h: u64 = 0;
                                                for b in o.cache_key.bytes() { h = h.wrapping_mul(31).wrapping_add(b as u64); }
                                                format!("{:x}", h)
                                            })
                                            .unwrap_or_default();
                                        let wave_url = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/thumbnail?v={}",
                                            project_id, slug, n.id, cache_bust
                                        ));
                                        let file_src = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/file?v={}",
                                            project_id, slug, n.id, cache_bust
                                        ));
                                        view! {
                                            <AudioPlayer wave_url=wave_url file_url=file_src />
                                        }.into_view()
                                    }
                                    ProcessNodeKind::DetectSilence | ProcessNodeKind::DetectSubtitles | ProcessNodeKind::SpeechBounds => {
                                        let slug = pk.url_slug();
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
                    NodeKind::Process(pk) => pk.output_ports(),
                    _ => vec![api_types::PortDef { name: String::new(), kind: n.kind.produced_output() }],
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
    player_src: RwSignal<Option<String>>,
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

            let video_src = file_url(project_id, node_id, kind);

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
                    <button class="play-btn-inline" on:click={
                        let video_src = video_src.clone();
                        move |ev: MouseEvent| {
                            ev.stop_propagation();
                            player_src.set(Some(video_src.clone()));
                        }
                    }>
                        <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
                            <polygon points="6,3 20,12 6,21"/>
                        </svg>
                    </button>
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
            view! {
                <AudioPlayer wave_url=wave_url file_url=audio_src />
                <div class="meta-row">
                    <span>{original}</span>
                    <span>{size}</span>
                </div>
            }.into_view()
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
fn AudioPlayer(wave_url: String, file_url: String) -> impl IntoView {
    let playing = create_rw_signal(false);
    let playhead_pct = create_rw_signal(0.0_f64);
    let audio_ref = create_node_ref::<html::Audio>();

    let toggle = move |_| {
        let Some(audio) = audio_ref.get_untracked() else { return };
        let el: &web_sys::HtmlMediaElement = audio.unchecked_ref();
        if playing.get_untracked() {
            el.pause().ok();
            playing.set(false);
        } else {
            el.play().ok();
            playing.set(true);
        }
    };

    let on_timeupdate = move |_| {
        let Some(audio) = audio_ref.get_untracked() else { return };
        let el: &web_sys::HtmlMediaElement = audio.unchecked_ref();
        let dur = el.duration();
        let cur = el.current_time();
        if dur.is_finite() && dur > 0.0 {
            playhead_pct.set(cur / dur * 100.0);
        }
    };

    let on_ended = move |_| {
        playing.set(false);
        playhead_pct.set(0.0);
    };

    view! {
        <div class="audio-player" class:playing=move || playing.get()>
            <div class="waveform-wrap">
                <img class="media-waveform" src=wave_url/>
                <div
                    class="playhead"
                    style=move || format!("left: {:.2}%;", playhead_pct.get())
                ></div>
            </div>
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
        NodeKind::Process(ProcessNodeKind::ExtractAudio) => "Извлечь аудио",
        NodeKind::Process(ProcessNodeKind::DetectSilence) => "Тишина",
        NodeKind::Process(ProcessNodeKind::DetectSubtitles) => "Субтитры",
        NodeKind::Process(ProcessNodeKind::SpeechBounds) => "Края речи",
        NodeKind::Process(ProcessNodeKind::TrimAudio) => "Обрезка аудио",
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
