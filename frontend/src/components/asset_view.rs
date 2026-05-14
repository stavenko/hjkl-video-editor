use api_types::InputNodeKind;
use leptos::*;
use uuid::Uuid;
use wasm_bindgen::JsCast;
use web_sys::{Event, FileList, HtmlInputElement};

use crate::services::api::absolute_url;

use super::helpers::{format_size, thumbnail_url, file_url};
use super::video_player::{VideoPlayer, AudioPlayer};

#[component]
pub fn UploadInput(
    kind: InputNodeKind,
    on_file: impl Fn(FileList) + Copy + 'static,
) -> impl IntoView {
    let accept = match kind {
        InputNodeKind::Video => "video/*",
        InputNodeKind::Audio => "audio/*",
        InputNodeKind::Image => "image/*",
        InputNodeKind::VideoArray => "video/*",
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

#[component]
pub fn AssetView(
    project_id: Uuid,
    node_id: Uuid,
    asset: api_types::Asset,
) -> impl IntoView {
    let original = asset.original_name.clone();
    let size = format_size(asset.size_bytes);
    let kind = asset.kind;

    match kind {
        InputNodeKind::Video => {
            let thumb = thumbnail_url(project_id, node_id, kind);
            let file = file_url(project_id, node_id, kind);
            let slug = kind.url_slug();
            let loop_base = absolute_url(&format!(
                "/api/projects/{project_id}/nodes/{slug}/{node_id}/loop-clip?v=0"
            ));
            view! {
                <VideoPlayer thumb_url=thumb file_url=file loop_clip_base=loop_base />
                <div class="meta-row">
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
            let slug = kind.url_slug();
            let loop_base = absolute_url(&format!(
                "/api/projects/{project_id}/nodes/{slug}/{node_id}/loop-clip?v=0"
            ));
            view! {
                <AudioPlayer wave_url=wave_url file_url=audio_src loop_clip_base=loop_base />
                <div class="meta-row">
                    <span>{original}</span>
                    <span>{size}</span>
                </div>
            }.into_view()
        }
        InputNodeKind::VideoArray => {
            // Should not reach here -- VideoArray uses assets vec, not single asset
            ().into_view()
        }
    }
}
