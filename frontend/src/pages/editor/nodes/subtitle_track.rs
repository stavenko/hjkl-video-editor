use api_types::{Edge, Node, NodeKind, ProcessNodeKind, SubtitleSegment};
use leptos::*;
use uuid::Uuid;
use wasm_bindgen::JsCast;
use web_sys::{MouseEvent, WheelEvent};

use crate::components::helpers::{parse_ass_time, wrap_text};
use crate::services::api::absolute_url;
use crate::services::project_service;

pub fn subtitle_track_body(
    node_signal: RwSignal<Node>,
    project_id: Uuid,
    id_for_drag: Uuid,
    edges: RwSignal<Vec<Edge>>,
    nodes: RwSignal<Vec<Node>>,
    st_style_modal: RwSignal<Option<RwSignal<Node>>>,
    st_ctx_menu: RwSignal<Option<(usize, f64, f64, RwSignal<Node>, RwSignal<Vec<SubtitleSegment>>, bool, usize)>>,
) -> impl IntoView {
    let n = node_signal.get_untracked();
    let st_nid = n.id;
    let preview_loaded = create_rw_signal(0_u32);
    let preview_reload = create_rw_signal(0_u32);
    let canvas_visible = create_rw_signal(false);
    // Local segments signal — the actual editable data
    let local_segments = create_rw_signal(
        match &n.settings {
            Some(api_types::NodeSettings::SubtitleTrack { segments, .. }) => segments.clone(),
            _ => Vec::new(),
        }
    );

    // Load segments from output reactively
    let segments = create_rw_signal::<Vec<(String, f64, f64)>>(Vec::new());
    let last_sz = create_rw_signal(0_u64);
    create_effect(move |_| {
        let ns = node_signal.get();
        let Some(output) = &ns.output else { return; };
        let sz = output.size_bytes;
        if sz == last_sz.get_untracked() { return; }
        last_sz.set(sz);
        let url = absolute_url(&format!(
            "/api/projects/{}/nodes/{}/{}/file?t={}",
            project_id, ProcessNodeKind::SubtitleTrack.url_slug(), st_nid, sz
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
                            let mut segs = Vec::new();
                            for line in s.lines() {
                                if let Some(rest) = line.strip_prefix("Dialogue:") {
                                    let parts: Vec<&str> = rest.splitn(10, ',').collect();
                                    if parts.len() >= 10 {
                                        let text = parts[9].trim().to_string();
                                        let start = parse_ass_time(parts[1].trim());
                                        let end = parse_ass_time(parts[2].trim());
                                        if !text.is_empty() { segs.push((text, start, end)); }
                                    }
                                }
                            }
                            segments.set(segs);
                        }
                    }
                }
            }
        });
    });

    let save_settings = move |settings: api_types::NodeSettings| {
        if let api_types::NodeSettings::SubtitleTrack { ref segments, .. } = settings {
            local_segments.set(segments.clone());
        }
        spawn_local(async move {
            let _ = project_service::update_node_settings(project_id, id_for_drag, settings).await;
            let _ = project_service::run_node(project_id, id_for_drag).await;
            preview_reload.update(|v| *v += 1);
        });
    };

    let cursor_px = create_rw_signal(0.0_f64);
    let cursor_ms = create_rw_signal(0.0_f64);
    let tl_zoom = create_rw_signal(1.0_f64);
    let tl_offset = create_rw_signal(0.0_f64);
    let tl_ref = create_node_ref::<leptos::html::Div>();
    // Drag state for timeline boundaries
    let dragging = create_rw_signal::<Option<(usize, bool, Option<usize>)>>(None);
    // Drag state for subtitle position in preview
    let sub_dragging = create_rw_signal::<Option<usize>>(None);
    let preview_ref = create_node_ref::<leptos::html::Div>();

    // Find background video for preview — reactive
    let bg_sig = Signal::derive(move || {
        let es = edges.get();
        let ns = nodes.get();
        es.iter()
            .find(|e| e.to_node == st_nid && e.to_port == "video")
            .and_then(|e| {
                let src = ns.iter().find(|n| n.id == e.from_node)?;
                let resolved = match src.kind {
                    NodeKind::Reference { source } => api_types::resolve_reference(&ns, source)?,
                    _ => src,
                };
                let slug = match resolved.kind {
                    NodeKind::Input(ik) => ik.url_slug().to_string(),
                    NodeKind::Process(pk) => pk.url_slug().to_string(),
                    _ => return None,
                };
                let dur = resolved.asset.as_ref().and_then(|a| a.duration_secs)
                    .or_else(|| resolved.output.as_ref().and_then(|o| o.duration_ms.map(|d| d / 1000.0)));
                Some((resolved.id, slug, dur))
            })
    });

    view! {
        <div class="subtrack-editor"
            on:mousedown=|ev: MouseEvent| ev.stop_propagation()
            on:mousemove=|ev: MouseEvent| ev.stop_propagation()
        >
            // Header
            <div class="subtrack-header">
                <span class="subtrack-info">{move || format!("{} сегм.", segments.get().len())}</span>
                <span class="subtrack-cursor-time">{move || format!("{:.0}ms", cursor_ms.get())}</span>
                <button class="subtrack-btn" on:click=move |_| st_style_modal.set(Some(node_signal))>"Стили"</button>
            </div>

            // Preview frame with subtitle overlay
            <div class="subtrack-preview-wrap" node_ref=preview_ref
                on:mousedown=move |ev: MouseEvent| {
                    ev.stop_propagation();
                    ev.prevent_default();
                    // Find closest visible segment to click position
                    let ms = cursor_ms.get_untracked();
                    let ls = local_segments.get_untracked();
                    if let Some(el) = preview_ref.get_untracked() {
                        let rect = el.get_bounding_client_rect();
                        let css_w = el.client_width() as f64;
                        let css_h = el.client_height() as f64;
                        let scale = if css_w > 0.0 { rect.width() / css_w } else { 1.0 };
                        let click_x = ((ev.client_x() as f64 - rect.left()) / scale / css_w).clamp(0.0, 1.0);
                        let click_y = ((ev.client_y() as f64 - rect.top()) / scale / css_h).clamp(0.0, 1.0);

                        // Find nearest visible segment by distance to its pos
                        let seg_idx = ls.iter().enumerate()
                            .filter(|(_, seg)| ms >= seg.start_ms && ms <= seg.end_ms)
                            .min_by(|(_, a), (_, b)| {
                                let da = (a.pos_x - click_x).powi(2) + (a.pos_y - click_y).powi(2);
                                let db = (b.pos_x - click_x).powi(2) + (b.pos_y - click_y).powi(2);
                                da.partial_cmp(&db).unwrap()
                            })
                            .map(|(i, _)| i);

                        if let Some(idx) = seg_idx {
                            sub_dragging.set(Some(idx));
                            canvas_visible.set(true);
                        }
                    }
                }
                on:mousemove=move |ev: MouseEvent| {
                    if let Some(seg_idx) = sub_dragging.get_untracked() {
                        if let Some(el) = preview_ref.get_untracked() {
                            let rect = el.get_bounding_client_rect();
                            let css_w = el.client_width() as f64;
                            let css_h = el.client_height() as f64;
                            let scale = if css_w > 0.0 { rect.width() / css_w } else { 1.0 };
                            let nx = ((ev.client_x() as f64 - rect.left()) / scale / css_w).clamp(0.0, 1.0);
                            let ny = ((ev.client_y() as f64 - rect.top()) / scale / css_h).clamp(0.0, 1.0);
                            local_segments.update(|segs| {
                                if let Some(s) = segs.get_mut(seg_idx) {
                                    s.pos_x = nx;
                                    s.pos_y = ny;
                                }
                            });
                        }
                    }
                }
                on:mouseup=move |_| {
                    if sub_dragging.get_untracked().is_some() {
                        sub_dragging.set(None);
                        // Save + run + force preview reload
                        let segs = local_segments.get_untracked();
                        let ns = node_signal.get_untracked();
                        let (st, rx, ry, fp) = match &ns.settings {
                            Some(api_types::NodeSettings::SubtitleTrack { styles, resolution_x, resolution_y, fps, .. }) =>
                                (styles.clone(), *resolution_x, *resolution_y, *fps),
                            _ => (vec![], 1920, 1080, 30),
                        };
                        let settings = api_types::NodeSettings::SubtitleTrack {
                            styles: st, segments: segs, resolution_x: rx, resolution_y: ry, fps: fp,
                        };
                        spawn_local(async move {
                            let _ = project_service::update_node_settings(project_id, id_for_drag, settings).await;
                            let _ = project_service::run_node(project_id, id_for_drag).await;
                            // Force preview reload
                            preview_reload.update(|v| *v += 1);
                        });
                    }
                }
                on:mouseleave=move |_| { sub_dragging.set(None); }
            >
                {
                    let preview_src = create_rw_signal(String::new());
                    let preview_loading = create_rw_signal(false);
                    let last_requested_ms = create_rw_signal(-1.0_f64);
                    let abort_ctrl = create_rw_signal::<Option<web_sys::AbortController>>(None);

                    create_effect(move |_| {
                        let reload = preview_reload.get();
                        let ms = (cursor_ms.get() / 200.0).round() * 200.0;
                        let key = ms + reload as f64 * 0.001; // unique key
                        if key == last_requested_ms.get_untracked() { return; }
                        last_requested_ms.set(key);

                        let bg = bg_sig.get();
                        let Some((id, slug, dur)) = bg else { return; };

                        // Abort previous request
                        if let Some(ctrl) = abort_ctrl.get_untracked() {
                            ctrl.abort();
                        }

                        let t_norm = if let Some(d) = dur {
                            if d > 0.0 { (ms / 1000.0 / d).clamp(0.0, 1.0) as f32 } else { 0.0 }
                        } else { 0.0 };

                        // Use subtitle-preview endpoint if we have segments
                        let has_subs = !local_segments.get_untracked().is_empty();
                        web_sys::console::log_1(&format!("ST preview: has_subs={}, t={:.4}", has_subs, t_norm).into());
                        let url = if has_subs {
                            absolute_url(&format!(
                                "/api/projects/{}/subtitle-preview?video_node={}&video_slug={}&subs_node={}&t={:.4}&w=640",
                                project_id, id, slug, st_nid, t_norm
                            ))
                        } else {
                            absolute_url(&format!(
                                "/api/projects/{}/nodes/{}/{}/thumbnail?t={:.4}&w=640",
                                project_id, slug, id, t_norm
                            ))
                        };

                        let ctrl = web_sys::AbortController::new().unwrap();
                        let signal = ctrl.signal();
                        abort_ctrl.set(Some(ctrl));
                        preview_loading.set(true);

                        spawn_local(async move {
                            let window = web_sys::window().unwrap();
                            let mut opts = web_sys::RequestInit::new();
                            opts.signal(Some(&signal));
                            opts.cache(web_sys::RequestCache::NoStore);
                            let req = web_sys::Request::new_with_str_and_init(&url, &opts).unwrap();
                            if let Ok(resp) = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&req)).await {
                                let resp: web_sys::Response = resp.unchecked_into();
                                if let Ok(blob_p) = resp.blob() {
                                    if let Ok(blob) = wasm_bindgen_futures::JsFuture::from(blob_p).await {
                                        let blob: web_sys::Blob = blob.unchecked_into();
                                        if let Ok(obj_url) = web_sys::Url::create_object_url_with_blob(&blob) {
                                            // Revoke old URL
                                            let old = preview_src.get_untracked();
                                            if !old.is_empty() {
                                                let _ = web_sys::Url::revoke_object_url(&old);
                                            }
                                            preview_src.set(obj_url);
                                        }
                                    }
                                }
                            }
                            preview_loading.set(false);
                        });
                    });

                    view! {
                        <Show when=move || preview_loading.get() && preview_src.get().is_empty()>
                            <div class="subtrack-preview-spinner">"..."</div>
                        </Show>
                        <img class="subtrack-preview-frame" draggable="false"
                            class:loading=move || preview_loading.get()
                            src=move || preview_src.get()
                            on:load=move |_| {
                                preview_loaded.update(|v| *v += 1);
                                canvas_visible.set(false);
                            }
                        />
                    }
                }
                // Canvas overlay for subtitle text
                {
                    let subs_canvas_ref = create_node_ref::<leptos::html::Canvas>();

                    create_effect(move |_| {
                        let _ = preview_loaded.get();
                        let _ = sub_dragging.get();
                        let ms = cursor_ms.get();
                        let ls = local_segments.get();
                        let styles = match &node_signal.get_untracked().settings {
                            Some(api_types::NodeSettings::SubtitleTrack { styles, .. }) => styles.clone(),
                            _ => vec![api_types::SubtitleStyle::default()],
                        };
                        let default_style = styles.first().cloned().unwrap_or_default();

                        let Some(canvas) = subs_canvas_ref.get() else { return };
                        let Some(parent) = preview_ref.get() else { return };
                        let pw = parent.client_width() as u32;
                        let ph = parent.client_height() as u32;
                        if pw == 0 || ph == 0 { return; }

                        canvas.set_width(pw);
                        canvas.set_height(ph);

                        let ctx: web_sys::CanvasRenderingContext2d = canvas
                            .get_context("2d").ok().flatten()
                            .unwrap()
                            .unchecked_into();
                        ctx.clear_rect(0.0, 0.0, pw as f64, ph as f64);

                        let res_y = match &node_signal.get_untracked().settings {
                            Some(api_types::NodeSettings::SubtitleTrack { resolution_y, .. }) => *resolution_y as f64,
                            _ => 1080.0,
                        };
                        let dragged_idx = sub_dragging.get();

                        for (seg_i, seg) in ls.iter().enumerate() {
                            if ms < seg.start_ms || ms > seg.end_ms { continue; }
                            let st = seg.style_name.as_ref()
                                .and_then(|sn| styles.iter().find(|s| s.name == *sn))
                                .unwrap_or(&default_style);

                            // ASS font size is ~75% of CSS/Canvas font size
                            let font_px = st.size as f64 * ph as f64 / res_y * 0.9;
                            let x = seg.pos_x * pw as f64;
                            let y = (seg.pos_y - 0.01) * ph as f64;

                            let weight = if st.bold { "bold " } else { "" };
                            ctx.set_font(&format!("{}{:.0}px {}", weight, font_px, st.font));
                            ctx.set_text_align("center");

                            // Word wrap: break text to fit canvas width
                            let max_w = pw as f64 * 0.9;
                            let lines = wrap_text(&ctx, &seg.text, max_w);
                            let line_h = font_px * 1.2;
                            let total_h = lines.len() as f64 * line_h;
                            let y_start = y - total_h + line_h;

                            for (li, line) in lines.iter().enumerate() {
                                let ly = y_start + li as f64 * line_h;

                                // Outline first (under fill, like ASS)
                                if st.outline_width > 0 {
                                    let olw = st.outline_width as f64 * ph as f64 / res_y;
                                    ctx.set_stroke_style_str(&st.outline_color);
                                    ctx.set_line_width(olw);
                                    ctx.set_line_join("round");
                                    ctx.stroke_text(line, x, ly).ok();
                                }

                                // Fill on top
                                ctx.set_fill_style_str(&st.color);
                                ctx.fill_text(line, x, ly).ok();
                            }

                            // Selection highlight for dragged segment
                            if dragged_idx == Some(seg_i) {
                                let max_line_w = lines.iter()
                                    .map(|l| ctx.measure_text(l).map(|m| m.width()).unwrap_or(0.0))
                                    .fold(0.0_f64, f64::max);
                                let pad = 4.0;
                                let box_x = x - max_line_w / 2.0 - pad;
                                let box_y = y_start - font_px + pad;
                                let box_w = max_line_w + pad * 2.0;
                                let box_h = total_h + pad;
                                ctx.set_stroke_style_str("rgba(77,171,247,0.8)");
                                ctx.set_line_width(2.0);
                                ctx.set_line_join("round");
                                ctx.stroke_rect(box_x, box_y, box_w, box_h);
                            }
                        }
                    });

                    view! {
                        <canvas node_ref=subs_canvas_ref
                            class="subtrack-subs-canvas"
                            class:hidden=move || !canvas_visible.get()
                        />
                    }
                }
            </div>

            // Timeline with zoom/pan
            <div class="subtrack-timeline" node_ref=tl_ref
                on:wheel=move |ev: WheelEvent| {
                    ev.prevent_default();
                    ev.stop_propagation();
                    if let Some(el) = tl_ref.get_untracked() {
                        let rect = el.get_bounding_client_rect();
                        let css_w = el.client_width() as f64;
                        let scale = if css_w > 0.0 { rect.width() / css_w } else { 1.0 };
                        let mx = (ev.client_x() as f64 - rect.left()) / scale;
                        if ev.ctrl_key() {
                            // Pinch zoom
                            let delta = -ev.delta_y() * 0.01;
                            let old_zoom = tl_zoom.get_untracked();
                            let new_zoom = (old_zoom * (1.0 + delta)).clamp(1.0, 50.0);
                            let ratio = new_zoom / old_zoom;
                            let old_off = tl_offset.get_untracked();
                            tl_offset.set(mx - (mx - old_off) * ratio);
                            tl_zoom.set(new_zoom);
                        } else {
                            // Pan
                            tl_offset.update(|o| *o -= ev.delta_x() + ev.delta_y());
                        }
                    }
                }
                on:click=move |ev: MouseEvent| {
                    // Set cursor position on click
                    if let Some(el) = tl_ref.get_untracked() {
                        let rect = el.get_bounding_client_rect();
                        let css_w = el.client_width() as f64;
                        let scale = if css_w > 0.0 { rect.width() / css_w } else { 1.0 };
                        let mx = (ev.client_x() as f64 - rect.left()) / scale;
                        cursor_px.set(mx);
                        let zoom = tl_zoom.get_untracked();
                        let off = tl_offset.get_untracked();
                        let pct = ((mx - off) / (css_w * zoom)).clamp(0.0, 1.0);
                        let ls = local_segments.get_untracked();
                        let t_min = ls.iter().map(|s| s.start_ms).fold(f64::MAX, f64::min);
                        let t_max = ls.iter().map(|s| s.end_ms).fold(0.0_f64, f64::max);
                        let total = if t_max > t_min { t_max - t_min } else { 1.0 };
                        cursor_ms.set(t_min + pct * total);
                    }
                }
                on:mousemove=move |ev: MouseEvent| {
                    // Only update cursor_px for drag operations
                    if let Some(el) = tl_ref.get_untracked() {
                        if dragging.get_untracked().is_none() { return; }
                        let rect = el.get_bounding_client_rect();
                        let css_w = el.client_width() as f64;
                        let scale = if css_w > 0.0 { rect.width() / css_w } else { 1.0 };
                        let mx = (ev.client_x() as f64 - rect.left()) / scale;
                        cursor_px.set(mx);
                        let zoom = tl_zoom.get_untracked();
                        let off = tl_offset.get_untracked();
                        let pct = ((mx - off) / (css_w * zoom)).clamp(0.0, 1.0);
                        let ls = local_segments.get_untracked();
                        let t_min = ls.iter().map(|s| s.start_ms).fold(f64::MAX, f64::min);
                        let t_max = ls.iter().map(|s| s.end_ms).fold(0.0_f64, f64::max);
                        let total = if t_max > t_min { t_max - t_min } else { 1.0 };
                        let ms = t_min + pct * total;
                        cursor_ms.set(ms);

                        // Handle boundary drag
                        if let Some((seg_idx, is_start, merge_next)) = dragging.get_untracked() {
                            local_segments.update(|segs| {
                                if let Some(merge_idx) = merge_next {
                                    // Merge boundary: move end of left + start of right
                                    if let Some(s) = segs.get_mut(seg_idx) { s.end_ms = ms; }
                                    if let Some(s) = segs.get_mut(merge_idx) { s.start_ms = ms; }
                                } else if is_start {
                                    if let Some(s) = segs.get_mut(seg_idx) { s.start_ms = ms; }
                                } else {
                                    if let Some(s) = segs.get_mut(seg_idx) { s.end_ms = ms; }
                                }
                            });
                        }
                    }
                }
                on:mouseup=move |_| {
                    if dragging.get_untracked().is_some() {
                        dragging.set(None);
                        // Save to server
                        let segs = local_segments.get_untracked();
                        let ns = node_signal.get_untracked();
                        let (st, rx, ry, fp) = match &ns.settings {
                            Some(api_types::NodeSettings::SubtitleTrack { styles, resolution_x, resolution_y, fps, .. }) =>
                                (styles.clone(), *resolution_x, *resolution_y, *fps),
                            _ => (vec![], 1920, 1080, 30),
                        };
                        let settings = api_types::NodeSettings::SubtitleTrack {
                            styles: st, segments: segs, resolution_x: rx, resolution_y: ry, fps: fp,
                        };
                        spawn_local(async move {
                            let _ = project_service::update_node_settings(project_id, id_for_drag, settings).await;
                        });
                    }
                }
                on:mouseleave=move |_| {
                    if dragging.get_untracked().is_some() {
                        dragging.set(None);
                    }
                }
            >
                // Cursor line — follows mouse directly
                <div class="subtrack-cursor" style=move || format!("left:{:.1}px", cursor_px.get())></div>

                // Inner container with zoom/pan transform
                <div class="subtrack-tl-inner" style=move || {
                    let z = tl_zoom.get();
                    let o = tl_offset.get();
                    format!("width:{:.1}%;left:{:.1}px;", z * 100.0, o)
                }>
                    // Segments with multi-track support — read directly from local_segments
                    {move || {
                        let ls = local_segments.get();
                        if ls.is_empty() {
                            // Fall back to ASS-parsed segments for initial display
                            let segs = segments.get();
                            if segs.is_empty() { return view! { <span class="subtrack-empty">"Запустите ноду"</span> }.into_view(); }
                        }

                        let seg_source: Vec<(usize, String, f64, f64, u32)> = if !ls.is_empty() {
                            ls.iter().enumerate()
                                .filter(|(_, s)| !s.text.is_empty())
                                .map(|(i, s)| (i, s.text.clone(), s.start_ms, s.end_ms, s.track))
                                .collect()
                        } else {
                            segments.get().into_iter().enumerate()
                                .map(|(i, (text, start, end))| (i, text, start, end, 0_u32))
                                .collect()
                        };
                        if seg_source.is_empty() { return view! { <span class="subtrack-empty">"Запустите ноду"</span> }.into_view(); }

                        let max_track = seg_source.iter().map(|s| s.4).max().unwrap_or(0);
                        let track_count = max_track + 1;
                        let track_h = 100.0 / track_count as f64;

                        let mut seg_data = seg_source;
                        seg_data.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap());

                        let t_min = seg_data.iter().map(|s| s.2).fold(f64::MAX, f64::min);
                        let t_max = seg_data.iter().map(|s| s.3).fold(0.0_f64, f64::max);
                        let total = if t_max > t_min { t_max - t_min } else { 1.0 };

                        let seg_stored = store_value(seg_data.clone());

                        let fps = match &node_signal.get_untracked().settings {
                            Some(api_types::NodeSettings::SubtitleTrack { fps, .. }) => *fps,
                            _ => 30,
                        };
                        let frame_ms = 1000.0 / fps.max(1) as f64;

                        let mut views = Vec::new();

                        for (si, &(i, ref text, start, end, track)) in seg_data.iter().enumerate() {
                            let left_pct = ((start - t_min) / total * 100.0).max(0.0);
                            let width_pct = ((end - start) / total * 100.0).max(0.3);
                            let top_pct = track as f64 * track_h;
                            let lh = 60.0 * track_h / 100.0;
                            let text_c = text.clone();

                            // Segment block
                            views.push(view! {
                                <div class="subtrack-tl-seg"
                                    style=format!("left:{:.2}%;width:{:.2}%;top:{:.1}%;height:{:.1}%;line-height:{:.0}px;",
                                        left_pct, width_pct, top_pct, track_h, lh)
                                    on:click=move |ev: MouseEvent| {
                                        ev.stop_propagation();
                                        // Center cursor on this segment
                                        let ls = local_segments.get_untracked();
                                        if let Some(seg) = ls.get(i) {
                                            let mid = (seg.start_ms + seg.end_ms) / 2.0;
                                            cursor_ms.set(mid);
                                            // Update cursor_px
                                            if let Some(el) = tl_ref.get_untracked() {
                                                let css_w = el.client_width() as f64;
                                                let t_min = ls.iter().map(|s| s.start_ms).fold(f64::MAX, f64::min);
                                                let t_max = ls.iter().map(|s| s.end_ms).fold(0.0_f64, f64::max);
                                                let total = if t_max > t_min { t_max - t_min } else { 1.0 };
                                                let pct = (mid - t_min) / total;
                                                let zoom = tl_zoom.get_untracked();
                                                let off = tl_offset.get_untracked();
                                                cursor_px.set(off + pct * css_w * zoom);
                                            }
                                        }
                                    }
                                    on:contextmenu=move |ev: MouseEvent| {
                                        ev.prevent_default();
                                        ev.stop_propagation();
                                        st_ctx_menu.set(Some((i, ev.client_x() as f64, ev.client_y() as f64, node_signal, local_segments, false, 0)));
                                    }
                                >
                                    {text_c}
                                </div>
                            }.into_view());

                            let prev_on_track = seg_data.iter().take(si).rev()
                                .find(|(_, _, _, _, t)| *t == track);
                            let next_on_track: Option<usize> = seg_data.iter().skip(si + 1)
                                .find(|(_, _, _, _, t)| *t == track).map(|s| s.0);

                            // Left boundary — skip only if previous segment on same track ends exactly here
                            // Adjacent = previous segment ends within 50ms of this one starting
                            let prev_adjacent = prev_on_track
                                .map(|p| (start - p.3).abs() < frame_ms)
                                .unwrap_or(false);
                            if !prev_adjacent {
                                let bp = left_pct;
                                let drag_i = i;
                                views.push(view! {
                                    <div class="subtrack-boundary"
                                        style=format!("left:{:.2}%;top:{:.1}%;height:{:.1}%;", bp, top_pct, track_h)
                                        on:mousedown=move |ev: MouseEvent| {
                                            ev.stop_propagation(); ev.prevent_default();
                                            dragging.set(Some((drag_i, true, None)));
                                        }
                                        on:contextmenu=|ev: MouseEvent| { ev.prevent_default(); ev.stop_propagation(); }
                                    ></div>
                                }.into_view());
                            }

                            // Right boundary — always present
                            // Merge popup only if next segment is adjacent (no gap)
                            {
                                let bp = left_pct + width_pct;
                                let cur_i = i;
                                // Adjacent = next segment starts within 50ms of this one ending
                                let next_adjacent = next_on_track.map(|ni| {
                                    seg_data.iter().find(|s| s.0 == ni)
                                        .map(|s| (s.2 - end).abs() < frame_ms)
                                        .unwrap_or(false)
                                }).unwrap_or(false);
                                // Get text/time data for merge from seg_data
                                let cur_text = text.clone();
                                let cur_start = start;
                                let cur_end = end;
                                let next_data = next_on_track.and_then(|ni| {
                                    seg_data.iter().find(|s| s.0 == ni).map(|s| (s.0, s.1.clone(), s.2, s.3))
                                });
                                let nd = store_value(next_data);

                                views.push(view! {
                                    <div class="subtrack-boundary"
                                        style=format!("left:{:.2}%;top:{:.1}%;height:{:.1}%;", bp, top_pct, track_h)
                                        on:contextmenu=move |ev: MouseEvent| {
                                            ev.prevent_default();
                                            ev.stop_propagation();
                                            if !next_adjacent { return; }
                                            // Open merge popup at page level
                                            let ni = next_on_track.unwrap_or(0);
                                            st_ctx_menu.set(Some((cur_i, ev.client_x() as f64, ev.client_y() as f64, node_signal, local_segments, true, ni)));
                                        }
                                        on:mousedown=move |ev: MouseEvent| {
                                            ev.stop_propagation(); ev.prevent_default();
                                            if next_adjacent {
                                                // Merge boundary: drag moves both end+start
                                                dragging.set(Some((cur_i, false, next_on_track)));
                                            } else {
                                                // End boundary: drag moves end only
                                                dragging.set(Some((cur_i, false, None)));
                                            }
                                        }
                                    ></div>
                                }.into_view());
                            }

                        }

                        views.collect_view()
                    }}
                </div>
            </div>
        </div>

    }.into_view()
}
