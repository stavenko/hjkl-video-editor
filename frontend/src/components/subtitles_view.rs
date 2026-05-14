use leptos::*;
use uuid::Uuid;
use wasm_bindgen::JsCast;
use web_sys::MouseEvent;

use crate::services::project_service;

#[component]
pub fn SubtitlesView(
    url: String,
    project_id: Uuid,
    node_id: Uuid,
    #[prop(default = false)] editable: bool,
    on_create_phrase_selector: impl Fn(String) + Copy + 'static,
) -> impl IntoView {
    let expanded = create_rw_signal(false);
    let words = create_rw_signal::<Vec<(String, f64, f64)>>(Vec::new());
    let raw_text = create_rw_signal(String::new());
    let loaded = create_rw_signal(false);
    let selection_text = create_rw_signal::<Option<String>>(None);
    let editing = create_rw_signal(false);
    let edit_content = create_rw_signal(String::new());
    let dirty = create_rw_signal(false);

    let load_data = {
        let url = url.clone();
        move || {
            if loaded.get_untracked() { return; }
            let url = url.clone();
            spawn_local(async move {
                let window = web_sys::window().unwrap();
                let mut opts = web_sys::RequestInit::new();
                opts.cache(web_sys::RequestCache::NoStore);
                let request = web_sys::Request::new_with_str_and_init(&url, &opts).unwrap();
                if let Ok(resp) = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request)).await {
                    let resp: web_sys::Response = resp.unchecked_into();
                    if let Ok(text_p) = resp.text() {
                        if let Ok(text_v) = wasm_bindgen_futures::JsFuture::from(text_p).await {
                            if let Some(s) = text_v.as_string() {
                                raw_text.set(s.clone());
                                let parsed: Vec<(String, f64, f64)> = if let Ok(arr) = js_sys::JSON::parse(&s) {
                                    let arr = if let Ok(segs) = js_sys::Reflect::get(&arr, &"segments".into()) {
                                        if segs.is_undefined() { arr } else { segs }
                                    } else { arr };
                                    if let Some(js_arr) = arr.dyn_ref::<js_sys::Array>() {
                                        (0..js_arr.length()).filter_map(|i| {
                                            let item = js_arr.get(i);
                                            let text = js_sys::Reflect::get(&item, &"text".into()).ok()?.as_string()?;
                                            let start = js_sys::Reflect::get(&item, &"start_ms".into()).ok()?.as_f64()?;
                                            let end = js_sys::Reflect::get(&item, &"end_ms".into()).ok()?.as_f64()?;
                                            let text = text.trim().to_string();
                                            if text.is_empty() { None } else { Some((text, start, end)) }
                                        }).collect()
                                    } else { Vec::new() }
                                } else { Vec::new() };
                                words.set(parsed);
                                loaded.set(true);
                            }
                        }
                    }
                }
            });
        }
    };

    let toggle = move |_| {
        let new_val = !expanded.get_untracked();
        expanded.set(new_val);
        if new_val { load_data(); }
    };

    let check_selection = move |_| {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(sel)) = window.get_selection() {
                let sel_str: String = sel.to_string().into();
                let text = sel_str.trim().to_string();
                if text.is_empty() {
                    selection_text.set(None);
                } else {
                    selection_text.set(Some(text));
                }
            }
        }
    };

    let start_editing = move |_: MouseEvent| {
        edit_content.set(raw_text.get_untracked());
        editing.set(true);
        dirty.set(false);
    };

    let save_edit = move |_: MouseEvent| {
        let c = edit_content.get_untracked();
        let settings = api_types::NodeSettings::DetectSubtitles {
            model: "small".to_string(),
            corrected_content: c.clone(),
        };
        // Update local state immediately so re-edit shows saved content
        raw_text.set(c.clone());
        // Re-parse words from saved content
        let parsed: Vec<(String, f64, f64)> = if let Ok(arr) = js_sys::JSON::parse(&c) {
            let arr = if let Ok(segs) = js_sys::Reflect::get(&arr, &"segments".into()) {
                if segs.is_undefined() { arr } else { segs }
            } else { arr };
            if let Some(js_arr) = arr.dyn_ref::<js_sys::Array>() {
                (0..js_arr.length()).filter_map(|i| {
                    let item = js_arr.get(i);
                    let text = js_sys::Reflect::get(&item, &"text".into()).ok()?.as_string()?;
                    let start = js_sys::Reflect::get(&item, &"start_ms".into()).ok()?.as_f64()?;
                    let end = js_sys::Reflect::get(&item, &"end_ms".into()).ok()?.as_f64()?;
                    let text = text.trim().to_string();
                    if text.is_empty() { None } else { Some((text, start, end)) }
                }).collect()
            } else { Vec::new() }
        } else { Vec::new() };
        words.set(parsed);
        dirty.set(false);
        editing.set(false);
        spawn_local(async move {
            let _ = project_service::update_node_settings(project_id, node_id, settings).await;
            let _ = project_service::run_node(project_id, node_id).await;
        });
    };

    let cancel_edit = move |_: MouseEvent| {
        editing.set(false);
        dirty.set(false);
    };

    view! {
        <div class="subs-view">
            <button class="subs-toggle" on:click=toggle>
                {move || if expanded.get() { "▾ Скрыть" } else { "▸ Показать" }}
            </button>
            <Show when=move || expanded.get()>
                <Show when=move || editing.get()>
                    <textarea
                        class="spell-textarea"
                        prop:value=move || edit_content.get()
                        on:input=move |ev| {
                            edit_content.set(event_target_value(&ev));
                            dirty.set(true);
                        }
                        on:mousedown=|ev: MouseEvent| ev.stop_propagation()
                        on:mousemove=|ev: MouseEvent| ev.stop_propagation()
                    />
                    <div class="subs-edit-buttons">
                        <button class="run-btn" on:click=save_edit>"Сохранить"</button>
                        <button class="run-btn" style="background: var(--border);" on:click=cancel_edit>"Отмена"</button>
                    </div>
                </Show>
                <Show when=move || !editing.get()>
                    <div class="subs-words"
                        on:mousedown=|ev: MouseEvent| ev.stop_propagation()
                        on:mousemove=|ev: MouseEvent| ev.stop_propagation()
                        on:mouseup=move |ev: MouseEvent| {
                            ev.stop_propagation();
                            check_selection(ev);
                        }
                    >
                        {move || words.get().into_iter().map(|(text, _start, _end)| {
                            view! {
                                <span class="sub-word">{text}</span>
                                {" "}
                            }
                        }).collect_view()}
                    </div>
                    {move || selection_text.get().map(|text| {
                        let phrase = text.clone();
                        view! {
                            <div class="subs-selection-tooltip">
                                <span class="subs-selection-text">{format!("\"{}\"", &text)}</span>
                                <button class="subs-create-phrase" on:click=move |ev: MouseEvent| {
                                    ev.stop_propagation();
                                    on_create_phrase_selector(phrase.clone());
                                    selection_text.set(None);
                                }>
                                    "🔍 Subtitle piece"
                                </button>
                            </div>
                        }
                    })}
                    {if editable {
                        Some(view! {
                            <button class="run-btn" style="margin-top: 4px;" on:click=start_editing>"✏ Редактировать"</button>
                        })
                    } else {
                        None
                    }}
                </Show>
            </Show>
        </div>
    }
}
