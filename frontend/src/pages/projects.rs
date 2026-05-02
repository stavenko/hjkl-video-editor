use api_types::ProjectSummary;
use leptos::*;
use leptos_router::A;
use uuid::Uuid;

use crate::services::project_service;

#[component]
pub fn ProjectsPage() -> impl IntoView {
    let projects = create_rw_signal::<Vec<ProjectSummary>>(Vec::new());
    let error = create_rw_signal::<Option<String>>(None);
    let new_name = create_rw_signal(String::new());
    let renaming_id = create_rw_signal::<Option<Uuid>>(None);
    let rename_value = create_rw_signal(String::new());

    let reload = move || {
        spawn_local(async move {
            match project_service::list_projects().await {
                Ok(out) => {
                    projects.set(out.projects);
                    error.set(None);
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    create_effect(move |_| {
        reload();
    });

    let on_create = move || {
        let name = new_name.get_untracked().trim().to_string();
        if name.is_empty() {
            error.set(Some("Введите имя проекта".to_string()));
            return;
        }
        spawn_local(async move {
            match project_service::create_project(name).await {
                Ok(_) => {
                    new_name.set(String::new());
                    error.set(None);
                    reload();
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    let on_delete = move |id: Uuid| {
        spawn_local(async move {
            match project_service::delete_project(id).await {
                Ok(_) => {
                    error.set(None);
                    reload();
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    let start_rename = move |project: &ProjectSummary| {
        rename_value.set(project.name.clone());
        renaming_id.set(Some(project.id));
    };

    let cancel_rename = move || {
        renaming_id.set(None);
    };

    let confirm_rename = move || {
        let Some(id) = renaming_id.get_untracked() else {
            return;
        };
        let value = rename_value.get_untracked().trim().to_string();
        if value.is_empty() {
            error.set(Some("Введите новое имя".to_string()));
            return;
        }
        spawn_local(async move {
            match project_service::rename_project(id, value).await {
                Ok(_) => {
                    renaming_id.set(None);
                    error.set(None);
                    reload();
                }
                Err(e) => error.set(Some(e.to_string())),
            }
        });
    };

    view! {
        <h1>"Проекты"</h1>

        {move || error.get().map(|msg| view! { <div class="error">{msg}</div> })}

        <div class="toolbar">
            <input
                type="text"
                placeholder="Имя нового проекта"
                prop:value=move || new_name.get()
                on:input=move |ev| new_name.set(event_target_value(&ev))
                on:keydown=move |ev: ev::KeyboardEvent| {
                    if ev.key() == "Enter" {
                        on_create();
                    }
                }
            />
            <button on:click=move |_| on_create()>"Создать"</button>
        </div>

        <div class="project-list">
            {move || {
                let items = projects.get();
                if items.is_empty() {
                    view! { <div class="empty">"Проектов пока нет"</div> }.into_view()
                } else {
                    items
                        .into_iter()
                        .map(|project| {
                            project_row(
                                project,
                                renaming_id,
                                rename_value,
                                start_rename,
                                cancel_rename,
                                confirm_rename,
                                on_delete,
                            )
                        })
                        .collect_view()
                }
            }}
        </div>
    }
}

fn project_row(
    project: ProjectSummary,
    renaming_id: RwSignal<Option<Uuid>>,
    rename_value: RwSignal<String>,
    start_rename: impl Fn(&ProjectSummary) + Copy + 'static,
    cancel_rename: impl Fn() + Copy + 'static,
    confirm_rename: impl Fn() + Copy + 'static,
    on_delete: impl Fn(Uuid) + Copy + 'static,
) -> View {
    let id = project.id;
    let name = project.name.clone();
    let updated_at = project.updated_at.format("%Y-%m-%d %H:%M").to_string();
    let editing = Signal::derive(move || renaming_id.get() == Some(id));
    let project_for_rename = project.clone();

    view! {
        <div class="project-row">
            <Show
                when=move || editing.get()
                fallback=move || {
                    let name = name.clone();
                    let updated_at = updated_at.clone();
                    let project_for_rename = project_for_rename.clone();
                    view! {
                        <div class="name">{name}</div>
                        <div class="meta">{updated_at}</div>
                        <div class="actions">
                            <A href=format!("/projects/{}", id) attr:class="button-link">
                                "Открыть"
                            </A>
                            <button
                                class="ghost"
                                on:click=move |_| start_rename(&project_for_rename)
                            >
                                "Переименовать"
                            </button>
                            <button class="danger" on:click=move |_| on_delete(id)>
                                "Удалить"
                            </button>
                        </div>
                    }
                }
            >
                <input
                    type="text"
                    prop:value=move || rename_value.get()
                    on:input=move |ev| rename_value.set(event_target_value(&ev))
                    on:keydown=move |ev: ev::KeyboardEvent| {
                        if ev.key() == "Enter" {
                            confirm_rename();
                        } else if ev.key() == "Escape" {
                            cancel_rename();
                        }
                    }
                />
                <button on:click=move |_| confirm_rename()>"Сохранить"</button>
                <button class="ghost" on:click=move |_| cancel_rename()>"Отмена"</button>
            </Show>
        </div>
    }
    .into_view()
}
