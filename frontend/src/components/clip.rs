use leptos::*;
use uuid::Uuid;
use wasm_bindgen::prelude::*;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, HtmlImageElement, MouseEvent};

use crate::services::api::absolute_url;


#[component]
pub fn ClipKfEditor(
    index: usize,
    keyframes: RwSignal<Vec<api_types::OverlayKeyframe>>,
    bg_info: Option<(Uuid, String, Option<f64>)>,
    project_id: Uuid,
    on_change: impl Fn() + Copy + 'static,
) -> impl IntoView {
    let kf = keyframes.with_untracked(|kfs| kfs[index].clone());

    let local_x = create_rw_signal(kf.x);
    let local_y = create_rw_signal(kf.y);
    let local_scale = create_rw_signal(kf.scale);
    let local_alpha = create_rw_signal(kf.alpha);
    let local_radius = create_rw_signal(kf.corner_radius);
    let local_border_width = create_rw_signal(kf.border_width);
    let local_border_color = create_rw_signal(kf.border_color.clone());

    let drag_active = create_rw_signal(false);
    let drag_start = create_rw_signal((0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64));

    let canvas_ref = create_node_ref::<leptos::html::Canvas>();
    let img_loaded = create_rw_signal(false);

    let frame_url = bg_info.as_ref().map(|(id, slug, dur)| {
        let t_norm = if let Some(d) = dur {
            if *d > 0.0 { (kf.t_ms / 1000.0 / d).clamp(0.0, 1.0) as f32 } else { 0.0 }
        } else { 0.0 };
        absolute_url(&format!(
            "/api/projects/{}/nodes/{}/{}/thumbnail?t={}&w=640",
            project_id, slug, id, t_norm
        ))
    });

    let img_el = create_rw_signal::<Option<HtmlImageElement>>(None);
    let canvas_w = create_rw_signal(1_u32);
    let canvas_h = create_rw_signal(1_u32);

    if let Some(url) = frame_url.clone() {
        let img = HtmlImageElement::new().unwrap();
        let img_for_store = img.clone();
        let img_for_load = img.clone();
        let onload = Closure::wrap(Box::new(move || {
            let nw = img_for_load.natural_width();
            let nh = img_for_load.natural_height();
            if nw > 0 && nh > 0 {
                canvas_w.set(nw);
                canvas_h.set(nh);
            }
            img_loaded.set(true);
        }) as Box<dyn Fn()>);
        img.set_onload(Some(onload.as_ref().unchecked_ref()));
        onload.forget();
        let img_for_err = img.clone();
        let retry_count = std::rc::Rc::new(std::cell::Cell::new(0_u32));
        let retry_count2 = retry_count.clone();
        let onerror = Closure::wrap(Box::new(move || {
            let attempts = retry_count2.get();
            if attempts < 3 {
                retry_count2.set(attempts + 1);
                let img_retry = img_for_err.clone();
                let src = img_retry.src();
                // Retry after 1 second
                let cb = Closure::once_into_js(move || {
                    img_retry.set_src(&src);
                });
                web_sys::window().unwrap()
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        cb.as_ref().unchecked_ref(), 1000
                    ).ok();
            } else {
                web_sys::console::error_1(&format!(
                    "ClipKfEditor: failed to load after retries: {}", img_for_err.src()
                ).into());
                img_loaded.set(true);
            }
        }) as Box<dyn Fn()>);
        img.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();
        img.set_src(&url);
        img_el.set(Some(img_for_store));
    } else {
        img_loaded.set(true);
    }

    let redraw = move || {
        let Some(canvas) = canvas_ref.get_untracked() else { return };
        let el: &HtmlCanvasElement = &canvas;
        let ctx = el
            .get_context("2d")
            .ok()
            .flatten()
            .and_then(|c| c.dyn_into::<CanvasRenderingContext2d>().ok());
        let Some(ctx) = ctx else { return };

        let cw = el.width() as f64;
        let ch = el.height() as f64;
        if cw == 0.0 || ch == 0.0 { return; }

        ctx.set_fill_style_str("#1a1a2e");
        ctx.fill_rect(0.0, 0.0, cw, ch);

        let Some(img) = img_el.get_untracked() else { return };
        if !img_loaded.get_untracked() { return; }

        let nw = img.natural_width() as f64;
        let nh = img.natural_height() as f64;
        if nw == 0.0 || nh == 0.0 { return; }

        let scale = local_scale.get_untracked();
        let x = local_x.get_untracked();
        let y = local_y.get_untracked();
        let alpha = local_alpha.get_untracked();
        let radius = local_radius.get_untracked();
        let bw = local_border_width.get_untracked();
        let bc = local_border_color.get_untracked();

        // scale=1 → frame fills canvas exactly (canvas matches video aspect ratio)
        let dw = cw * scale;
        let dh = ch * scale;
        let dx = x * cw - dw / 2.0;
        let dy = y * ch - dh / 2.0;
        let scale_ratio = dw / nw; // output pixels → canvas pixels
        let bw_px = bw * scale_ratio;
        let total_radius = radius + bw;
        let outer_r_px = total_radius * scale_ratio;

        ctx.save();
        ctx.set_global_alpha(alpha);

        // Draw border (outer rounded rect)
        if bw > 0.0 && outer_r_px > 0.5 {
            ctx.set_fill_style_str(&bc);
            ctx.begin_path();
            round_rect(&ctx, dx, dy, dw, dh, outer_r_px);
            ctx.fill();
        }

        // Clip inner area and draw video
        let inner_r_px = if radius > 0.5 { radius * scale_ratio } else { 0.0 };
        if inner_r_px > 0.5 || bw_px > 0.5 {
            ctx.begin_path();
            round_rect(&ctx, dx + bw_px, dy + bw_px, dw - 2.0 * bw_px, dh - 2.0 * bw_px, inner_r_px.max(0.0));
            ctx.clip();
        }

        let _ = ctx.draw_image_with_html_image_element_and_dw_and_dh(
            &img, dx + bw_px, dy + bw_px, dw - 2.0 * bw_px, dh - 2.0 * bw_px
        );
        ctx.restore();

        // Crosshair
        ctx.set_stroke_style_str("rgba(255,255,0,0.7)");
        ctx.set_line_width(1.0);
        let cx = x * cw;
        let cy = y * ch;
        ctx.begin_path();
        ctx.move_to(cx - 10.0, cy);
        ctx.line_to(cx + 10.0, cy);
        ctx.move_to(cx, cy - 10.0);
        ctx.line_to(cx, cy + 10.0);
        ctx.stroke();
    };

    create_effect(move |_| {
        let _ = local_x.get();
        let _ = local_y.get();
        let _ = local_scale.get();
        let _ = local_alpha.get();
        let _ = local_radius.get();
        let _ = img_loaded.get();
        let _ = canvas_w.get();
        let _ = canvas_h.get();
        let _ = local_border_width.get();
        let _ = local_border_color.get();
        redraw();
    });

    let commit = move || {
        keyframes.update(|kfs| {
            if let Some(kf) = kfs.get_mut(index) {
                kf.x = local_x.get_untracked();
                kf.y = local_y.get_untracked();
                kf.scale = local_scale.get_untracked();
                kf.alpha = local_alpha.get_untracked();
                kf.corner_radius = local_radius.get_untracked();
                kf.border_width = local_border_width.get_untracked();
                kf.border_color = local_border_color.get_untracked();
            }
        });
        on_change();
    };

    let on_mousedown = move |ev: MouseEvent| {
        ev.prevent_default();
        ev.stop_propagation();
        drag_active.set(true);
        drag_start.set((
            ev.client_x() as f64,
            ev.client_y() as f64,
            local_x.get_untracked(),
            local_y.get_untracked(),
        ));
    };

    let on_mousemove = move |ev: MouseEvent| {
        if !drag_active.get_untracked() { return; }
        ev.prevent_default();
        ev.stop_propagation();
        let Some(canvas) = canvas_ref.get_untracked() else { return };
        let rect = canvas.get_bounding_client_rect();
        let (start_mx, start_my, orig_x, orig_y) = drag_start.get_untracked();
        let dx = (ev.client_x() as f64 - start_mx) / rect.width();
        let dy = (ev.client_y() as f64 - start_my) / rect.height();
        local_x.set(orig_x + dx);
        local_y.set(orig_y + dy);
    };

    let on_mouseup = move |ev: MouseEvent| {
        ev.stop_propagation();
        if drag_active.get_untracked() {
            drag_active.set(false);
            commit();
        }
    };

    view! {
        <div class="clip-visual-editor">
            <canvas
                node_ref=canvas_ref
                attr:width=move || canvas_w.get().to_string()
                attr:height=move || canvas_h.get().to_string()
                style="width:100%;height:auto;cursor:move;border-radius:4px;"
                on:mousedown=on_mousedown
                on:mousemove=on_mousemove
                on:mouseup=on_mouseup
                on:mouseleave=move |_| {
                    if drag_active.get_untracked() {
                        drag_active.set(false);
                        commit();
                    }
                }
            />
            <Show when=move || !img_loaded.get()>
                <div class="overlay-loading">"Загрузка кадра..."</div>
            </Show>
            <div class="overlay-sliders">
                <label class="overlay-slider-row">
                    <span>"scale"</span>
                    <input type="range" min="0.1" max="5.0" step="0.01"
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
                <label class="overlay-slider-row">
                    <span>"border"</span>
                    <input type="range" min="0" max="20" step="1"
                        prop:value=move || format!("{}", local_border_width.get())
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<f64>() {
                                local_border_width.set(v);
                            }
                        }
                        on:change=move |_| commit()
                    />
                    <span class="overlay-slider-val">{move || format!("{:.0}", local_border_width.get())}</span>
                </label>
                <label class="overlay-xy-field">
                    <span>"border_color"</span>
                    <input type="color"
                        prop:value=move || local_border_color.get()
                        on:input=move |ev| { local_border_color.set(event_target_value(&ev)); }
                        on:change=move |_| commit()
                        on:mousedown=|ev: MouseEvent| ev.stop_propagation()
                    />
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
                                    {kfs.iter().enumerate().filter(|(i, _)| *i != cur_idx).map(|(_i, kf)| {
                                        let x = kf.x;
                                        let y = kf.y;
                                        let s = kf.scale;
                                        let a = kf.alpha;
                                        let r = kf.corner_radius;
                                        let bw = kf.border_width;
                                        let bcolor = kf.border_color.clone();
                                        let t = kf.t_ms;
                                        view! {
                                            <button class="overlay-copy-item" on:click=move |ev: MouseEvent| {
                                                ev.stop_propagation();
                                                local_x.set(x);
                                                local_y.set(y);
                                                local_scale.set(s);
                                                local_alpha.set(a);
                                                local_radius.set(r);
                                                local_border_width.set(bw);
                                                local_border_color.set(bcolor.clone());
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

fn round_rect(ctx: &CanvasRenderingContext2d, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    ctx.move_to(x + r, y);
    ctx.line_to(x + w - r, y);
    ctx.arc_to(x + w, y, x + w, y + r, r).ok();
    ctx.line_to(x + w, y + h - r);
    ctx.arc_to(x + w, y + h, x + w - r, y + h, r).ok();
    ctx.line_to(x + r, y + h);
    ctx.arc_to(x, y + h, x, y + h - r, r).ok();
    ctx.line_to(x, y + r);
    ctx.arc_to(x, y, x + r, y, r).ok();
    ctx.close_path();
}
