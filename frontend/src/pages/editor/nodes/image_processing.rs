use api_types::{Node, ProcessNodeKind, TaskStatus};
use leptos::*;
use uuid::Uuid;
use wasm_bindgen::JsCast;
use web_sys::MouseEvent;

use crate::services::api::absolute_url;
use crate::services::project_service;

pub fn remove_background_body(
    node_signal: RwSignal<Node>,
    project_id: Uuid,
    id_for_drag: Uuid,
) -> impl IntoView {
    let nid = node_signal.get_untracked().id;
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

pub fn resize_image_body(
    node_signal: RwSignal<Node>,
    project_id: Uuid,
    id_for_drag: Uuid,
) -> impl IntoView {
    let nid = node_signal.get_untracked().id;
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

pub fn add_border_body(
    node_signal: RwSignal<Node>,
    project_id: Uuid,
    id_for_drag: Uuid,
) -> impl IntoView {
    let nid = node_signal.get_untracked().id;
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
