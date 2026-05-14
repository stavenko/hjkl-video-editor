use leptos::*;
use uuid::Uuid;
use web_sys::MouseEvent;

use crate::services::api::absolute_url;

#[component]
pub fn OverlayPreviewAnim(
    keyframes: RwSignal<Vec<api_types::OverlayKeyframe>>,
    bg_info: Option<(Uuid, String, Option<f64>)>,
    image_url: Option<String>,
    project_id: Uuid,
) -> impl IntoView {
    let playing = create_rw_signal(false);
    let anim_t = create_rw_signal(0.0_f64); // 0..1 normalized progress

    let start_anim = move |_: MouseEvent| {
        if playing.get_untracked() { return; }
        let kfs = keyframes.get_untracked();
        if kfs.len() < 2 { return; }
        let t_start = kfs.first().unwrap().t_ms;
        let t_end = kfs.last().unwrap().t_ms;
        let duration_ms = t_end - t_start;
        if duration_ms <= 0.0 { return; }

        // Resume from current position
        let start_t = anim_t.get_untracked();
        playing.set(true);

        spawn_local(async move {
            let perf = web_sys::window().unwrap().performance().unwrap();
            let start_wall = perf.now();
            loop {
                if !playing.get_untracked() { break; }
                let elapsed = perf.now() - start_wall;
                let t = (start_t + elapsed / duration_ms).min(1.0);
                anim_t.set(t);
                if t >= 1.0 { playing.set(false); break; }
                wasm_bindgen_futures::JsFuture::from(js_sys::Promise::new(&mut |resolve, _| {
                    web_sys::window().unwrap()
                        .request_animation_frame(&resolve).ok();
                })).await.ok();
            }
        });
    };

    // Interpolate value between keyframes at normalized t
    let interp_val = move |t: f64, get_val: fn(&api_types::OverlayKeyframe) -> f64| -> f64 {
        let kfs = keyframes.get();
        if kfs.is_empty() { return 0.0; }
        if kfs.len() == 1 { return get_val(&kfs[0]); }
        let t_start = kfs.first().unwrap().t_ms;
        let t_end = kfs.last().unwrap().t_ms;
        let dur = t_end - t_start;
        if dur <= 0.0 { return get_val(&kfs[0]); }
        let abs_t = t_start + t * dur;

        // Find segment
        let mut i = 0;
        while i + 1 < kfs.len() && kfs[i + 1].t_ms < abs_t { i += 1; }
        if i + 1 >= kfs.len() { return get_val(&kfs[kfs.len() - 1]); }

        let k0 = &kfs[i];
        let k1 = &kfs[i + 1];
        let seg_dur = k1.t_ms - k0.t_ms;
        let seg_t = if seg_dur > 0.0 { ((abs_t - k0.t_ms) / seg_dur).clamp(0.0, 1.0) } else { 0.0 };

        let eased = match k0.interpolation {
            api_types::Interpolation::Step => 0.0,
            api_types::Interpolation::Linear => seg_t,
            api_types::Interpolation::EaseIn => seg_t * seg_t,
            api_types::Interpolation::EaseOut => 1.0 - (1.0 - seg_t) * (1.0 - seg_t),
            api_types::Interpolation::EaseInOut => {
                if seg_t < 0.5 { 2.0 * seg_t * seg_t } else { 1.0 - (-2.0 * seg_t + 2.0_f64).powi(2) / 2.0 }
            }
            api_types::Interpolation::CatmullRom => seg_t,
        };
        let v0 = get_val(k0);
        let v1 = get_val(k1);
        if matches!(k0.interpolation, api_types::Interpolation::Step) { v0 } else { v0 + (v1 - v0) * eased }
    };

    // Background frame (middle of animation)
    let bg_url = bg_info.as_ref().map(|(id, slug, dur)| {
        let kfs = keyframes.get_untracked();
        let mid_t = if kfs.len() >= 2 {
            let t0 = kfs.first().unwrap().t_ms;
            let t1 = kfs.last().unwrap().t_ms;
            (t0 + t1) / 2.0
        } else { 0.0 };
        let t_norm = if let Some(d) = dur {
            if *d > 0.0 { (mid_t / 1000.0 / d).clamp(0.0, 1.0) as f32 } else { 0.0 }
        } else { 0.0 };
        absolute_url(&format!(
            "/api/projects/{}/nodes/{}/{}/thumbnail?t={}&w=640",
            project_id, slug, id, t_norm
        ))
    });

    let preview_open = create_rw_signal(false);
    let scrubbing = create_rw_signal(false);
    let bar_ref = create_node_ref::<leptos::html::Div>();

    let toggle_preview = move |ev: MouseEvent| {
        ev.stop_propagation();
        let open = !preview_open.get_untracked();
        preview_open.set(open);
        if !open { playing.set(false); }
    };

    let toggle_play = move |ev: MouseEvent| {
        ev.stop_propagation();
        if playing.get_untracked() {
            playing.set(false);
        } else {
            start_anim(ev);
        }
    };

    let scrub_to = move |client_x: i32| {
        if let Some(el) = bar_ref.get_untracked() {
            let rect = el.get_bounding_client_rect();
            let t = ((client_x as f64 - rect.left()) / rect.width()).clamp(0.0, 1.0);
            playing.set(false);
            anim_t.set(t);
        }
    };

    view! {
        <button class="overlay-preview-btn"
            on:click=toggle_preview
            on:mousedown=|ev: MouseEvent| ev.stop_propagation()
        >
            {move || if preview_open.get() { "▾ Preview" } else { "▸ Preview" }}
        </button>
        <Show when=move || preview_open.get()>
            <div class="overlay-preview" style="min-height:120px;">
                {bg_url.clone().map(|url| view! {
                    <img class="overlay-bg-frame" src=url draggable="false" />
                })}
                {image_url.clone().map(|url| view! {
                    <img class="overlay-img-preview" src=url draggable="false"
                        style=move || {
                            let t = anim_t.get();
                            let x = interp_val(t, |k| k.x) * 100.0;
                            let y = interp_val(t, |k| k.y) * 100.0;
                            let s = interp_val(t, |k| k.scale) * 100.0;
                            let a = interp_val(t, |k| k.alpha);
                            let r = interp_val(t, |k| k.corner_radius);
                            format!(
                                "left:{:.1}%;top:{:.1}%;width:{:.1}%;transform:translate(-50%,-50%);opacity:{:.2};border-radius:{:.1}px;",
                                x, y, s, a, r
                            )
                        }
                    />
                })}
            </div>
            <div class="overlay-anim-controls">
                <button class="overlay-play-btn"
                    on:click=toggle_play
                    on:mousedown=|ev: MouseEvent| ev.stop_propagation()
                >
                    {move || if playing.get() { "⏸" } else { "⏵" }}
                </button>
                <div class="overlay-anim-bar"
                    node_ref=bar_ref
                    on:mousedown=move |ev: MouseEvent| {
                        ev.stop_propagation();
                        ev.prevent_default();
                        scrubbing.set(true);
                        scrub_to(ev.client_x());
                    }
                    on:mousemove=move |ev: MouseEvent| {
                        if scrubbing.get_untracked() {
                            scrub_to(ev.client_x());
                        }
                    }
                    on:mouseup=move |_| scrubbing.set(false)
                    on:mouseleave=move |_| scrubbing.set(false)
                >
                    <div class="overlay-anim-fill" style=move || format!("width:{:.1}%", anim_t.get() * 100.0)></div>
                    <div class="overlay-anim-thumb" style=move || format!("left:{:.1}%", anim_t.get() * 100.0)></div>
                </div>
                <span class="overlay-anim-time">{move || {
                    let kfs = keyframes.get();
                    if kfs.len() >= 2 {
                        let t0 = kfs.first().unwrap().t_ms;
                        let t1 = kfs.last().unwrap().t_ms;
                        let cur = t0 + anim_t.get() * (t1 - t0);
                        format!("{:.0}ms", cur)
                    } else {
                        String::new()
                    }
                }}</span>
            </div>
        </Show>
    }
}

#[component]
pub fn OverlayKfEditor(
    index: usize,
    keyframes: RwSignal<Vec<api_types::OverlayKeyframe>>,
    bg_info: Option<(Uuid, String, Option<f64>)>,
    image_url: Option<String>,
    project_id: Uuid,
    on_change: impl Fn() + Copy + 'static,
) -> impl IntoView {
    let kf = keyframes.with_untracked(|kfs| kfs[index].clone());

    // Background frame URL
    let bg_url = bg_info.as_ref().map(|(id, slug, dur)| {
        let t_norm = if let Some(d) = dur {
            if *d > 0.0 { (kf.t_ms / 1000.0 / d).clamp(0.0, 1.0) as f32 } else { 0.0 }
        } else { 0.0 };
        absolute_url(&format!(
            "/api/projects/{}/nodes/{}/{}/thumbnail?t={}&w=640",
            project_id, slug, id, t_norm
        ))
    });

    let drag_active = create_rw_signal(false);
    let local_x = create_rw_signal(kf.x);
    let local_y = create_rw_signal(kf.y);
    let local_scale = create_rw_signal(kf.scale);
    let local_alpha = create_rw_signal(kf.alpha);
    let local_radius = create_rw_signal(kf.corner_radius);
    let local_interp = create_rw_signal(kf.interpolation);

    let commit = move || {
        keyframes.update(|kfs| {
            if let Some(kf) = kfs.get_mut(index) {
                kf.x = local_x.get_untracked();
                kf.y = local_y.get_untracked();
                kf.scale = local_scale.get_untracked();
                kf.alpha = local_alpha.get_untracked();
                kf.corner_radius = local_radius.get_untracked();
                kf.interpolation = local_interp.get_untracked();
            }
        });
        on_change();
    };

    let preview_ref = create_node_ref::<leptos::html::Div>();

    let on_preview_mousedown = move |ev: MouseEvent| {
        ev.prevent_default();
        ev.stop_propagation();
        drag_active.set(true);
        if let Some(el) = preview_ref.get_untracked() {
            let rect = el.get_bounding_client_rect();
            let nx = (ev.client_x() as f64 - rect.left()) / rect.width();
            let ny = (ev.client_y() as f64 - rect.top()) / rect.height();
            local_x.set(nx);
            local_y.set(ny);
        }
    };

    let on_preview_mousemove = move |ev: MouseEvent| {
        if !drag_active.get_untracked() { return; }
        ev.prevent_default();
        ev.stop_propagation();
        if let Some(el) = preview_ref.get_untracked() {
            let rect = el.get_bounding_client_rect();
            let nx = (ev.client_x() as f64 - rect.left()) / rect.width();
            let ny = (ev.client_y() as f64 - rect.top()) / rect.height();
            local_x.set(nx);
            local_y.set(ny);
        }
    };

    let on_preview_mouseup = move |ev: MouseEvent| {
        ev.stop_propagation();
        if drag_active.get_untracked() {
            drag_active.set(false);
            commit();
        }
    };

    let img_url_for_view = image_url.clone();
    let has_bg = bg_url.is_some();
    let bg_loaded = create_rw_signal(!has_bg);

    view! {
        <div class="overlay-visual-editor">
            <div class=move || if bg_loaded.get() { "overlay-preview loaded" } else { "overlay-preview" }
                node_ref=preview_ref
                on:mousedown=on_preview_mousedown
                on:mousemove=on_preview_mousemove
                on:mouseup=on_preview_mouseup
                on:mouseleave=move |_| {
                    if drag_active.get_untracked() {
                        drag_active.set(false);
                        commit();
                    }
                }
            >
                <Show when=move || !bg_loaded.get()>
                    <div class="overlay-loading">"Загрузка кадра..."</div>
                </Show>
                {bg_url.map(|url| view! {
                    <img class="overlay-bg-frame" src=url draggable="false"
                        on:load=move |_| bg_loaded.set(true)
                    />
                })}
                {img_url_for_view.map(|url| view! {
                    <img class="overlay-img-preview" src=url draggable="false"
                        style=move || {
                            let x_pct = local_x.get() * 100.0;
                            let y_pct = local_y.get() * 100.0;
                            let s = local_scale.get() * 100.0;
                            let a = local_alpha.get();
                            let r = local_radius.get();
                            format!(
                                "left:{:.1}%;top:{:.1}%;width:{:.1}%;transform:translate(-50%,-50%);opacity:{:.2};border-radius:{:.1}px;",
                                x_pct, y_pct, s, a, r
                            )
                        }
                    />
                })}
                <div class="overlay-crosshair" style=move || {
                    format!("left:{:.1}%;top:{:.1}%;", local_x.get() * 100.0, local_y.get() * 100.0)
                }></div>
            </div>
            <div class="overlay-sliders">
                <label class="overlay-slider-row">
                    <span>"scale"</span>
                    <input type="range" min="0.05" max="3.0" step="0.01"
                        prop:value=move || format!("{}", local_scale.get())
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                local_scale.set(v);
                            }
                        }
                        on:change=move |_| commit()
                    />
                    <span class="overlay-slider-val">{move || format!("{:.2}", local_scale.get())}</span>
                </label>
                <label class="overlay-slider-row">
                    <span>"alpha"</span>
                    <input type="range" min="0" max="1.0" step="0.01"
                        prop:value=move || format!("{}", local_alpha.get())
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                local_alpha.set(v);
                            }
                        }
                        on:change=move |_| commit()
                    />
                    <span class="overlay-slider-val">{move || format!("{:.2}", local_alpha.get())}</span>
                </label>
                <label class="overlay-slider-row">
                    <span>"radius"</span>
                    <input type="range" min="0" max="100" step="1"
                        prop:value=move || format!("{}", local_radius.get())
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                local_radius.set(v);
                            }
                        }
                        on:change=move |_| commit()
                    />
                    <span class="overlay-slider-val">{move || format!("{:.0}", local_radius.get())}</span>
                </label>
                <div class="overlay-xy-row">
                    <label class="overlay-xy-field">
                        <span>"x"</span>
                        <input type="text"
                            prop:value=move || format!("{:.3}", local_x.get())
                            on:change=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                    local_x.set(v);
                                    commit();
                                }
                            }
                            on:mousedown=|ev: MouseEvent| ev.stop_propagation()
                        />
                    </label>
                    <label class="overlay-xy-field">
                        <span>"y"</span>
                        <input type="text"
                            prop:value=move || format!("{:.3}", local_y.get())
                            on:change=move |ev| {
                                if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                    local_y.set(v);
                                    commit();
                                }
                            }
                            on:mousedown=|ev: MouseEvent| ev.stop_propagation()
                        />
                    </label>
                </div>
            </div>
            {move || {
                let kfs = keyframes.get();
                if kfs.len() > 1 {
                    let copy_open = create_rw_signal(false);
                    let cur_idx = index;
                    Some(view! {
                        <div class="overlay-copy-wrap">
                            <button class="overlay-copy-btn" on:click=move |ev: MouseEvent| {
                                ev.stop_propagation();
                                copy_open.update(|v| *v = !*v);
                            }>"Copy from..."</button>
                            <Show when=move || copy_open.get()>
                                <div class="overlay-copy-menu">
                                    {kfs.iter().enumerate().filter(|(i, _)| *i != cur_idx).map(|(i, kf)| {
                                        let t = kf.t_ms;
                                        let x = kf.x;
                                        let y = kf.y;
                                        let s = kf.scale;
                                        let a = kf.alpha;
                                        let r = kf.corner_radius;
                                        let interp = kf.interpolation;
                                        view! {
                                            <button class="overlay-copy-item" on:click=move |ev: MouseEvent| {
                                                ev.stop_propagation();
                                                local_x.set(x);
                                                local_y.set(y);
                                                local_scale.set(s);
                                                local_alpha.set(a);
                                                local_radius.set(r);
                                                local_interp.set(interp);
                                                copy_open.set(false);
                                                commit();
                                            }>{format!("{:.0}ms", t)}</button>
                                        }
                                    }).collect_view()}
                                </div>
                            </Show>
                        </div>
                    })
                } else {
                    None
                }
            }}
        </div>
    }
}
