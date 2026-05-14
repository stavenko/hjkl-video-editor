use api_types::{Edge, InputNodeKind, Node, NodeKind, Position, ProcessNodeKind, TaskStatus};
use leptos::*;
use uuid::Uuid;
use wasm_bindgen::JsCast;
use web_sys::{Event, FileList, HtmlInputElement, MouseEvent};

use crate::components::helpers::*;
use crate::components::asset_view::{UploadInput, AssetView};
use crate::services::{project_service, upload_service};

use super::nodes;

#[component]
pub fn NodeView(
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
                            nodes::basic::map_body(node_signal, on_enter_map).into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::SubtitlePiece) => {
                            nodes::basic::subtitle_piece_body(node_signal, project_id, id_for_drag).into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::Overlay) => {
                            nodes::overlay::overlay_body(node_signal, project_id, id_for_drag, edges, nodes).into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::RemoveBackground) => {
                            nodes::image_processing::remove_background_body(node_signal, project_id, id_for_drag).into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::ResizeImage) => {
                            nodes::image_processing::resize_image_body(node_signal, project_id, id_for_drag).into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::AddBorder) => {
                            nodes::image_processing::add_border_body(node_signal, project_id, id_for_drag).into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::SubtitleTrack) => {
                            nodes::subtitle_track::subtitle_track_body(node_signal, project_id, id_for_drag, edges, nodes, st_style_modal, st_ctx_menu).into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::NamedInput) => {
                            nodes::named::named_input_body(node_signal, project_id, id_for_drag).into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::NamedOutput) => {
                            nodes::named::named_output_body(node_signal, project_id, id_for_drag, nodes).into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::Clip) => {
                            nodes::clip::clip_body(node_signal, project_id, id_for_drag, edges, nodes).into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::Scalar) => {
                            nodes::basic::scalar_body(node_signal, project_id, id_for_drag).into_view()
                        }
                        NodeKind::Process(ProcessNodeKind::Spline) => {
                            nodes::basic::spline_body(node_signal, project_id).into_view()
                        }
                        NodeKind::Process(pk) => {
                            nodes::basic::generic_process_body(node_signal, project_id, id_for_drag, pk, missing_ports, on_run, json_modal, on_create_phrase_selector, on_upload_error).into_view()
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
