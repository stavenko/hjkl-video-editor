use api_types::{Edge, Node, ProcessNodeKind, TaskStatus};
use leptos::*;
use uuid::Uuid;
use wasm_bindgen::JsCast;
use web_sys::MouseEvent;

use crate::components::helpers::kind_label;
use crate::components::subtitles_view::SubtitlesView;
use crate::components::video_player::{VideoPlayer, AudioPlayer};
use crate::services::api::absolute_url;
use crate::services::project_service;

pub fn map_body(
    node_signal: RwSignal<Node>,
    on_enter_map: impl Fn(Uuid) + Copy + 'static,
) -> impl IntoView {
    let n = node_signal.get_untracked();
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

pub fn subtitle_piece_body(
    node_signal: RwSignal<Node>,
    project_id: Uuid,
    id_for_drag: Uuid,
) -> impl IntoView {
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

pub fn scalar_body(
    node_signal: RwSignal<Node>,
    project_id: Uuid,
    id_for_drag: Uuid,
) -> impl IntoView {
    let n = node_signal.get_untracked();
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

pub fn spline_body(
    node_signal: RwSignal<Node>,
    project_id: Uuid,
) -> impl IntoView {
    let n = node_signal.get_untracked();
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

pub fn generic_process_body(
    node_signal: RwSignal<Node>,
    project_id: Uuid,
    id_for_drag: Uuid,
    pk: ProcessNodeKind,
    missing_ports: Signal<Vec<String>>,
    on_run: impl Fn() + Copy + 'static,
    json_modal: RwSignal<Option<(String, &'static str)>>,
    on_create_phrase_selector: impl Fn(Uuid, String) + Copy + 'static,
    on_upload_error: impl Fn(String) + Copy + 'static,
) -> impl IntoView {
    let n = node_signal.get_untracked();
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

pub fn template_body(
    node_signal: RwSignal<Node>,
    project_id: Uuid,
    node_id: Uuid,
    nodes: RwSignal<Vec<Node>>,
    edges: RwSignal<Vec<Edge>>,
    on_delete: impl Fn(Uuid) + Copy + 'static,
) -> impl IntoView {
    let n = node_signal.get_untracked();
    let (tpl_name, bbox_w, bbox_h) = match &n.settings {
        Some(api_types::NodeSettings::Template { template_name, bbox_w, bbox_h, .. }) =>
            (template_name.clone(), *bbox_w, *bbox_h),
        _ => (String::new(), 0.0, 0.0),
    };

    let unpack_loading = create_rw_signal(false);

    let on_unpack = move |ev: MouseEvent| {
        ev.stop_propagation();
        if unpack_loading.get_untracked() { return; }
        unpack_loading.set(true);

        let n = node_signal.get_untracked();
        let tpl_name = match &n.settings {
            Some(api_types::NodeSettings::Template { template_name, .. }) => template_name.clone(),
            _ => return,
        };
        let pos = n.position;

        spawn_local(async move {
            match crate::services::project_service::unpack_template(
                project_id, tpl_name, pos, None,
            ).await {
                Ok(out) => {
                    let template_node_id = node_id;

                    // Rewire external connections using id_map
                    // For each edge connected TO the template node, find the
                    // internal target node via template inputs + id_map
                    let template_inputs: Vec<api_types::TemplatePort> = match &n.settings {
                        Some(api_types::NodeSettings::Template { inputs, .. }) => inputs.clone(),
                        _ => vec![],
                    };

                    let cur_edges = edges.get_untracked();
                    let external_edges: Vec<_> = cur_edges.iter()
                        .filter(|e| e.to_node == template_node_id)
                        .cloned()
                        .collect();

                    // Rewire: each external edge → fan out to all targets in the matching template port
                    let mut rewired_edges = Vec::new();
                    for ext_edge in &external_edges {
                        if let Some(tpl_port) = template_inputs.iter().find(|p| p.port_name == ext_edge.to_port) {
                            for target in &tpl_port.targets {
                                if let Some(new_id) = out.id_map.get(&target.node_id) {
                                    rewired_edges.push(api_types::Edge {
                                        from_node: ext_edge.from_node,
                                        from_port: ext_edge.from_port.clone(),
                                        to_node: *new_id,
                                        to_port: target.port_name.clone(),
                                    });
                                }
                            }
                        }
                    }

                    // Connect rewired edges on server
                    let pid = project_id;
                    for re in &rewired_edges {
                        let _ = crate::services::project_service::connect_nodes(
                            pid, re.from_node, re.from_port.clone(),
                            re.to_node, re.to_port.clone(), None,
                        ).await;
                    }

                    // Delete template node on server
                    let _ = crate::services::project_service::delete_node(pid, template_node_id, None).await;

                    // Reload to get clean state
                    nodes.update(|ns| {
                        ns.retain(|n| n.id != template_node_id);
                        for new_node in &out.nodes { ns.push(new_node.clone()); }
                    });
                    edges.update(|es| {
                        es.retain(|e| e.to_node != template_node_id && e.from_node != template_node_id);
                        for new_edge in &out.edges { es.push(new_edge.clone()); }
                        for re in rewired_edges { es.push(re); }
                    });

                    unpack_loading.set(false);
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("Unpack failed: {}", e).into());
                    unpack_loading.set(false);
                }
            }
        });
    };

    view! {
        <div class="node-body" style=format!("min-height:{}px;", bbox_h.max(40.0))
            on:mousedown=|ev: MouseEvent| ev.stop_propagation()
        >
            <div style="padding:8px;font-size:12px;color:var(--text-muted);">
                {format!("{} ({}×{:.0})", tpl_name, bbox_w as u32, bbox_h)}
            </div>
            <button class="header-btn" style="margin:8px;padding:4px 12px;font-size:12px;"
                on:click=on_unpack
                disabled=move || unpack_loading.get()
            >
                {move || if unpack_loading.get() { "Разворачиваю..." } else { "Развернуть" }}
            </button>
        </div>
    }.into_view()
}
