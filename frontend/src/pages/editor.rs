use api_types::{Edge, InputNodeKind, Node, NodeKind, Position, ProcessNodeKind, ProjectDetail, TaskStatus};
use leptos::*;
use leptos_router::{use_params_map, A};
use uuid::Uuid;
use wasm_bindgen::JsCast;
use web_sys::{Event, FileList, HtmlInputElement, MouseEvent, WheelEvent};

use crate::components::CanvasTransform;
use crate::components::helpers::*;
use crate::components::video_player::{VideoPlayer, AudioPlayer, VideoPlayerModal};
use crate::components::modals::{AddNodeModal, NodeListModal, JsonModal};
use crate::components::asset_view::{UploadInput, AssetView};
use crate::components::subtitles_view::SubtitlesView;
use crate::components::overlay::{OverlayPreviewAnim, OverlayKfEditor};
use crate::components::subtitle_track::{SubtitleStyleModal, SubtitleSegmentModal};
use crate::services::api::absolute_url;
use crate::services::{project_service, upload_service};

#[derive(Clone, Copy)]
struct DragState {
    node_id: Uuid,
    pointer_offset_x: f32,
    pointer_offset_y: f32,
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
    let root_nodes = create_rw_signal::<Vec<Node>>(Vec::new());
    let root_edges = create_rw_signal::<Vec<Edge>>(Vec::new());

    // Active nodes/edges — switch based on whether we're inside a Map subgraph
    let nodes = create_rw_signal::<Vec<Node>>(Vec::new());
    let edges = create_rw_signal::<Vec<Edge>>(Vec::new());
    let error = create_rw_signal::<Option<String>>(None);
    let add_modal_open = create_rw_signal(false);
    let player_src = create_rw_signal::<Option<String>>(None);
    let connecting_from = create_rw_signal::<Option<Uuid>>(None);
    let connect_mouse = create_rw_signal::<Option<(f32, f32)>>(None);
    let json_modal = create_rw_signal::<Option<(String, &'static str)>>(None);
    let placing_kind = create_rw_signal::<Option<NodeKind>>(None);
    // Pending phrase selector: (src_node_id, phrase) — not yet created on server
    let placing_phrase = create_rw_signal::<Option<(Uuid, String)>>(None);
    // SubtitleTrack modals — rendered at EditorPage level to escape canvas transform
    let st_style_modal = create_rw_signal::<Option<RwSignal<Node>>>(None);
    let st_seg_modal = create_rw_signal::<Option<(usize, String, f64, f64, RwSignal<Node>)>>(None);
    // (idx, x, y, node_signal, local_segments, is_merge, next_idx)
    let st_ctx_menu = create_rw_signal::<Option<(usize, f64, f64, RwSignal<Node>, RwSignal<Vec<api_types::SubtitleSegment>>, bool, usize)>>(None);
    let node_list_open = create_rw_signal(false);
    let editing_map = create_rw_signal::<Option<Uuid>>(None); // Some(map_node_id) when inside subgraph
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

    let switch_to_view = move |proj_nodes: &[Node], proj_edges: &[Edge]| {
        root_nodes.set(proj_nodes.to_vec());
        root_edges.set(proj_edges.to_vec());
        if let Some(map_id) = editing_map.get_untracked() {
            // Inside a Map subgraph — show subgraph nodes
            if let Some(map_node) = proj_nodes.iter().find(|n| n.id == map_id) {
                if let Some(sg) = &map_node.subgraph {
                    nodes.set(sg.nodes.clone());
                    edges.set(sg.edges.clone());
                    return;
                }
            }
            // Map node not found — exit subgraph
            editing_map.set(None);
        }
        nodes.set(proj_nodes.to_vec());
        edges.set(proj_edges.to_vec());
    };

    let reload = move || {
        let Some(id) = project_id.get_untracked() else {
            return;
        };
        spawn_local(async move {
            match project_service::get_project(id).await {
                Ok(out) => {
                    switch_to_view(&out.project.nodes, &out.project.edges);
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
            match project_service::create_node(pid, kind, position, editing_map.get_untracked()).await {
                Ok(out) => {
                    nodes.update(|ns| ns.push(out.node));
                    error.set(None);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    let enter_map = move |map_node_id: Uuid| {
        editing_map.set(Some(map_node_id));
        let rn = root_nodes.get_untracked();
        if let Some(map_node) = rn.iter().find(|n| n.id == map_node_id) {
            if let Some(sg) = &map_node.subgraph {
                nodes.set(sg.nodes.clone());
                edges.set(sg.edges.clone());
            }
        }
    };

    let exit_map = move || {
        editing_map.set(None);
        nodes.set(root_nodes.get_untracked());
        edges.set(root_edges.get_untracked());
    };

    let cancel_placement = move || {
        placing_kind.set(None);
        placing_pos.set(None);
    };

    // Esc cancels placement
    create_effect(move |_| {
        let cb = wasm_bindgen::closure::Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(move |ev: web_sys::KeyboardEvent| {
            if ev.key() == "Escape" {
                if placing_kind.get_untracked().is_some() {
                    cancel_placement();
                }
                if placing_phrase.get_untracked().is_some() {
                    placing_phrase.set(None);
                    placing_pos.set(None);
                }
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
            match project_service::delete_node(pid, node_id, editing_map.get_untracked()).await {
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
            match project_service::disconnect_nodes(pid, from, from_port, to, to_port, editing_map.get_untracked()).await {
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
            match project_service::connect_nodes(pid, from, from_port, to, to_port, editing_map.get_untracked()).await {
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
        if placing_kind.get_untracked().is_some() || placing_phrase.get_untracked().is_some() {
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
        if let Some((src_id, phrase)) = placing_phrase.get_untracked() {
            let Some((cx, cy)) = placing_pos.get_untracked() else { return };
            placing_phrase.set(None);
            placing_pos.set(None);
            let Some(pid) = project_id.get_untracked() else { return };
            let parent = editing_map.get_untracked();
            let pos = Position { x: cx, y: cy };
            spawn_local(async move {
                let kind = NodeKind::Process(ProcessNodeKind::SubtitlePiece);
                if let Ok(out) = project_service::create_node(pid, kind, pos, parent).await {
                    let ps_id = out.node.id;
                    let settings = api_types::NodeSettings::SubtitlePiece {
                        phrase, occurrence: 0,
                    };
                    let _ = project_service::update_node_settings(pid, ps_id, settings).await;
                    let _ = project_service::connect_nodes(
                        pid, src_id, String::new(),
                        ps_id, "subtitles".to_string(), parent
                    ).await;
                    let _ = project_service::run_node(pid, ps_id).await;
                    reload();
                }
            });
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
                {move || if editing_map.get().is_some() {
                    view! {
                        <button class="back" on:click=move |_| exit_map()>"← Граф"</button>
                    }.into_view()
                } else {
                    view! {
                        <A href="/" attr:class="back">"← Проекты"</A>
                    }.into_view()
                }}
                <div class="title">
                    {move || {
                        let name = project.get().map(|p| p.project.name).unwrap_or_default();
                        if let Some(map_id) = editing_map.get() {
                            let map_label = root_nodes.get()
                                .iter()
                                .find(|n| n.id == map_id)
                                .map(|n| kind_label(n.kind))
                                .unwrap_or("Map");
                            format!("{} > {}", name, map_label)
                        } else {
                            name
                        }
                    }}
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
                                match project_service::create_node(pid, node_kind, pos, None).await {
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
                                nodes=nodes
                                drag_pos=drag_pos
                                edges=edges
                                connecting_from=connecting_from
                                connecting_from_port=connecting_from_port
                                player_src=player_src
                                json_modal=json_modal
                                on_drag_start=start_drag
                                on_delete=on_delete_node
                                on_connect_complete=on_connect_complete
                                on_enter_map=enter_map
                                on_create_phrase_selector=move |src_node_id: Uuid, phrase: String| {
                                    placing_phrase.set(Some((src_node_id, phrase)));
                                    placing_pos.set(None);
                                }
                                on_create_reference=move |source_id: Uuid| {
                                    placing_kind.set(Some(NodeKind::Reference { source: source_id }));
                                    placing_pos.set(None);
                                }
                                st_style_modal=st_style_modal
                                st_seg_modal=st_seg_modal
                                st_ctx_menu=st_ctx_menu
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
                    let (cx, cy) = placing_pos.get()?;
                    if let Some(kind) = placing_kind.get() {
                        let label = kind_label(kind);
                        return Some(view! {
                            <div
                                class="node ghost"
                                style=format!("left: {}px; top: {}px;", cx, cy)
                            >
                                <div class="node-header">
                                    <span class="node-kind-badge">{label}</span>
                                </div>
                            </div>
                        }.into_view());
                    }
                    if let Some((_, ref phrase)) = placing_phrase.get() {
                        return Some(view! {
                            <div
                                class="node ghost process"
                                style=format!("left: {}px; top: {}px;", cx, cy)
                            >
                                <div class="node-header">
                                    <span class="node-kind-badge">"Subtitle piece"</span>
                                </div>
                                <div class="node-body">
                                    <div class="phrase-input phrase-found" style="font-size:12px; padding:4px;">
                                        {phrase.clone()}
                                    </div>
                                </div>
                            </div>
                        }.into_view());
                    }
                    None
                }}
                </div>
            </div>

            <Show when=move || add_modal_open.get()>
                <AddNodeModal
                    on_select=on_create_node
                    on_close=move || add_modal_open.set(false)
                    inside_subgraph=Signal::derive(move || editing_map.get().is_some())
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
                            let _ = project_service::delete_node(pid, id, editing_map.get_untracked()).await;
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

            // SubtitleTrack modals — outside canvas transform
            {move || st_ctx_menu.get().map(|(idx, x, y, ns, local_segs, is_merge, next_idx)| {
                let save_segs = move |new_segs: Vec<api_types::SubtitleSegment>| {
                    local_segs.set(new_segs.clone());
                    let n = ns.get_untracked();
                    let (st, rx, ry, fp) = match &n.settings {
                        Some(api_types::NodeSettings::SubtitleTrack { styles, resolution_x, resolution_y, fps, .. }) =>
                            (styles.clone(), *resolution_x, *resolution_y, *fps),
                        _ => (vec![], 1920, 1080, 30),
                    };
                    let settings = api_types::NodeSettings::SubtitleTrack {
                        styles: st, segments: new_segs, resolution_x: rx, resolution_y: ry, fps: fp,
                    };
                    spawn_local(async move {
                        let pid = project_id.get_untracked().unwrap_or(Uuid::nil());
                        let nid = ns.get_untracked().id;
                        let _ = project_service::update_node_settings(pid, nid, settings).await;
                        let _ = project_service::run_node(pid, nid).await;
                    });
                };
                let move_track = move |new_track: u32| {
                    let mut segs = local_segs.get_untracked();
                    if let Some(s) = segs.get_mut(idx) { s.track = new_track; }
                    save_segs(segs);
                    st_ctx_menu.set(None);
                };
                let do_merge = move || {
                    let mut segs = local_segs.get_untracked();
                    if next_idx >= segs.len() || idx >= segs.len() { st_ctx_menu.set(None); return; }
                    // Merge: extend left, remove right
                    let right_text = segs[next_idx].text.clone();
                    let right_end = segs[next_idx].end_ms;
                    segs[idx].text = format!("{} {}", segs[idx].text.trim(), right_text.trim());
                    segs[idx].end_ms = right_end;
                    segs.remove(next_idx);
                    save_segs(segs);
                    st_ctx_menu.set(None);
                };
                view! {
                    <div class="subtrack-ctx-backdrop" on:click=move |_| st_ctx_menu.set(None)>
                        <div class="subtrack-ctx-menu" style=format!("left:{}px;top:{}px;", x, y)
                            on:click=|ev: MouseEvent| ev.stop_propagation()
                        >
                            {if is_merge {
                                view! { <button class="subtrack-ctx-item" on:click=move |_| do_merge()>"Объединить"</button> }.into_view()
                            } else {
                                let segs = local_segs.get();
                                let cur_seg = store_value(segs.get(idx).cloned());
                                let cur_track = cur_seg.get_value().as_ref().map(|s| s.track).unwrap_or(0);
                                let cur_start = cur_seg.get_value().as_ref().map(|s| s.start_ms).unwrap_or(0.0);
                                let cur_end = cur_seg.get_value().as_ref().map(|s| s.end_ms).unwrap_or(0.0);

                                // Check overlap on target track
                                let can_go_up = cur_track > 0 && !segs.iter().any(|s|
                                    s.track == cur_track - 1 && s.start_ms < cur_end && s.end_ms > cur_start
                                );
                                let can_go_down = !segs.iter().any(|s|
                                    s.track == cur_track + 1 && s.start_ms < cur_end && s.end_ms > cur_start
                                );

                                let style_list = match &ns.get_untracked().settings {
                                    Some(api_types::NodeSettings::SubtitleTrack { styles, .. }) => styles.clone(),
                                    _ => vec![],
                                };

                                view! {
                                    // Track submenu
                                    {can_go_up.then(|| {
                                        let t = cur_track - 1;
                                        view! { <button class="subtrack-ctx-item" on:click=move |_| move_track(t)>"↑ Дорожка вверх"</button> }
                                    })}
                                    {can_go_down.then(|| {
                                        let t = cur_track + 1;
                                        view! { <button class="subtrack-ctx-item" on:click=move |_| move_track(t)>"↓ Дорожка вниз"</button> }
                                    })}
                                    <div style="border-top:1px solid var(--border);margin:2px 0;"></div>
                                    // Style submenu
                                    {
                                        let style_sub_open = create_rw_signal(false);
                                        let style_list = store_value(style_list);
                                        view! {
                                            <div class="subtrack-ctx-submenu-wrap">
                                                <button class="subtrack-ctx-item"
                                                    on:click=move |ev: MouseEvent| {
                                                        ev.stop_propagation();
                                                        style_sub_open.update(|v| *v = !*v);
                                                    }
                                                >"Стиль ▸"</button>
                                                <Show when=move || style_sub_open.get()>
                                                    <div class="subtrack-ctx-submenu">
                                                        {style_list.get_value().into_iter().map(|s| {
                                                            let sn = s.name.clone();
                                                            let sn2 = sn.clone();
                                                            let is_current = cur_seg.get_value().as_ref()
                                                                .and_then(|seg| seg.style_name.as_ref())
                                                                .map(|n| *n == sn)
                                                                .unwrap_or(sn == "Default");
                                                            view! {
                                                                <button class="subtrack-ctx-item"
                                                                    class:active=is_current
                                                                    on:click=move |_| {
                                                                        let mut segs = local_segs.get_untracked();
                                                                        if let Some(s) = segs.get_mut(idx) { s.style_name = Some(sn2.clone()); }
                                                                        save_segs(segs);
                                                                        st_ctx_menu.set(None);
                                                                    }
                                                                >{sn}</button>
                                                            }
                                                        }).collect_view()}
                                                    </div>
                                                </Show>
                                            </div>
                                        }
                                    }
                                    <div style="border-top:1px solid var(--border);margin:2px 0;"></div>
                                    // Clone to next track
                                    <button class="subtrack-ctx-item" on:click=move |_| {
                                        let mut segs: Vec<api_types::SubtitleSegment> = local_segs.get_untracked();
                                        if let Some(seg) = segs.get(idx).cloned() {
                                            let new_track = seg.track + 1;
                                            let mut cloned = seg;
                                            cloned.track = new_track;
                                            segs.push(cloned);
                                            save_segs(segs);
                                        }
                                        st_ctx_menu.set(None);
                                    }>"Клонировать ↓"</button>
                                    // Edit text
                                    {
                                        let edit_open = create_rw_signal(false);
                                        let edit_text = create_rw_signal(
                                            cur_seg.get_value().as_ref().map(|s| s.text.clone()).unwrap_or_default()
                                        );
                                        view! {
                                            <button class="subtrack-ctx-item" on:click=move |ev: MouseEvent| {
                                                ev.stop_propagation();
                                                edit_open.update(|v| *v = !*v);
                                            }>"Изменить текст ▸"</button>
                                            <Show when=move || edit_open.get()>
                                                <div class="subtrack-ctx-submenu" style="padding:6px;min-width:200px;">
                                                    <input type="text" class="subtrack-edit-input"
                                                        prop:value=move || edit_text.get()
                                                        on:input=move |ev| edit_text.set(event_target_value(&ev))
                                                        on:mousedown=|ev: MouseEvent| ev.stop_propagation()
                                                        on:keydown=move |ev: web_sys::KeyboardEvent| {
                                                            if ev.key() == "Enter" {
                                                                let mut segs = local_segs.get_untracked();
                                                                if let Some(s) = segs.get_mut(idx) {
                                                                    s.text = edit_text.get_untracked();
                                                                }
                                                                save_segs(segs);
                                                                st_ctx_menu.set(None);
                                                            }
                                                        }
                                                    />
                                                    <button class="subtrack-ctx-item" style="margin-top:4px;" on:click=move |_| {
                                                        let mut segs = local_segs.get_untracked();
                                                        if let Some(s) = segs.get_mut(idx) {
                                                            s.text = edit_text.get_untracked();
                                                        }
                                                        save_segs(segs);
                                                        st_ctx_menu.set(None);
                                                    }>"Сохранить"</button>
                                                </div>
                                            </Show>
                                        }
                                    }
                                }.into_view()
                            }}
                        </div>
                    </div>
                }
            })}

            {move || st_style_modal.get().map(|ns| {
                let save = move |settings: api_types::NodeSettings| {
                    spawn_local(async move {
                        let pid = project_id.get_untracked().unwrap_or(Uuid::nil());
                        let nid = ns.get_untracked().id;
                        let _ = project_service::update_node_settings(pid, nid, settings).await;
                        let _ = project_service::run_node(pid, nid).await;
                    });
                };
                view! {
                    <SubtitleStyleModal node_signal=ns on_save=save on_close=move || st_style_modal.set(None) />
                }
            })}

            {move || st_seg_modal.get().map(|(idx, text, start, end, ns)| {
                let save = move |settings: api_types::NodeSettings| {
                    spawn_local(async move {
                        let pid = project_id.get_untracked().unwrap_or(Uuid::nil());
                        let nid = ns.get_untracked().id;
                        let _ = project_service::update_node_settings(pid, nid, settings).await;
                        let _ = project_service::run_node(pid, nid).await;
                    });
                };
                view! {
                    <SubtitleSegmentModal
                        index=idx text=text start_ms=start end_ms=end
                        node_signal=ns on_save=save
                        on_close=move || st_seg_modal.set(None)
                    />
                }
            })}
        </div>
    }
}

#[component]
fn NodeView(
    node: Node,
    project_id: Uuid,
    nodes: RwSignal<Vec<Node>>,
    drag_pos: RwSignal<Option<(Uuid, Position)>>,
    edges: RwSignal<Vec<Edge>>,
    connecting_from: RwSignal<Option<Uuid>>,
    connecting_from_port: RwSignal<String>,
    player_src: RwSignal<Option<String>>,
    json_modal: RwSignal<Option<(String, &'static str)>>,
    on_drag_start: impl Fn(Uuid, MouseEvent) + Copy + 'static,
    on_delete: impl Fn(Uuid) + Copy + 'static,
    on_connect_complete: impl Fn(Uuid, String, Uuid, String) + Copy + 'static,
    on_enter_map: impl Fn(Uuid) + Copy + 'static,
    on_create_phrase_selector: impl Fn(Uuid, String) + Copy + 'static,
    on_create_reference: impl Fn(Uuid) + Copy + 'static,
    st_style_modal: RwSignal<Option<RwSignal<Node>>>,
    st_seg_modal: RwSignal<Option<(usize, String, f64, f64, RwSignal<Node>)>>,
    st_ctx_menu: RwSignal<Option<(usize, f64, f64, RwSignal<Node>, RwSignal<Vec<api_types::SubtitleSegment>>, bool, usize)>>,
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
    let is_reference = matches!(node_signal.get_untracked().kind, NodeKind::Reference { .. });
    let has_multi_inputs = {
        let n = node_signal.get_untracked();
        if let NodeKind::Process(pk) = n.kind {
            pk.input_ports_with_settings(n.settings.as_ref()).len() > 1
        } else { false }
    };
    let has_multi_outputs = {
        let n = node_signal.get_untracked();
        match n.kind {
            NodeKind::Process(ref pk) => pk.output_ports_with_settings(n.settings.as_ref()).len() > 1,
            _ => n.kind.output_ports().len() > 1,
        }
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
                let mut cls = if is_reference { "node reference".to_string() }
                    else if is_process { "node process".to_string() }
                    else { "node".to_string() };
                if has_multi_inputs { cls.push_str(" multi-inputs"); }
                if has_multi_outputs { cls.push_str(" multi-outputs"); }
                match node_signal.get_untracked().kind {
                    NodeKind::Process(ProcessNodeKind::SubtitleTrack) => cls.push_str(" wide"),
                    NodeKind::Process(ProcessNodeKind::NamedOutput) => cls.push_str(" medium"),
                    _ => {}
                }
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
                <span class="node-kind-badge">{move || {
                    let n = node_signal.get();
                    if let NodeKind::Reference { source } = n.kind {
                        let src_label = nodes.with(|ns| {
                            ns.iter().find(|n| n.id == source)
                                .map(|n| kind_label(n.kind))
                                .unwrap_or("?")
                        });
                        format!("& {}", src_label)
                    } else {
                        kind_label(n.kind).to_string()
                    }
                }}</span>
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
                {(!matches!(node_signal.get_untracked().kind, NodeKind::Reference { .. })).then(|| view! {
                    <button class="header-btn" title="Создать ссылку"
                        on:click=move |ev: MouseEvent| {
                            ev.stop_propagation();
                            on_create_reference(id_for_drag);
                        }
                    >"&"</button>
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
                        NodeKind::Process(ProcessNodeKind::Map) => {
                            let nid = n.id;
                            let sg_count = n.subgraph.as_ref().map(|sg| sg.nodes.len()).unwrap_or(0);
                            view! {
                                <div class="map-body">
                                    <div class="map-info">{format!("{} нод внутри", sg_count)}</div>
                                    <button class="run-btn" style="background: var(--accent);"
                                        on:click=move |_| on_enter_map(nid)
                                    >"Открыть"</button>
                                </div>
                            }.into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::SubtitlePiece) => {
                            let phrase_sig = Signal::derive(move || {
                                match &node_signal.get().settings {
                                    Some(api_types::NodeSettings::SubtitlePiece { phrase, .. }) => phrase.clone(),
                                    _ => String::new(),
                                }
                            });
                            let occ_sig = Signal::derive(move || {
                                match &node_signal.get().settings {
                                    Some(api_types::NodeSettings::SubtitlePiece { occurrence, .. }) => *occurrence,
                                    _ => 0,
                                }
                            });
                            let input_class = Signal::derive(move || {
                                let n = node_signal.get();
                                if matches!(n.task_status, Some(TaskStatus::Failed)) {
                                    "phrase-input phrase-error"
                                } else if n.output.is_some() {
                                    "phrase-input phrase-found"
                                } else {
                                    "phrase-input"
                                }
                            });
                            let save_phrase = move |p: String| {
                                let occ = occ_sig.get_untracked();
                                let settings = api_types::NodeSettings::SubtitlePiece { phrase: p, occurrence: occ };
                                node_signal.update(|n| n.settings = Some(settings.clone()));
                                spawn_local(async move {
                                    let _ = project_service::update_node_settings(project_id, id_for_drag, settings).await;
                                    let _ = project_service::run_node(project_id, id_for_drag).await;
                                });
                            };
                            view! {
                                <input type="text" class=input_class
                                    prop:value=move || phrase_sig.get()
                                    on:change=move |ev| {
                                        save_phrase(event_target_value(&ev).trim().to_string());
                                    }
                                    on:mousedown=|ev: MouseEvent| ev.stop_propagation()
                                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                                        if ev.key() == "Enter" {
                                            ev.prevent_default();
                                            ev.target().unwrap().unchecked_ref::<web_sys::HtmlElement>().blur().ok();
                                        }
                                    }
                                />
                                {move || {
                                    let n = node_signal.get();
                                    match n.task_status {
                                        Some(TaskStatus::Running { .. }) => view! {
                                            <div class="process-hint">"Поиск..."</div>
                                        }.into_view(),
                                        _ => ().into_view(),
                                    }
                                }}
                            }.into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::Overlay) => {
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
                        NodeKind::Process(ProcessNodeKind::RemoveBackground) => {
                            let current_prompt = match &n.settings {
                                Some(api_types::NodeSettings::RemoveBackground { prompt }) => prompt.clone(),
                                _ => String::new(),
                            };
                            let nid = n.id;
                            let prompt_sig = Signal::derive(move || {
                                match &node_signal.get().settings {
                                    Some(api_types::NodeSettings::RemoveBackground { prompt }) => prompt.clone(),
                                    _ => String::new(),
                                }
                            });
                            let save_prompt = move |p: String| {
                                let settings = api_types::NodeSettings::RemoveBackground { prompt: p.clone() };
                                node_signal.update(|n| n.settings = Some(settings.clone()));
                                spawn_local(async move {
                                    let _ = project_service::update_node_settings(project_id, id_for_drag, settings).await;
                                });
                            };
                            view! {
                                <input type="text" class="phrase-input"
                                    placeholder="fish, green leaf..."
                                    prop:value=move || prompt_sig.get()
                                    on:change=move |ev| {
                                        save_prompt(event_target_value(&ev).trim().to_string());
                                    }
                                    on:mousedown=|ev: MouseEvent| ev.stop_propagation()
                                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                                        if ev.key() == "Enter" {
                                            ev.prevent_default();
                                            ev.target().unwrap().unchecked_ref::<web_sys::HtmlElement>().blur().ok();
                                        }
                                    }
                                />
                                {move || {
                                    let ns = node_signal.get();
                                    match &ns.task_status {
                                        Some(TaskStatus::Queued) => view! {
                                            <div class="rembg-status">"В очереди..."</div>
                                        }.into_view(),
                                        Some(TaskStatus::Running { progress_pct }) => view! {
                                            <div class="rembg-status">
                                                <div class="rembg-progress-bar">
                                                    <div class="rembg-progress-fill" style=format!("width:{}%", progress_pct)></div>
                                                </div>
                                                "Обработка..."
                                            </div>
                                        }.into_view(),
                                        Some(TaskStatus::Failed) => view! {
                                            <div class="rembg-status error">"Ошибка"</div>
                                        }.into_view(),
                                        _ => {
                                            if let Some(output) = &ns.output {
                                                let url = absolute_url(&format!(
                                                    "/api/projects/{}/nodes/{}/{}/file?t={}",
                                                    project_id, ProcessNodeKind::RemoveBackground.url_slug(), nid, output.size_bytes
                                                ));
                                                view! {
                                                    <img class="rembg-preview" src=url draggable="false" />
                                                }.into_view()
                                            } else {
                                                ().into_view()
                                            }
                                        }
                                    }
                                }}
                            }.into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::ResizeImage) => {
                            let nid = n.id;
                            let w_sig = Signal::derive(move || match &node_signal.get().settings {
                                Some(api_types::NodeSettings::ResizeImage { width, .. }) => *width,
                                _ => 1920,
                            });
                            let h_sig = Signal::derive(move || match &node_signal.get().settings {
                                Some(api_types::NodeSettings::ResizeImage { height, .. }) => *height,
                                _ => 1080,
                            });
                            let save_resize = move |w: u32, h: u32| {
                                let settings = api_types::NodeSettings::ResizeImage { width: w, height: h };
                                node_signal.update(|n| n.settings = Some(settings.clone()));
                                spawn_local(async move {
                                    let _ = project_service::update_node_settings(project_id, id_for_drag, settings).await;
                                });
                            };
                            view! {
                                <div class="overlay-xy-row" style="margin:4px 0;"
                                    on:mousedown=|ev: MouseEvent| ev.stop_propagation()
                                >
                                    <label class="overlay-xy-field">
                                        <span>"W"</span>
                                        <input type="text" prop:value=move || format!("{}", w_sig.get())
                                            on:change=move |ev| {
                                                let w = event_target_value(&ev).parse::<u32>().unwrap_or(1920);
                                                save_resize(w, h_sig.get_untracked());
                                            }
                                        />
                                    </label>
                                    <label class="overlay-xy-field">
                                        <span>"H"</span>
                                        <input type="text" prop:value=move || format!("{}", h_sig.get())
                                            on:change=move |ev| {
                                                let h = event_target_value(&ev).parse::<u32>().unwrap_or(1080);
                                                save_resize(w_sig.get_untracked(), h);
                                            }
                                        />
                                    </label>
                                </div>
                                {move || {
                                    let ns = node_signal.get();
                                    if let Some(output) = &ns.output {
                                        let url = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/file?t={}",
                                            project_id, ProcessNodeKind::ResizeImage.url_slug(), nid, output.size_bytes
                                        ));
                                        view! { <img class="rembg-preview" src=url draggable="false" /> }.into_view()
                                    } else {
                                        ().into_view()
                                    }
                                }}
                            }.into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::AddBorder) => {
                            let nid = n.id;
                            let color_sig = Signal::derive(move || match &node_signal.get().settings {
                                Some(api_types::NodeSettings::AddBorder { color, .. }) => color.clone(),
                                _ => "#FFFFFF".to_string(),
                            });
                            let bw_sig = Signal::derive(move || match &node_signal.get().settings {
                                Some(api_types::NodeSettings::AddBorder { border_width, .. }) => *border_width,
                                _ => 5,
                            });
                            let save_border = move |c: String, w: u32| {
                                let settings = api_types::NodeSettings::AddBorder { color: c, border_width: w };
                                node_signal.update(|n| n.settings = Some(settings.clone()));
                                spawn_local(async move {
                                    let _ = project_service::update_node_settings(project_id, id_for_drag, settings).await;
                                });
                            };
                            view! {
                                <div class="overlay-xy-row" style="margin:4px 0;"
                                    on:mousedown=|ev: MouseEvent| ev.stop_propagation()
                                >
                                    <label class="overlay-xy-field">
                                        <span>"color"</span>
                                        <input type="color" style="width:40px;height:20px;padding:0;border:none;"
                                            prop:value=move || color_sig.get()
                                            on:input=move |ev| {
                                                save_border(event_target_value(&ev), bw_sig.get_untracked());
                                            }
                                        />
                                    </label>
                                    <label class="overlay-xy-field">
                                        <span>"px"</span>
                                        <input type="text" prop:value=move || format!("{}", bw_sig.get())
                                            on:change=move |ev| {
                                                let w = event_target_value(&ev).parse::<u32>().unwrap_or(5);
                                                save_border(color_sig.get_untracked(), w);
                                            }
                                        />
                                    </label>
                                </div>
                                {move || {
                                    let ns = node_signal.get();
                                    match &ns.task_status {
                                        Some(TaskStatus::Queued) => view! {
                                            <div class="rembg-status">"В очереди..."</div>
                                        }.into_view(),
                                        Some(TaskStatus::Running { .. }) => view! {
                                            <div class="rembg-status">
                                                <div class="rembg-progress-bar">
                                                    <div class="rembg-progress-fill" style="width:50%"></div>
                                                </div>
                                                "Обработка..."
                                            </div>
                                        }.into_view(),
                                        Some(TaskStatus::Failed) => view! {
                                            <div class="rembg-status error">"Ошибка"</div>
                                        }.into_view(),
                                        _ => {
                                            if let Some(output) = &ns.output {
                                                let url = absolute_url(&format!(
                                                    "/api/projects/{}/nodes/{}/{}/file?t={}",
                                                    project_id, ProcessNodeKind::AddBorder.url_slug(), nid, output.size_bytes
                                                ));
                                                view! {
                                                    <img class="rembg-preview" src=url draggable="false" />
                                                }.into_view()
                                            } else {
                                                ().into_view()
                                            }
                                        }
                                    }
                                }}
                            }.into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::SubtitleTrack) => {
                            let st_nid = n.id;
                            let preview_loaded = create_rw_signal(0_u32);
                            let preview_reload = create_rw_signal(0_u32);
                            let canvas_visible = create_rw_signal(false);
                            // Local segments signal — the actual editable data
                            let local_segments = create_rw_signal(
                                match &n.settings {
                                    Some(api_types::NodeSettings::SubtitleTrack { segments, .. }) => segments.clone(),
                                    _ => Vec::new(),
                                }
                            );

                            // Load segments from output reactively
                            let segments = create_rw_signal::<Vec<(String, f64, f64)>>(Vec::new());
                            let last_sz = create_rw_signal(0_u64);
                            create_effect(move |_| {
                                let ns = node_signal.get();
                                let Some(output) = &ns.output else { return; };
                                let sz = output.size_bytes;
                                if sz == last_sz.get_untracked() { return; }
                                last_sz.set(sz);
                                let url = absolute_url(&format!(
                                    "/api/projects/{}/nodes/{}/{}/file?t={}",
                                    project_id, ProcessNodeKind::SubtitleTrack.url_slug(), st_nid, sz
                                ));
                                spawn_local(async move {
                                    let window = web_sys::window().unwrap();
                                    let mut opts = web_sys::RequestInit::new();
                                    opts.cache(web_sys::RequestCache::NoStore);
                                    let req = web_sys::Request::new_with_str_and_init(&url, &opts).unwrap();
                                    if let Ok(resp) = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&req)).await {
                                        let resp: web_sys::Response = resp.unchecked_into();
                                        if let Ok(text_p) = resp.text() {
                                            if let Ok(text_v) = wasm_bindgen_futures::JsFuture::from(text_p).await {
                                                if let Some(s) = text_v.as_string() {
                                                    let mut segs = Vec::new();
                                                    for line in s.lines() {
                                                        if let Some(rest) = line.strip_prefix("Dialogue:") {
                                                            let parts: Vec<&str> = rest.splitn(10, ',').collect();
                                                            if parts.len() >= 10 {
                                                                let text = parts[9].trim().to_string();
                                                                let start = parse_ass_time(parts[1].trim());
                                                                let end = parse_ass_time(parts[2].trim());
                                                                if !text.is_empty() { segs.push((text, start, end)); }
                                                            }
                                                        }
                                                    }
                                                    segments.set(segs);
                                                }
                                            }
                                        }
                                    }
                                });
                            });

                            let save_settings = move |settings: api_types::NodeSettings| {
                                if let api_types::NodeSettings::SubtitleTrack { ref segments, .. } = settings {
                                    local_segments.set(segments.clone());
                                }
                                spawn_local(async move {
                                    let _ = project_service::update_node_settings(project_id, id_for_drag, settings).await;
                                    let _ = project_service::run_node(project_id, id_for_drag).await;
                                    preview_reload.update(|v| *v += 1);
                                });
                            };

                            let cursor_px = create_rw_signal(0.0_f64);
                            let cursor_ms = create_rw_signal(0.0_f64);
                            let tl_zoom = create_rw_signal(1.0_f64);
                            let tl_offset = create_rw_signal(0.0_f64);
                            let tl_ref = create_node_ref::<leptos::html::Div>();
                            // Drag state for timeline boundaries
                            let dragging = create_rw_signal::<Option<(usize, bool, Option<usize>)>>(None);
                            // Drag state for subtitle position in preview
                            let sub_dragging = create_rw_signal::<Option<usize>>(None);
                            let preview_ref = create_node_ref::<leptos::html::Div>();

                            // Find background video for preview — reactive
                            let bg_sig = Signal::derive(move || {
                                let es = edges.get();
                                let ns = nodes.get();
                                es.iter()
                                    .find(|e| e.to_node == st_nid && e.to_port == "video")
                                    .and_then(|e| {
                                        let src = ns.iter().find(|n| n.id == e.from_node)?;
                                        let resolved = match src.kind {
                                            NodeKind::Reference { source } => api_types::resolve_reference(&ns, source)?,
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
                                <div class="subtrack-editor"
                                    on:mousedown=|ev: MouseEvent| ev.stop_propagation()
                                    on:mousemove=|ev: MouseEvent| ev.stop_propagation()
                                >
                                    // Header
                                    <div class="subtrack-header">
                                        <span class="subtrack-info">{move || format!("{} сегм.", segments.get().len())}</span>
                                        <span class="subtrack-cursor-time">{move || format!("{:.0}ms", cursor_ms.get())}</span>
                                        <button class="subtrack-btn" on:click=move |_| st_style_modal.set(Some(node_signal))>"Стили"</button>
                                    </div>

                                    // Preview frame with subtitle overlay
                                    <div class="subtrack-preview-wrap" node_ref=preview_ref
                                        on:mousedown=move |ev: MouseEvent| {
                                            ev.stop_propagation();
                                            ev.prevent_default();
                                            // Find closest visible segment to click position
                                            let ms = cursor_ms.get_untracked();
                                            let ls = local_segments.get_untracked();
                                            if let Some(el) = preview_ref.get_untracked() {
                                                let rect = el.get_bounding_client_rect();
                                                let css_w = el.client_width() as f64;
                                                let css_h = el.client_height() as f64;
                                                let scale = if css_w > 0.0 { rect.width() / css_w } else { 1.0 };
                                                let click_x = ((ev.client_x() as f64 - rect.left()) / scale / css_w).clamp(0.0, 1.0);
                                                let click_y = ((ev.client_y() as f64 - rect.top()) / scale / css_h).clamp(0.0, 1.0);

                                                // Find nearest visible segment by distance to its pos
                                                let seg_idx = ls.iter().enumerate()
                                                    .filter(|(_, seg)| ms >= seg.start_ms && ms <= seg.end_ms)
                                                    .min_by(|(_, a), (_, b)| {
                                                        let da = (a.pos_x - click_x).powi(2) + (a.pos_y - click_y).powi(2);
                                                        let db = (b.pos_x - click_x).powi(2) + (b.pos_y - click_y).powi(2);
                                                        da.partial_cmp(&db).unwrap()
                                                    })
                                                    .map(|(i, _)| i);

                                                if let Some(idx) = seg_idx {
                                                    sub_dragging.set(Some(idx));
                                                    canvas_visible.set(true);
                                                }
                                            }
                                        }
                                        on:mousemove=move |ev: MouseEvent| {
                                            if let Some(seg_idx) = sub_dragging.get_untracked() {
                                                if let Some(el) = preview_ref.get_untracked() {
                                                    let rect = el.get_bounding_client_rect();
                                                    let css_w = el.client_width() as f64;
                                                    let css_h = el.client_height() as f64;
                                                    let scale = if css_w > 0.0 { rect.width() / css_w } else { 1.0 };
                                                    let nx = ((ev.client_x() as f64 - rect.left()) / scale / css_w).clamp(0.0, 1.0);
                                                    let ny = ((ev.client_y() as f64 - rect.top()) / scale / css_h).clamp(0.0, 1.0);
                                                    local_segments.update(|segs| {
                                                        if let Some(s) = segs.get_mut(seg_idx) {
                                                            s.pos_x = nx;
                                                            s.pos_y = ny;
                                                        }
                                                    });
                                                }
                                            }
                                        }
                                        on:mouseup=move |_| {
                                            if sub_dragging.get_untracked().is_some() {
                                                sub_dragging.set(None);
                                                // Save + run + force preview reload
                                                let segs = local_segments.get_untracked();
                                                let ns = node_signal.get_untracked();
                                                let (st, rx, ry, fp) = match &ns.settings {
                                                    Some(api_types::NodeSettings::SubtitleTrack { styles, resolution_x, resolution_y, fps, .. }) =>
                                                        (styles.clone(), *resolution_x, *resolution_y, *fps),
                                                    _ => (vec![], 1920, 1080, 30),
                                                };
                                                let settings = api_types::NodeSettings::SubtitleTrack {
                                                    styles: st, segments: segs, resolution_x: rx, resolution_y: ry, fps: fp,
                                                };
                                                spawn_local(async move {
                                                    let _ = project_service::update_node_settings(project_id, id_for_drag, settings).await;
                                                    let _ = project_service::run_node(project_id, id_for_drag).await;
                                                    // Force preview reload
                                                    preview_reload.update(|v| *v += 1);
                                                });
                                            }
                                        }
                                        on:mouseleave=move |_| { sub_dragging.set(None); }
                                    >
                                        {
                                            let preview_src = create_rw_signal(String::new());
                                            let preview_loading = create_rw_signal(false);
                                            let last_requested_ms = create_rw_signal(-1.0_f64);
                                            let abort_ctrl = create_rw_signal::<Option<web_sys::AbortController>>(None);

                                            create_effect(move |_| {
                                                let reload = preview_reload.get();
                                                let ms = (cursor_ms.get() / 200.0).round() * 200.0;
                                                let key = ms + reload as f64 * 0.001; // unique key
                                                if key == last_requested_ms.get_untracked() { return; }
                                                last_requested_ms.set(key);

                                                let bg = bg_sig.get();
                                                let Some((id, slug, dur)) = bg else { return; };

                                                // Abort previous request
                                                if let Some(ctrl) = abort_ctrl.get_untracked() {
                                                    ctrl.abort();
                                                }

                                                let t_norm = if let Some(d) = dur {
                                                    if d > 0.0 { (ms / 1000.0 / d).clamp(0.0, 1.0) as f32 } else { 0.0 }
                                                } else { 0.0 };

                                                // Use subtitle-preview endpoint if we have segments
                                                let has_subs = !local_segments.get_untracked().is_empty();
                                                web_sys::console::log_1(&format!("ST preview: has_subs={}, t={:.4}", has_subs, t_norm).into());
                                                let url = if has_subs {
                                                    absolute_url(&format!(
                                                        "/api/projects/{}/subtitle-preview?video_node={}&video_slug={}&subs_node={}&t={:.4}&w=640",
                                                        project_id, id, slug, st_nid, t_norm
                                                    ))
                                                } else {
                                                    absolute_url(&format!(
                                                        "/api/projects/{}/nodes/{}/{}/thumbnail?t={:.4}&w=640",
                                                        project_id, slug, id, t_norm
                                                    ))
                                                };

                                                let ctrl = web_sys::AbortController::new().unwrap();
                                                let signal = ctrl.signal();
                                                abort_ctrl.set(Some(ctrl));
                                                preview_loading.set(true);

                                                spawn_local(async move {
                                                    let window = web_sys::window().unwrap();
                                                    let mut opts = web_sys::RequestInit::new();
                                                    opts.signal(Some(&signal));
                                                    opts.cache(web_sys::RequestCache::NoStore);
                                                    let req = web_sys::Request::new_with_str_and_init(&url, &opts).unwrap();
                                                    if let Ok(resp) = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&req)).await {
                                                        let resp: web_sys::Response = resp.unchecked_into();
                                                        if let Ok(blob_p) = resp.blob() {
                                                            if let Ok(blob) = wasm_bindgen_futures::JsFuture::from(blob_p).await {
                                                                let blob: web_sys::Blob = blob.unchecked_into();
                                                                if let Ok(obj_url) = web_sys::Url::create_object_url_with_blob(&blob) {
                                                                    // Revoke old URL
                                                                    let old = preview_src.get_untracked();
                                                                    if !old.is_empty() {
                                                                        let _ = web_sys::Url::revoke_object_url(&old);
                                                                    }
                                                                    preview_src.set(obj_url);
                                                                }
                                                            }
                                                        }
                                                    }
                                                    preview_loading.set(false);
                                                });
                                            });

                                            view! {
                                                <Show when=move || preview_loading.get() && preview_src.get().is_empty()>
                                                    <div class="subtrack-preview-spinner">"..."</div>
                                                </Show>
                                                <img class="subtrack-preview-frame" draggable="false"
                                                    class:loading=move || preview_loading.get()
                                                    src=move || preview_src.get()
                                                    on:load=move |_| {
                                                        preview_loaded.update(|v| *v += 1);
                                                        canvas_visible.set(false);
                                                    }
                                                />
                                            }
                                        }
                                        // Canvas overlay for subtitle text
                                        {
                                            let subs_canvas_ref = create_node_ref::<leptos::html::Canvas>();

                                            create_effect(move |_| {
                                                let _ = preview_loaded.get();
                                                let _ = sub_dragging.get();
                                                let ms = cursor_ms.get();
                                                let ls = local_segments.get();
                                                let styles = match &node_signal.get_untracked().settings {
                                                    Some(api_types::NodeSettings::SubtitleTrack { styles, .. }) => styles.clone(),
                                                    _ => vec![api_types::SubtitleStyle::default()],
                                                };
                                                let default_style = styles.first().cloned().unwrap_or_default();

                                                let Some(canvas) = subs_canvas_ref.get() else { return };
                                                let Some(parent) = preview_ref.get() else { return };
                                                let pw = parent.client_width() as u32;
                                                let ph = parent.client_height() as u32;
                                                if pw == 0 || ph == 0 { return; }

                                                canvas.set_width(pw);
                                                canvas.set_height(ph);

                                                let ctx: web_sys::CanvasRenderingContext2d = canvas
                                                    .get_context("2d").ok().flatten()
                                                    .unwrap()
                                                    .unchecked_into();
                                                ctx.clear_rect(0.0, 0.0, pw as f64, ph as f64);

                                                let res_y = match &node_signal.get_untracked().settings {
                                                    Some(api_types::NodeSettings::SubtitleTrack { resolution_y, .. }) => *resolution_y as f64,
                                                    _ => 1080.0,
                                                };
                                                let dragged_idx = sub_dragging.get();

                                                for (seg_i, seg) in ls.iter().enumerate() {
                                                    if ms < seg.start_ms || ms > seg.end_ms { continue; }
                                                    let st = seg.style_name.as_ref()
                                                        .and_then(|sn| styles.iter().find(|s| s.name == *sn))
                                                        .unwrap_or(&default_style);

                                                    // ASS font size is ~75% of CSS/Canvas font size
                                                    let font_px = st.size as f64 * ph as f64 / res_y * 0.9;
                                                    let x = seg.pos_x * pw as f64;
                                                    let y = (seg.pos_y - 0.01) * ph as f64;

                                                    let weight = if st.bold { "bold " } else { "" };
                                                    ctx.set_font(&format!("{}{:.0}px {}", weight, font_px, st.font));
                                                    ctx.set_text_align("center");

                                                    // Word wrap: break text to fit canvas width
                                                    let max_w = pw as f64 * 0.9;
                                                    let lines = wrap_text(&ctx, &seg.text, max_w);
                                                    let line_h = font_px * 1.2;
                                                    let total_h = lines.len() as f64 * line_h;
                                                    let y_start = y - total_h + line_h;

                                                    for (li, line) in lines.iter().enumerate() {
                                                        let ly = y_start + li as f64 * line_h;

                                                        // Outline first (under fill, like ASS)
                                                        if st.outline_width > 0 {
                                                            let olw = st.outline_width as f64 * ph as f64 / res_y;
                                                            ctx.set_stroke_style_str(&st.outline_color);
                                                            ctx.set_line_width(olw);
                                                            ctx.set_line_join("round");
                                                            ctx.stroke_text(line, x, ly).ok();
                                                        }

                                                        // Fill on top
                                                        ctx.set_fill_style_str(&st.color);
                                                        ctx.fill_text(line, x, ly).ok();
                                                    }

                                                    // Selection highlight for dragged segment
                                                    if dragged_idx == Some(seg_i) {
                                                        let max_line_w = lines.iter()
                                                            .map(|l| ctx.measure_text(l).map(|m| m.width()).unwrap_or(0.0))
                                                            .fold(0.0_f64, f64::max);
                                                        let pad = 4.0;
                                                        let box_x = x - max_line_w / 2.0 - pad;
                                                        let box_y = y_start - font_px + pad;
                                                        let box_w = max_line_w + pad * 2.0;
                                                        let box_h = total_h + pad;
                                                        ctx.set_stroke_style_str("rgba(77,171,247,0.8)");
                                                        ctx.set_line_width(2.0);
                                                        ctx.set_line_join("round");
                                                        ctx.stroke_rect(box_x, box_y, box_w, box_h);
                                                    }
                                                }
                                            });

                                            view! {
                                                <canvas node_ref=subs_canvas_ref
                                                    class="subtrack-subs-canvas"
                                                    class:hidden=move || !canvas_visible.get()
                                                />
                                            }
                                        }
                                    </div>

                                    // Timeline with zoom/pan
                                    <div class="subtrack-timeline" node_ref=tl_ref
                                        on:wheel=move |ev: WheelEvent| {
                                            ev.prevent_default();
                                            ev.stop_propagation();
                                            if let Some(el) = tl_ref.get_untracked() {
                                                let rect = el.get_bounding_client_rect();
                                                let css_w = el.client_width() as f64;
                                                let scale = if css_w > 0.0 { rect.width() / css_w } else { 1.0 };
                                                let mx = (ev.client_x() as f64 - rect.left()) / scale;
                                                if ev.ctrl_key() {
                                                    // Pinch zoom
                                                    let delta = -ev.delta_y() * 0.01;
                                                    let old_zoom = tl_zoom.get_untracked();
                                                    let new_zoom = (old_zoom * (1.0 + delta)).clamp(1.0, 50.0);
                                                    let ratio = new_zoom / old_zoom;
                                                    let old_off = tl_offset.get_untracked();
                                                    tl_offset.set(mx - (mx - old_off) * ratio);
                                                    tl_zoom.set(new_zoom);
                                                } else {
                                                    // Pan
                                                    tl_offset.update(|o| *o -= ev.delta_x() + ev.delta_y());
                                                }
                                            }
                                        }
                                        on:click=move |ev: MouseEvent| {
                                            // Set cursor position on click
                                            if let Some(el) = tl_ref.get_untracked() {
                                                let rect = el.get_bounding_client_rect();
                                                let css_w = el.client_width() as f64;
                                                let scale = if css_w > 0.0 { rect.width() / css_w } else { 1.0 };
                                                let mx = (ev.client_x() as f64 - rect.left()) / scale;
                                                cursor_px.set(mx);
                                                let zoom = tl_zoom.get_untracked();
                                                let off = tl_offset.get_untracked();
                                                let pct = ((mx - off) / (css_w * zoom)).clamp(0.0, 1.0);
                                                let ls = local_segments.get_untracked();
                                                let t_min = ls.iter().map(|s| s.start_ms).fold(f64::MAX, f64::min);
                                                let t_max = ls.iter().map(|s| s.end_ms).fold(0.0_f64, f64::max);
                                                let total = if t_max > t_min { t_max - t_min } else { 1.0 };
                                                cursor_ms.set(t_min + pct * total);
                                            }
                                        }
                                        on:mousemove=move |ev: MouseEvent| {
                                            // Only update cursor_px for drag operations
                                            if let Some(el) = tl_ref.get_untracked() {
                                                if dragging.get_untracked().is_none() { return; }
                                                let rect = el.get_bounding_client_rect();
                                                let css_w = el.client_width() as f64;
                                                let scale = if css_w > 0.0 { rect.width() / css_w } else { 1.0 };
                                                let mx = (ev.client_x() as f64 - rect.left()) / scale;
                                                cursor_px.set(mx);
                                                let zoom = tl_zoom.get_untracked();
                                                let off = tl_offset.get_untracked();
                                                let pct = ((mx - off) / (css_w * zoom)).clamp(0.0, 1.0);
                                                let ls = local_segments.get_untracked();
                                                let t_min = ls.iter().map(|s| s.start_ms).fold(f64::MAX, f64::min);
                                                let t_max = ls.iter().map(|s| s.end_ms).fold(0.0_f64, f64::max);
                                                let total = if t_max > t_min { t_max - t_min } else { 1.0 };
                                                let ms = t_min + pct * total;
                                                cursor_ms.set(ms);

                                                // Handle boundary drag
                                                if let Some((seg_idx, is_start, merge_next)) = dragging.get_untracked() {
                                                    local_segments.update(|segs| {
                                                        if let Some(merge_idx) = merge_next {
                                                            // Merge boundary: move end of left + start of right
                                                            if let Some(s) = segs.get_mut(seg_idx) { s.end_ms = ms; }
                                                            if let Some(s) = segs.get_mut(merge_idx) { s.start_ms = ms; }
                                                        } else if is_start {
                                                            if let Some(s) = segs.get_mut(seg_idx) { s.start_ms = ms; }
                                                        } else {
                                                            if let Some(s) = segs.get_mut(seg_idx) { s.end_ms = ms; }
                                                        }
                                                    });
                                                }
                                            }
                                        }
                                        on:mouseup=move |_| {
                                            if dragging.get_untracked().is_some() {
                                                dragging.set(None);
                                                // Save to server
                                                let segs = local_segments.get_untracked();
                                                let ns = node_signal.get_untracked();
                                                let (st, rx, ry, fp) = match &ns.settings {
                                                    Some(api_types::NodeSettings::SubtitleTrack { styles, resolution_x, resolution_y, fps, .. }) =>
                                                        (styles.clone(), *resolution_x, *resolution_y, *fps),
                                                    _ => (vec![], 1920, 1080, 30),
                                                };
                                                let settings = api_types::NodeSettings::SubtitleTrack {
                                                    styles: st, segments: segs, resolution_x: rx, resolution_y: ry, fps: fp,
                                                };
                                                spawn_local(async move {
                                                    let _ = project_service::update_node_settings(project_id, id_for_drag, settings).await;
                                                });
                                            }
                                        }
                                        on:mouseleave=move |_| {
                                            if dragging.get_untracked().is_some() {
                                                dragging.set(None);
                                            }
                                        }
                                    >
                                        // Cursor line — follows mouse directly
                                        <div class="subtrack-cursor" style=move || format!("left:{:.1}px", cursor_px.get())></div>

                                        // Inner container with zoom/pan transform
                                        <div class="subtrack-tl-inner" style=move || {
                                            let z = tl_zoom.get();
                                            let o = tl_offset.get();
                                            format!("width:{:.1}%;left:{:.1}px;", z * 100.0, o)
                                        }>
                                            // Segments with multi-track support — read directly from local_segments
                                            {move || {
                                                let ls = local_segments.get();
                                                if ls.is_empty() {
                                                    // Fall back to ASS-parsed segments for initial display
                                                    let segs = segments.get();
                                                    if segs.is_empty() { return view! { <span class="subtrack-empty">"Запустите ноду"</span> }.into_view(); }
                                                }

                                                let seg_source: Vec<(usize, String, f64, f64, u32)> = if !ls.is_empty() {
                                                    ls.iter().enumerate()
                                                        .filter(|(_, s)| !s.text.is_empty())
                                                        .map(|(i, s)| (i, s.text.clone(), s.start_ms, s.end_ms, s.track))
                                                        .collect()
                                                } else {
                                                    segments.get().into_iter().enumerate()
                                                        .map(|(i, (text, start, end))| (i, text, start, end, 0_u32))
                                                        .collect()
                                                };
                                                if seg_source.is_empty() { return view! { <span class="subtrack-empty">"Запустите ноду"</span> }.into_view(); }

                                                let max_track = seg_source.iter().map(|s| s.4).max().unwrap_or(0);
                                                let track_count = max_track + 1;
                                                let track_h = 100.0 / track_count as f64;

                                                let mut seg_data = seg_source;
                                                seg_data.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());

                                                let t_min = seg_data.iter().map(|s| s.2).fold(f64::MAX, f64::min);
                                                let t_max = seg_data.iter().map(|s| s.3).fold(0.0_f64, f64::max);
                                                let total = if t_max > t_min { t_max - t_min } else { 1.0 };

                                                let seg_stored = store_value(seg_data.clone());

                                                let fps = match &node_signal.get_untracked().settings {
                                                    Some(api_types::NodeSettings::SubtitleTrack { fps, .. }) => *fps,
                                                    _ => 30,
                                                };
                                                let frame_ms = 1000.0 / fps.max(1) as f64;

                                                let mut views = Vec::new();

                                                for (si, &(i, ref text, start, end, track)) in seg_data.iter().enumerate() {
                                                    let left_pct = ((start - t_min) / total * 100.0).max(0.0);
                                                    let width_pct = ((end - start) / total * 100.0).max(0.3);
                                                    let top_pct = track as f64 * track_h;
                                                    let lh = 60.0 * track_h / 100.0;
                                                    let text_c = text.clone();

                                                    // Segment block
                                                    views.push(view! {
                                                        <div class="subtrack-tl-seg"
                                                            style=format!("left:{:.2}%;width:{:.2}%;top:{:.1}%;height:{:.1}%;line-height:{:.0}px;",
                                                                left_pct, width_pct, top_pct, track_h, lh)
                                                            on:click=move |ev: MouseEvent| {
                                                                ev.stop_propagation();
                                                                // Center cursor on this segment
                                                                let ls = local_segments.get_untracked();
                                                                if let Some(seg) = ls.get(i) {
                                                                    let mid = (seg.start_ms + seg.end_ms) / 2.0;
                                                                    cursor_ms.set(mid);
                                                                    // Update cursor_px
                                                                    if let Some(el) = tl_ref.get_untracked() {
                                                                        let css_w = el.client_width() as f64;
                                                                        let t_min = ls.iter().map(|s| s.start_ms).fold(f64::MAX, f64::min);
                                                                        let t_max = ls.iter().map(|s| s.end_ms).fold(0.0_f64, f64::max);
                                                                        let total = if t_max > t_min { t_max - t_min } else { 1.0 };
                                                                        let pct = (mid - t_min) / total;
                                                                        let zoom = tl_zoom.get_untracked();
                                                                        let off = tl_offset.get_untracked();
                                                                        cursor_px.set(off + pct * css_w * zoom);
                                                                    }
                                                                }
                                                            }
                                                            on:contextmenu=move |ev: MouseEvent| {
                                                                ev.prevent_default();
                                                                ev.stop_propagation();
                                                                st_ctx_menu.set(Some((i, ev.client_x() as f64, ev.client_y() as f64, node_signal, local_segments, false, 0)));
                                                            }
                                                        >
                                                            {text_c}
                                                        </div>
                                                    }.into_view());

                                                    let prev_on_track = seg_data.iter().take(si).rev()
                                                        .find(|(_, _, _, _, t)| *t == track);
                                                    let next_on_track: Option<usize> = seg_data.iter().skip(si + 1)
                                                        .find(|(_, _, _, _, t)| *t == track).map(|s| s.0);

                                                    // Left boundary — skip only if previous segment on same track ends exactly here
                                                    // Adjacent = previous segment ends within 50ms of this one starting
                                                    let prev_adjacent = prev_on_track
                                                        .map(|p| (start - p.3).abs() < frame_ms)
                                                        .unwrap_or(false);
                                                    if !prev_adjacent {
                                                        let bp = left_pct;
                                                        let drag_i = i;
                                                        views.push(view! {
                                                            <div class="subtrack-boundary"
                                                                style=format!("left:{:.2}%;top:{:.1}%;height:{:.1}%;", bp, top_pct, track_h)
                                                                on:mousedown=move |ev: MouseEvent| {
                                                                    ev.stop_propagation(); ev.prevent_default();
                                                                    dragging.set(Some((drag_i, true, None)));
                                                                }
                                                                on:contextmenu=|ev: MouseEvent| { ev.prevent_default(); ev.stop_propagation(); }
                                                            ></div>
                                                        }.into_view());
                                                    }

                                                    // Right boundary — always present
                                                    // Merge popup only if next segment is adjacent (no gap)
                                                    {
                                                        let bp = left_pct + width_pct;
                                                        let cur_i = i;
                                                        // Adjacent = next segment starts within 50ms of this one ending
                                                        let next_adjacent = next_on_track.map(|ni| {
                                                            seg_data.iter().find(|s| s.0 == ni)
                                                                .map(|s| (s.2 - end).abs() < frame_ms)
                                                                .unwrap_or(false)
                                                        }).unwrap_or(false);
                                                        // Get text/time data for merge from seg_data
                                                        let cur_text = text.clone();
                                                        let cur_start = start;
                                                        let cur_end = end;
                                                        let next_data = next_on_track.and_then(|ni| {
                                                            seg_data.iter().find(|s| s.0 == ni).map(|s| (s.0, s.1.clone(), s.2, s.3))
                                                        });
                                                        let nd = store_value(next_data);

                                                        views.push(view! {
                                                            <div class="subtrack-boundary"
                                                                style=format!("left:{:.2}%;top:{:.1}%;height:{:.1}%;", bp, top_pct, track_h)
                                                                on:contextmenu=move |ev: MouseEvent| {
                                                                    ev.prevent_default();
                                                                    ev.stop_propagation();
                                                                    if !next_adjacent { return; }
                                                                    // Open merge popup at page level
                                                                    let ni = next_on_track.unwrap_or(0);
                                                                    st_ctx_menu.set(Some((cur_i, ev.client_x() as f64, ev.client_y() as f64, node_signal, local_segments, true, ni)));
                                                                }
                                                                on:mousedown=move |ev: MouseEvent| {
                                                                    ev.stop_propagation(); ev.prevent_default();
                                                                    if next_adjacent {
                                                                        // Merge boundary: drag moves both end+start
                                                                        dragging.set(Some((cur_i, false, next_on_track)));
                                                                    } else {
                                                                        // End boundary: drag moves end only
                                                                        dragging.set(Some((cur_i, false, None)));
                                                                    }
                                                                }
                                                            ></div>
                                                        }.into_view());
                                                    }

                                                }

                                                views.collect_view()
                                            }}
                                        </div>
                                    </div>
                                </div>

                            }.into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::NamedInput) => {
                            let name_sig = Signal::derive(move || {
                                match &node_signal.get().settings {
                                    Some(api_types::NodeSettings::NamedInput { name }) => name.clone(),
                                    _ => "default".to_string(),
                                }
                            });
                            view! {
                                <input type="text" class="phrase-input"
                                    prop:value=move || name_sig.get()
                                    on:change=move |ev| {
                                        let name = event_target_value(&ev).trim().to_string();
                                        let settings = api_types::NodeSettings::NamedInput { name };
                                        node_signal.update(|n| n.settings = Some(settings.clone()));
                                        spawn_local(async move {
                                            let _ = project_service::update_node_settings(project_id, id_for_drag, settings).await;
                                        });
                                    }
                                    on:mousedown=|ev: MouseEvent| ev.stop_propagation()
                                />
                            }.into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::NamedOutput) => {
                            let selected_names = Signal::derive(move || {
                                match &node_signal.get().settings {
                                    Some(api_types::NodeSettings::NamedOutput { names }) => names.clone(),
                                    _ => Vec::new(),
                                }
                            });
                            let available_names = Signal::derive(move || {
                                let selected = selected_names.get();
                                nodes.get().iter().filter_map(|n| {
                                    match &n.settings {
                                        Some(api_types::NodeSettings::NamedInput { name }) if !selected.contains(name) => Some(name.clone()),
                                        _ => None,
                                    }
                                }).collect::<Vec<_>>()
                            });
                            let add_name = create_rw_signal(String::new());
                            let save_names = move |names: Vec<String>| {
                                let settings = api_types::NodeSettings::NamedOutput { names };
                                node_signal.update(|n| n.settings = Some(settings.clone()));
                                spawn_local(async move {
                                    let _ = project_service::update_node_settings(project_id, id_for_drag, settings).await;
                                });
                            };
                            view! {
                                <div on:mousedown=|ev: MouseEvent| ev.stop_propagation()
                                     on:mousemove=|ev: MouseEvent| ev.stop_propagation()
                                >
                                    // Output list with delete buttons
                                    {move || selected_names.get().into_iter().map(|name| {
                                        let n = name.clone();
                                        let n2 = name.clone();
                                        view! {
                                            <div style="display:flex;align-items:center;gap:4px;padding:2px 0;font-size:11px;">
                                                <span style="color:var(--accent);flex:1;">{n}</span>
                                                <button style="background:none;border:none;color:var(--text-dim);cursor:pointer;font-size:10px;padding:0 2px;"
                                                    on:click=move |_| {
                                                        let mut names = selected_names.get_untracked();
                                                        names.retain(|x| x != &n2);
                                                        save_names(names);
                                                    }
                                                >"✕"</button>
                                            </div>
                                        }
                                    }).collect_view()}
                                    // Add new output
                                    <div style="display:flex;gap:4px;">
                                        <select class="overlay-interp-select" style="flex:1;"
                                            prop:value=move || add_name.get()
                                            on:change=move |ev| add_name.set(event_target_value(&ev))
                                        >
                                            <option value="">"+ добавить..."</option>
                                            {move || available_names.get().into_iter().map(|n| {
                                                let v = n.clone();
                                                view! { <option value=v>{n}</option> }
                                            }).collect_view()}
                                        </select>
                                        <button class="subtrack-btn" on:click=move |_| {
                                            let name = add_name.get_untracked();
                                            if name.is_empty() { return; }
                                            let mut names = selected_names.get_untracked();
                                            if !names.contains(&name) { names.push(name); }
                                            save_names(names);
                                            add_name.set(String::new());
                                        }>"+"</button>
                                    </div>
                                </div>
                            }.into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::Clip) => {
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

                            // Find background video via edges
                            let clip_bg = {
                                let es = edges.get_untracked();
                                let ns_all = nodes.get_untracked();
                                es.iter()
                                    .find(|e| e.to_node == n.id && (e.to_port == "media" || e.to_port.is_empty()))
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
                            };
                            let clip_bg_stored = store_value(clip_bg);
                            let clip_img_url: Option<String> = None;
                            let clip_img_stored = store_value(clip_img_url);

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
                                            let bg_stored = clip_bg_stored;
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
                                                            image_url=clip_img_stored.get_value()
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
                                    api_types::PortType::Number | api_types::PortType::SubtitleSegments
                                    | api_types::PortType::ClipDescriptor | api_types::PortType::AssSubtitles => {
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
                                        let math_nid = n.id;
                                        let math_slug = slug.to_string();
                                        let result_val = create_rw_signal("...".to_string());
                                        let last_fetched = create_rw_signal(0_u64);
                                        create_effect(move |_| {
                                            let ns = node_signal.get();
                                            let sz = ns.output.as_ref().map(|o| o.size_bytes).unwrap_or(0);
                                            if sz == 0 { result_val.set("...".to_string()); return; }
                                            if sz == last_fetched.get_untracked() { return; }
                                            last_fetched.set(sz);
                                            let url = absolute_url(&format!(
                                                "/api/projects/{}/nodes/{}/{}/file?t={}",
                                                project_id, math_slug, math_nid, sz
                                            ));
                                            spawn_local(async move {
                                                let window = web_sys::window().unwrap();
                                                let mut opts = web_sys::RequestInit::new();
                                                opts.cache(web_sys::RequestCache::NoStore);
                                                let req = web_sys::Request::new_with_str_and_init(&url, &opts).unwrap();
                                                if let Ok(resp) = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&req)).await {
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
                                        });
                                        view! {
                                            <div class="math-result">{move || result_val.get()}</div>
                                        }.into_view()
                                    }
                                    ProcessNodeKind::DetectSubtitles => {
                                        let sz = n.output.as_ref().map(|o| o.size_bytes).unwrap_or(0);
                                        let json_url = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/file?t={}",
                                            project_id, slug, n.id, sz
                                        ));
                                        let subs_node_id = n.id;
                                        view! {
                                            <SubtitlesView
                                                url=json_url
                                                project_id=project_id
                                                node_id=subs_node_id
                                                editable=true
                                                on_create_phrase_selector=move |phrase: String| {
                                                    on_create_phrase_selector(subs_node_id, phrase);
                                                }
                                            />
                                        }.into_view()
                                    }
                                    ProcessNodeKind::AssBuilder => {
                                        let sz = n.output.as_ref().map(|o| o.size_bytes).unwrap_or(0);
                                        let json_url = absolute_url(&format!(
                                            "/api/projects/{}/nodes/{}/{}/file?t={}",
                                            project_id, slug, n.id, sz
                                        ));
                                        let subs_node_id = n.id;
                                        view! {
                                            <SubtitlesView
                                                url=json_url
                                                project_id=project_id
                                                node_id=subs_node_id
                                                editable=false
                                                on_create_phrase_selector=move |phrase: String| {
                                                    on_create_phrase_selector(subs_node_id, phrase);
                                                }
                                            />
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
                        NodeKind::Reference { .. } => {
                            let num_ports = n.kind.output_ports_in_graph(&nodes.get()).len();
                            let h = if num_ports > 1 { num_ports * 20 } else { 0 };
                            view! {
                                <div style=format!("min-height: {}px;", h)></div>
                            }.into_view()
                        }
                    }
                }}
            </div>
            {move || {
                let n = node_signal.get();
                let ports = match n.kind {
                    NodeKind::Reference { .. } => n.kind.output_ports_in_graph(&nodes.get()),
                    NodeKind::Process(ref pk) => pk.output_ports_with_settings(n.settings.as_ref()),
                    _ => n.kind.output_ports(),
                };
                if ports.is_empty() {
                    ().into_view()
                } else if ports.len() == 1 {
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
