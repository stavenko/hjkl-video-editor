use leptos::*;
use wasm_bindgen::JsCast;
use web_sys::MouseEvent;

#[component]
pub fn VideoPlayer(
    thumb_url: String,
    file_url: String,
    loop_clip_base: String,
) -> impl IntoView {
    let playing = create_rw_signal(false);
    let playhead_pct = create_rw_signal(0.0_f64);
    let current_time_ms = create_rw_signal(0.0_f64);
    let hover_pct = create_rw_signal::<Option<f64>>(None);
    let hover_time_ms = create_rw_signal(0.0_f64);
    let full_duration = create_rw_signal(0.0_f64);
    let video_ref = create_node_ref::<html::Video>();
    let thumb_ref = create_node_ref::<html::Div>();

    let selection = create_rw_signal::<Option<(f64, f64)>>(None);
    let selecting_from = create_rw_signal::<Option<f64>>(None);

    let mouse_down_pct = create_rw_signal::<Option<f64>>(None);
    let did_drag = create_rw_signal(false);

    let on_thumb_mousedown = move |ev: MouseEvent| {
        ev.prevent_default();
        let target = ev.target().unwrap();
        let el = target.unchecked_ref::<web_sys::HtmlElement>();
        let rect = el.get_bounding_client_rect();
        let pct = ((ev.client_x() as f64 - rect.left()) / rect.width()).clamp(0.0, 1.0);
        mouse_down_pct.set(Some(pct));
        selecting_from.set(Some(pct));
        did_drag.set(false);
        selection.set(None);
    };

    let on_thumb_mousemove = move |ev: MouseEvent| {
        if let Some(el) = thumb_ref.get_untracked() {
            let rect = el.get_bounding_client_rect();
            let pct = ((ev.client_x() as f64 - rect.left()) / rect.width()).clamp(0.0, 1.0);

            // Hover cursor -- use full video duration
            hover_pct.set(Some(pct));
            let dur = full_duration.get_untracked();
            if dur > 0.0 {
                hover_time_ms.set(pct * dur * 1000.0);
            }

            // Selection drag
            if let Some(start) = selecting_from.get_untracked() {
                let (a, b) = if pct < start { (pct, start) } else { (start, pct) };
                if (b - a) > 0.01 {
                    selection.set(Some((a, b)));
                    did_drag.set(true);
                }
            }
        }
    };

    let on_thumb_mouseup = move |_| {
        selecting_from.set(None);
        // Click without drag -> seek to position
        if !did_drag.get_untracked() {
            if let Some(pct) = mouse_down_pct.get_untracked() {
                if let Some(v) = video_ref.get_untracked() {
                    let el: &web_sys::HtmlMediaElement = v.unchecked_ref();
                    let dur = el.duration();
                    if dur.is_finite() && dur > 0.0 {
                        el.set_current_time(pct * dur);
                        playhead_pct.set(pct * 100.0);
                        current_time_ms.set(pct * dur * 1000.0);
                        // Pause if playing
                        if playing.get_untracked() {
                            el.pause().ok();
                            playing.set(false);
                        }
                    }
                }
            }
        }
        mouse_down_pct.set(None);
    };

    let toggle = {
        let file_url = file_url.clone();
        let loop_clip_base = loop_clip_base.clone();
        move |_| {
            let Some(video) = video_ref.get_untracked() else { return };
            let el: web_sys::HtmlMediaElement = video.unchecked_ref::<web_sys::HtmlMediaElement>().clone();
            if playing.get_untracked() {
                el.pause().ok();
                playing.set(false);
                return;
            }
            // If already has src (paused) -- just resume
            let current_src = el.src();
            let want_src = if let Some((a, b)) = selection.get_untracked() {
                format!("{}&start={:.4}&end={:.4}", loop_clip_base, a, b)
            } else {
                file_url.clone()
            };
            let need_reload = current_src.is_empty() || !current_src.ends_with(&want_src);
            if need_reload {
                el.set_src(&want_src);
                el.set_loop(selection.get_untracked().is_some());
                el.load();
            }
            el.set_volume(1.0);
            el.set_muted(false);
            playing.set(true);
            if let Ok(p) = el.play() {
                spawn_local(async move {
                    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
                });
            }
        }
    };

    let on_timeupdate = move |_| {
        let Some(video) = video_ref.get_untracked() else { return };
        let el: &web_sys::HtmlMediaElement = video.unchecked_ref();
        let dur = el.duration();
        let cur = el.current_time();
        if dur.is_finite() && dur > 0.0 {
            if let Some((a, b)) = selection.get_untracked() {
                let abs_t = a * dur + cur;
                playhead_pct.set((a + (cur / dur) * (b - a)) * 100.0);
                current_time_ms.set(abs_t * 1000.0);
            } else {
                playhead_pct.set(cur / dur * 100.0);
                current_time_ms.set(cur * 1000.0);
            }
        }
    };

    let on_ended = move |_| { playing.set(false); playhead_pct.set(0.0); };

    let clear_selection = move |ev: MouseEvent| {
        ev.stop_propagation();
        selection.set(None);
        if playing.get_untracked() {
            if let Some(v) = video_ref.get_untracked() {
                let el: &web_sys::HtmlMediaElement = v.unchecked_ref();
                el.pause().ok();
                playing.set(false);
            }
        }
    };

    view! {
        <div class="video-player" class:playing=move || playing.get()>
            <div
                class="video-thumb-wrap"
                node_ref=thumb_ref
                on:mousedown=on_thumb_mousedown
                on:mousemove=on_thumb_mousemove
                on:mouseup=on_thumb_mouseup
                on:mouseleave=move |_| {
                    selecting_from.set(None);
                    hover_pct.set(None);
                }
            >
                <video
                    node_ref=video_ref
                    class="node-video"
                    poster=thumb_url
                    src=file_url
                    preload="metadata"
                    on:timeupdate=on_timeupdate
                    on:ended=on_ended
                    on:loadedmetadata=move |_| {
                        if let Some(v) = video_ref.get_untracked() {
                            let el: &web_sys::HtmlMediaElement = v.unchecked_ref();
                            let dur = el.duration();
                            if dur.is_finite() && dur > 0.0 && full_duration.get_untracked() == 0.0 {
                                full_duration.set(dur);
                            }
                        }
                    }
                />
                {move || selection.get().map(|(a, b)| {
                    let left = format!("{:.2}%", a * 100.0);
                    let width = format!("{:.2}%", (b - a) * 100.0);
                    view! { <div class="selection-highlight" style:left=left style:width=width></div> }
                })}
                {move || hover_pct.get().map(|pct| {
                    view! {
                        <div class="hover-cursor" style=format!("left:{:.2}%", pct * 100.0)>
                            <span class="hover-time">{format!("{:.0}ms", hover_time_ms.get())}</span>
                        </div>
                    }
                })}
                <div class="playhead" style=move || format!("left: {:.2}%;", playhead_pct.get())></div>
            </div>
            <div class="audio-controls">
                <button class="play-btn-inline" on:click=toggle>
                    {move || if playing.get() {
                        view! {
                            <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
                                <rect x="6" y="4" width="4" height="16"/>
                                <rect x="14" y="4" width="4" height="16"/>
                            </svg>
                        }.into_view()
                    } else {
                        view! {
                            <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
                                <polygon points="6,3 20,12 6,21"/>
                            </svg>
                        }.into_view()
                    }}
                </button>
                {move || selection.get().map(|_| view! {
                    <button class="clear-sel-btn" on:click=clear_selection title="Снять выделение">"✕"</button>
                })}
            </div>
        </div>
    }
}

#[component]
pub fn AudioPlayer(
    wave_url: String,
    file_url: String,
    loop_clip_base: String,
) -> impl IntoView {
    let playing = create_rw_signal(false);
    let playhead_pct = create_rw_signal(0.0_f64);
    let audio_ref = create_node_ref::<html::Audio>();

    // Selection: (start_pct, end_pct) in 0..1
    let selection = create_rw_signal::<Option<(f64, f64)>>(None);
    let selecting_from = create_rw_signal::<Option<f64>>(None);

    let on_wave_mousedown = move |ev: MouseEvent| {
        ev.prevent_default();
        let target = ev.target().unwrap();
        let el = target.unchecked_ref::<web_sys::HtmlElement>();
        let rect = el.get_bounding_client_rect();
        let pct = ((ev.client_x() as f64 - rect.left()) / rect.width()).clamp(0.0, 1.0);
        selecting_from.set(Some(pct));
        selection.set(None);
        // Stop playback on new selection
        if playing.get_untracked() {
            if let Some(audio) = audio_ref.get_untracked() {
                let el: &web_sys::HtmlMediaElement = audio.unchecked_ref();
                el.pause().ok();
                playing.set(false);
            }
        }
    };

    let on_wave_mousemove = move |ev: MouseEvent| {
        let Some(start) = selecting_from.get_untracked() else { return };
        let target = ev.target().unwrap();
        let el = target.unchecked_ref::<web_sys::HtmlElement>();
        let rect = el.get_bounding_client_rect();
        let pct = ((ev.client_x() as f64 - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let (a, b) = if pct < start { (pct, start) } else { (start, pct) };
        if (b - a) > 0.01 {
            selection.set(Some((a, b)));
        }
    };

    let on_wave_mouseup = move |_ev: MouseEvent| {
        selecting_from.set(None);
    };

    let toggle = {
        let file_url = file_url.clone();
        let loop_clip_base = loop_clip_base.clone();
        move |_| {
            let Some(audio) = audio_ref.get_untracked() else { return };
            let el: web_sys::HtmlMediaElement = audio.unchecked_ref::<web_sys::HtmlMediaElement>().clone();
            if playing.get_untracked() {
                el.pause().ok();
                playing.set(false);
                return;
            }
            let needs_new_src = if let Some((a, b)) = selection.get_untracked() {
                let clip_url = format!(
                    "{}&start={:.4}&end={:.4}",
                    loop_clip_base, a, b
                );
                el.set_src(&clip_url);
                el.set_loop(true);
                true
            } else {
                let full = &file_url;
                if !el.src().ends_with(full.split('?').next().unwrap_or(full)) {
                    el.set_src(full);
                    el.set_loop(false);
                    true
                } else {
                    el.set_current_time(0.0);
                    false
                }
            };
            playing.set(true);
            if needs_new_src {
                // Wait for the new source to load, then play
                spawn_local(async move {
                    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
                        let el2 = el.clone();
                        let cb = wasm_bindgen::closure::Closure::once(move || {
                            el2.set_oncanplaythrough(None);
                            resolve.call0(&wasm_bindgen::JsValue::NULL).ok();
                        });
                        el.set_oncanplaythrough(Some(cb.as_ref().unchecked_ref()));
                        cb.forget();
                    });
                    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                    let audio2 = audio_ref.get_untracked();
                    if let Some(a) = audio2 {
                        let m: &web_sys::HtmlMediaElement = a.unchecked_ref();
                        let _ = wasm_bindgen_futures::JsFuture::from(
                            m.play().unwrap()
                        ).await;
                    }
                });
            } else {
                let play_promise = el.play().unwrap();
                spawn_local(async move {
                    let _ = wasm_bindgen_futures::JsFuture::from(play_promise).await;
                });
            }
        }
    };

    let on_timeupdate = move |_| {
        let Some(audio) = audio_ref.get_untracked() else { return };
        let el: &web_sys::HtmlMediaElement = audio.unchecked_ref();
        let dur = el.duration();
        let cur = el.current_time();
        if dur.is_finite() && dur > 0.0 {
            if let Some((a, b)) = selection.get_untracked() {
                // Map clip time to selection position
                let sel_pct = a + (cur / dur) * (b - a);
                playhead_pct.set(sel_pct * 100.0);
            } else {
                playhead_pct.set(cur / dur * 100.0);
            }
        }
    };

    let on_ended = move |_| {
        playing.set(false);
        playhead_pct.set(0.0);
    };

    let clear_selection = move |ev: MouseEvent| {
        ev.stop_propagation();
        selection.set(None);
        if playing.get_untracked() {
            if let Some(audio) = audio_ref.get_untracked() {
                let el: &web_sys::HtmlMediaElement = audio.unchecked_ref();
                el.pause().ok();
                playing.set(false);
            }
        }
    };

    view! {
        <div class="audio-player" class:playing=move || playing.get()>
            <div
                class="waveform-wrap"
                on:mousedown=on_wave_mousedown
                on:mousemove=on_wave_mousemove
                on:mouseup=on_wave_mouseup
                on:mouseleave=move |_| selecting_from.set(None)
            >
                <img class="media-waveform" src=wave_url draggable="false"/>
                {move || selection.get().map(|(a, b)| {
                    let left = format!("{:.2}%", a * 100.0);
                    let width = format!("{:.2}%", (b - a) * 100.0);
                    view! {
                        <div class="selection-highlight" style:left=left style:width=width></div>
                    }
                })}
                <div
                    class="playhead"
                    style=move || format!("left: {:.2}%;", playhead_pct.get())
                ></div>
            </div>
            <div class="audio-controls">
                <button class="play-btn-inline" on:click=toggle>
                    {move || if playing.get() {
                        view! {
                            <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
                                <rect x="6" y="4" width="4" height="16"/>
                                <rect x="14" y="4" width="4" height="16"/>
                            </svg>
                        }.into_view()
                    } else {
                        view! {
                            <svg viewBox="0 0 24 24" width="14" height="14" fill="currentColor">
                                <polygon points="6,3 20,12 6,21"/>
                            </svg>
                        }.into_view()
                    }}
                </button>
                {move || selection.get().map(|_| view! {
                    <button class="clear-sel-btn" on:click=clear_selection title="Снять выделение">
                        "✕"
                    </button>
                })}
            </div>
            <audio
                node_ref=audio_ref
                src=file_url
                preload="metadata"
                on:timeupdate=on_timeupdate
                on:ended=on_ended
            />
        </div>
    }
}

#[component]
pub fn VideoPlayerModal(
    src: String,
    on_close: impl Fn() + Copy + 'static,
) -> impl IntoView {
    view! {
        <div class="modal-backdrop player-backdrop" on:click=move |_| on_close()>
            <div class="player-modal" on:click=|ev| ev.stop_propagation()>
                <div class="player-header">
                    <button class="modal-close" on:click=move |_| on_close()>"✕"</button>
                </div>
                <video
                    class="player-video"
                    src=src
                    controls=true
                    autoplay=true
                />
            </div>
        </div>
    }
}
