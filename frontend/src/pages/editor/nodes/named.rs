use api_types::Node;
use leptos::*;
use uuid::Uuid;
use web_sys::MouseEvent;

use crate::services::project_service;

pub fn named_input_body(
    node_signal: RwSignal<Node>,
    project_id: Uuid,
    id_for_drag: Uuid,
) -> impl IntoView {
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

pub fn named_output_body(
    node_signal: RwSignal<Node>,
    project_id: Uuid,
    id_for_drag: Uuid,
    nodes: RwSignal<Vec<Node>>,
) -> impl IntoView {
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
                        >"\u{2715}"</button>
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
