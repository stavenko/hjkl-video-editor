use api_types::{InputNodeKind, Node, NodeKind, ProcessNodeKind};
use leptos::*;
use uuid::Uuid;
use wasm_bindgen::JsCast;

use super::helpers::kind_label;

#[component]
pub fn AddNodeModal(
    on_select: impl Fn(NodeKind) + Copy + 'static,
    on_close: impl Fn() + Copy + 'static,
    inside_subgraph: Signal<bool>,
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
                            {move || inside_subgraph.get().then(|| view! {
                                <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::SubgraphInput))>
                                    <div class="node-type-icon">"➡️"</div>
                                    <div class="node-type-label">"Вход"</div>
                                </button>
                                <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::SubgraphOutput))>
                                    <div class="node-type-icon">"⬅️"</div>
                                    <div class="node-type-label">"Выход"</div>
                                </button>
                            })}
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
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::AssBuilder))>
                                <div class="node-type-icon">"📝"</div>
                                <div class="node-type-label">"ASS субтитры"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::SubtitlePiece))>
                                <div class="node-type-icon">"🔍"</div>
                                <div class="node-type-label">"Subtitle piece"</div>
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
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::RemoveBackground))>
                                <div class="node-type-icon">"✂"</div>
                                <div class="node-type-label">"Убрать фон"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::ResizeImage))>
                                <div class="node-type-icon">"📐"</div>
                                <div class="node-type-label">"Ресайз"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::AddBorder))>
                                <div class="node-type-icon">"🔲"</div>
                                <div class="node-type-label">"Обводка"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::NamedInput))>
                                <div class="node-type-icon">"📥"</div>
                                <div class="node-type-label">"Вход →"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::NamedOutput))>
                                <div class="node-type-icon">"📤"</div>
                                <div class="node-type-label">"→ Выход"</div>
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
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::Overlay))>
                                <div class="node-type-icon">"🖼"</div>
                                <div class="node-type-label">"Оверлей"</div>
                            </button>
                            <button class="node-type-card" on:click=move |_| on_select(NodeKind::Process(ProcessNodeKind::SubtitleTrack))>
                                <div class="node-type-icon">"💬"</div>
                                <div class="node-type-label">"Дорожка СТ"</div>
                            </button>
                        </div>
                    </Show>
                </div>
            </div>
        </div>
    }
}

#[component]
pub fn NodeListModal(
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
pub fn JsonModal(
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
