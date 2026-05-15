mod node_view;
mod nodes;
use node_view::NodeView;

use api_types::{Edge, InputNodeKind, Node, NodeKind, Position, ProcessNodeKind, ProjectDetail};
use leptos::*;
use leptos_router::{use_params_map, A};
use uuid::Uuid;
use wasm_bindgen::JsCast;
use web_sys::{MouseEvent, WheelEvent};

use std::collections::HashSet;

use crate::components::CanvasTransform;
use crate::components::helpers::*;
use crate::components::video_player::VideoPlayerModal;
use crate::components::modals::{AddNodeModal, NodeListModal, JsonModal};
use crate::components::subtitle_track::{SubtitleStyleModal, SubtitleSegmentModal};
use crate::services::{project_service, upload_service};

fn apply_position_updates(
    updates: &[(Uuid, Position)],
    nodes: RwSignal<Vec<Node>>,
    project_id: Signal<Option<Uuid>>,
) {
    // Update locally — triggers node_signal sync in NodeView
    let updates_clone = updates.to_vec();
    nodes.update(|ns| {
        for (id, pos) in &updates_clone {
            if let Some(n) = ns.iter_mut().find(|n| n.id == *id) {
                n.position = *pos;
            }
        }
    });
    // Persist to server in background
    let Some(pid) = project_id.get_untracked() else { return };
    spawn_local(async move {
        for (id, pos) in updates_clone {
            let _ = project_service::update_node_position(pid, id, pos).await;
        }
    });
}

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
    let selected_nodes = create_rw_signal::<HashSet<Uuid>>(HashSet::new());
    // Selection rectangle: (start_x, start_y, current_x, current_y) in canvas coords
    let selection_rect = create_rw_signal::<Option<(f32, f32, f32, f32)>>(None);
    let selection_start_screen = create_rw_signal::<Option<(i32, i32)>>(None);
    // Template save: (screen_x, screen_y) for context menu
    let template_ctx_menu = create_rw_signal::<Option<(f64, f64)>>(None);
    let template_name_prompt = create_rw_signal(false);
    let template_name_input = create_rw_signal(String::new());
    // Template placement: (template_name, bbox_w, bbox_h, inputs)
    let placing_template = create_rw_signal::<Option<(String, f32, f32, Vec<api_types::TemplatePort>)>>(None);
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
                if placing_template.get_untracked().is_some() {
                    placing_template.set(None);
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
        if placing_kind.get_untracked().is_some() || placing_phrase.get_untracked().is_some() || placing_template.get_untracked().is_some() {
            let (cx, cy) = screen_to_canvas(ev.client_x(), ev.client_y());
            placing_pos.set(Some((cx, cy)));
            return;
        }
        if connecting_from.get_untracked().is_some() {
            let (cx, cy) = screen_to_canvas(ev.client_x(), ev.client_y());
            connect_mouse.set(Some((cx, cy)));
            return;
        }
        if let Some(state) = drag_state.get_untracked() {
            let (cx, cy) = screen_to_canvas(ev.client_x(), ev.client_y());
            let new_pos = Position {
                x: cx - state.pointer_offset_x,
                y: cy - state.pointer_offset_y,
            };
            drag_pos.set(Some((state.node_id, new_pos)));
            return;
        }
        // Update selection rectangle
        if let Some((sx, sy, _, _)) = selection_rect.get_untracked() {
            let (cx, cy) = screen_to_canvas(ev.client_x(), ev.client_y());
            selection_rect.set(Some((sx, sy, cx, cy)));
        }
    };

    let on_mouse_up = move |ev: MouseEvent| {
        // Finalize selection rectangle
        if let Some((sx, sy, ex, ey)) = selection_rect.get_untracked() {
            selection_rect.set(None);
            selection_start_screen.set(None);
            let min_x = sx.min(ex);
            let max_x = sx.max(ex);
            let min_y = sy.min(ey);
            let max_y = sy.max(ey);
            // Only select if dragged at least 5px (avoid accidental click)
            if (max_x - min_x) > 5.0 || (max_y - min_y) > 5.0 {
                let ns = nodes.get_untracked();
                let mut sel = if ev.shift_key() {
                    selected_nodes.get_untracked()
                } else {
                    HashSet::new()
                };
                for n in &ns {
                    let nx = n.position.x;
                    let ny = n.position.y;
                    if nx >= min_x && nx <= max_x && ny >= min_y && ny <= max_y {
                        sel.insert(n.id);
                    }
                }
                selected_nodes.set(sel);
                return;
            }
        }
        // Template placement — create Template node
        if let Some((tpl_name, bbox_w, bbox_h, inputs)) = placing_template.get_untracked() {
            let Some((cx, cy)) = placing_pos.get_untracked() else { return };
            let Some(pid) = project_id.get_untracked() else { return };
            placing_template.set(None);
            placing_pos.set(None);
            let position = Position { x: cx, y: cy };
            let kind = NodeKind::Process(ProcessNodeKind::Template);
            let parent = editing_map.get_untracked();
            spawn_local(async move {
                match project_service::create_node(pid, kind, position, parent).await {
                    Ok(out) => {
                        let node_id = out.node.id;
                        let settings = api_types::NodeSettings::Template {
                            template_name: tpl_name,
                            bbox_w, bbox_h, inputs,
                        };
                        let _ = project_service::update_node_settings(pid, node_id, settings).await;
                        reload();
                    }
                    Err(e) => error.set(Some(e.to_string())),
                }
            });
            return;
        }
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
                on:mousedown=move |ev: MouseEvent| {
                    // Only start selection rect on direct canvas click (not on a node)
                    if ev.button() != 0 { return; }
                    if placing_kind.get_untracked().is_some() || placing_phrase.get_untracked().is_some() { return; }
                    if connecting_from.get_untracked().is_some() { return; }
                    // Start selection rectangle
                    let (cx, cy) = screen_to_canvas(ev.client_x(), ev.client_y());
                    selection_rect.set(Some((cx, cy, cx, cy)));
                    selection_start_screen.set(Some((ev.client_x(), ev.client_y())));
                    if !ev.shift_key() {
                        selected_nodes.set(HashSet::new());
                    }
                }
                on:contextmenu=move |ev: MouseEvent| {
                    if !selected_nodes.get_untracked().is_empty() {
                        ev.prevent_default();
                        template_ctx_menu.set(Some((ev.client_x() as f64, ev.client_y() as f64)));
                    }
                }
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
                                selected_nodes=selected_nodes
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
                    if let Some((ref name, bw, bh, _)) = placing_template.get() {
                        return Some(view! {
                            <div
                                class="node ghost process"
                                style=format!("left:{}px;top:{}px;width:{}px;height:{}px;opacity:0.6;", cx, cy, bw, bh)
                            >
                                <div class="node-header">
                                    <span class="node-kind-badge">{format!("Шаблон: {}", name)}</span>
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

                // Selection rectangle
                {move || {
                    let (sx, sy, ex, ey) = selection_rect.get()?;
                    let left = sx.min(ex);
                    let top = sy.min(ey);
                    let w = (ex - sx).abs();
                    let h = (ey - sy).abs();
                    Some(view! {
                        <div class="selection-rect" style=format!(
                            "left:{}px;top:{}px;width:{}px;height:{}px;",
                            left, top, w, h
                        )/>
                    })
                }}
                </div>
            </div>

            <Show when=move || add_modal_open.get()>
                <AddNodeModal
                    on_select=on_create_node
                    on_close=move || add_modal_open.set(false)
                    on_template=move |template_name: String| {
                        add_modal_open.set(false);
                        // Load template to get bbox and inputs, then enter placement mode
                        spawn_local(async move {
                            match project_service::list_templates().await {
                                Ok(out) => {
                                    if let Some(t) = out.templates.into_iter().find(|t| t.name == template_name) {
                                        // Compute bounding box from template nodes
                                        let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                                        for tn in &t.nodes {
                                            min_x = min_x.min(tn.relative_position.x);
                                            min_y = min_y.min(tn.relative_position.y);
                                            max_x = max_x.max(tn.relative_position.x + 240.0); // node width
                                            max_y = max_y.max(tn.relative_position.y + 100.0); // approx node height
                                        }
                                        let bbox_w = (max_x - min_x).max(240.0);
                                        let bbox_h = (max_y - min_y).max(100.0);
                                        placing_template.set(Some((template_name, bbox_w, bbox_h, t.inputs)));
                                        placing_pos.set(None);
                                    }
                                }
                                Err(e) => error.set(Some(e.to_string())),
                            }
                        });
                    }
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

            // Template context menu (right-click on selected nodes)
            {move || template_ctx_menu.get().map(|(x, y)| {
                view! {
                    <div class="subtrack-ctx-backdrop" on:click=move |_| template_ctx_menu.set(None)>
                        <div class="subtrack-ctx-menu" style=format!("left:{}px;top:{}px;", x, y)
                            on:click=|ev: MouseEvent| ev.stop_propagation()
                        >
                            <button class="subtrack-ctx-item" on:click=move |_| {
                                template_ctx_menu.set(None);
                                template_name_input.set(String::new());
                                template_name_prompt.set(true);
                            }>"Сохранить как шаблон"</button>
                            <button class="subtrack-ctx-item" on:click=move |_| {
                                template_ctx_menu.set(None);
                                let sel = selected_nodes.get_untracked();
                                let ns = nodes.get_untracked();
                                let mut sel_nodes: Vec<_> = ns.iter().filter(|n| sel.contains(&n.id)).cloned().collect();
                                if sel_nodes.len() < 2 { return; }
                                sel_nodes.sort_by(|a, b| a.position.y.partial_cmp(&b.position.y).unwrap());
                                let min_y = sel_nodes.first().unwrap().position.y;
                                let max_y = sel_nodes.last().unwrap().position.y;
                                let step = (max_y - min_y) / (sel_nodes.len() - 1) as f32;
                                let updates: Vec<_> = sel_nodes.iter().enumerate()
                                    .map(|(i, n)| (n.id, Position { x: n.position.x, y: min_y + step * i as f32 }))
                                    .collect();
                                apply_position_updates(&updates, nodes, project_id);
                            }>"Упорядочить по Y"</button>
                            <button class="subtrack-ctx-item" on:click=move |_| {
                                template_ctx_menu.set(None);
                                let sel = selected_nodes.get_untracked();
                                let ns = nodes.get_untracked();
                                let mut sel_nodes: Vec<_> = ns.iter().filter(|n| sel.contains(&n.id)).cloned().collect();
                                if sel_nodes.len() < 2 { return; }
                                sel_nodes.sort_by(|a, b| a.position.x.partial_cmp(&b.position.x).unwrap());
                                let min_x = sel_nodes.first().unwrap().position.x;
                                let max_x = sel_nodes.last().unwrap().position.x;
                                let step = (max_x - min_x) / (sel_nodes.len() - 1) as f32;
                                let updates: Vec<_> = sel_nodes.iter().enumerate()
                                    .map(|(i, n)| (n.id, Position { x: min_x + step * i as f32, y: n.position.y }))
                                    .collect();
                                apply_position_updates(&updates, nodes, project_id);
                            }>"Упорядочить по X"</button>
                            <button class="subtrack-ctx-item" on:click=move |_| {
                                template_ctx_menu.set(None);
                                let sel = selected_nodes.get_untracked();
                                let ns = nodes.get_untracked();
                                let sel_nodes: Vec<_> = ns.iter().filter(|n| sel.contains(&n.id)).collect();
                                if sel_nodes.is_empty() { return; }
                                let avg_x = sel_nodes.iter().map(|n| n.position.x).sum::<f32>() / sel_nodes.len() as f32;
                                let updates: Vec<_> = sel_nodes.iter()
                                    .map(|n| (n.id, Position { x: avg_x, y: n.position.y }))
                                    .collect();
                                apply_position_updates(&updates, nodes, project_id);
                            }>"Выровнять по Y"</button>
                            <button class="subtrack-ctx-item" on:click=move |_| {
                                template_ctx_menu.set(None);
                                let sel = selected_nodes.get_untracked();
                                let ns = nodes.get_untracked();
                                let sel_nodes: Vec<_> = ns.iter().filter(|n| sel.contains(&n.id)).collect();
                                if sel_nodes.is_empty() { return; }
                                let avg_y = sel_nodes.iter().map(|n| n.position.y).sum::<f32>() / sel_nodes.len() as f32;
                                let updates: Vec<_> = sel_nodes.iter()
                                    .map(|n| (n.id, Position { x: n.position.x, y: avg_y }))
                                    .collect();
                                apply_position_updates(&updates, nodes, project_id);
                            }>"Выровнять по X"</button>
                            <button class="subtrack-ctx-item" style="color:#e55;" on:click=move |_| {
                                template_ctx_menu.set(None);
                                let ids: Vec<Uuid> = selected_nodes.get_untracked().into_iter().collect();
                                let Some(pid) = project_id.get_untracked() else { return };
                                let parent = editing_map.get_untracked();
                                selected_nodes.set(HashSet::new());
                                spawn_local(async move {
                                    for id in ids {
                                        let _ = project_service::delete_node(pid, id, parent).await;
                                    }
                                    reload();
                                });
                            }>"Удалить выделенные"</button>
                        </div>
                    </div>
                }
            })}

            // Template name prompt
            <Show when=move || template_name_prompt.get()>
                <div class="modal-backdrop" on:click=move |_| template_name_prompt.set(false)>
                    <div class="modal" on:click=|ev: MouseEvent| ev.stop_propagation() style="width:320px;">
                        <div class="modal-header">"Имя шаблона"</div>
                        <div class="modal-body">
                            <input type="text" style="width:100%;padding:8px;font-size:14px;"
                                placeholder="Название..."
                                prop:value=move || template_name_input.get()
                                on:input=move |ev| template_name_input.set(event_target_value(&ev))
                                on:keydown=move |ev: web_sys::KeyboardEvent| {
                                    if ev.key() == "Enter" {
                                        let name = template_name_input.get_untracked();
                                        if !name.trim().is_empty() {
                                            let ids: Vec<Uuid> = selected_nodes.get_untracked().into_iter().collect();
                                            let Some(pid) = project_id.get_untracked() else { return };
                                            let parent = editing_map.get_untracked();
                                            template_name_prompt.set(false);
                                            spawn_local(async move {
                                                match project_service::save_template(pid, name, ids, parent).await {
                                                    Ok(_) => { selected_nodes.set(HashSet::new()); }
                                                    Err(e) => error.set(Some(e.to_string())),
                                                }
                                            });
                                        }
                                    }
                                }
                            />
                        </div>
                        <div class="modal-footer">
                            <button on:click=move |_| {
                                let name = template_name_input.get_untracked();
                                if !name.trim().is_empty() {
                                    let ids: Vec<Uuid> = selected_nodes.get_untracked().into_iter().collect();
                                    let Some(pid) = project_id.get_untracked() else { return };
                                    let parent = editing_map.get_untracked();
                                    template_name_prompt.set(false);
                                    spawn_local(async move {
                                        match project_service::save_template(pid, name, ids, parent).await {
                                            Ok(_) => { selected_nodes.set(HashSet::new()); }
                                            Err(e) => error.set(Some(e.to_string())),
                                        }
                                    });
                                }
                            }>"Сохранить"</button>
                            <button on:click=move |_| template_name_prompt.set(false)>"Отмена"</button>
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}
