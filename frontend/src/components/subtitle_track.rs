use api_types::Node;
use leptos::*;
use web_sys::MouseEvent;

#[component]
pub fn SubtitleStyleModal(
    node_signal: RwSignal<Node>,
    on_save: impl Fn(api_types::NodeSettings) + Copy + 'static,
    on_close: impl Fn() + Copy + 'static,
) -> impl IntoView {
    let styles = Signal::derive(move || {
        match &node_signal.get().settings {
            Some(api_types::NodeSettings::SubtitleTrack { styles, .. }) => styles.clone(),
            _ => vec![api_types::SubtitleStyle::default()],
        }
    });
    let editing = create_rw_signal(0_usize);

    let commit = move |updated: api_types::SubtitleStyle| {
        let ns = node_signal.get_untracked();
        let (mut st, ovr, rx, ry) = match &ns.settings {
            Some(api_types::NodeSettings::SubtitleTrack { styles, segments, resolution_x, resolution_y, .. }) =>
                (styles.clone(), segments.clone(), *resolution_x, *resolution_y),
            _ => (vec![], Vec::new(), 1920, 1080),
        };
        let idx = editing.get_untracked();
        if idx < st.len() { st[idx] = updated; } else { st.push(updated); }
        on_save(api_types::NodeSettings::SubtitleTrack { styles: st, segments: ovr, resolution_x: rx, resolution_y: ry, fps: 30 });
    };

    let add_style = move |_: MouseEvent| {
        let ns = node_signal.get_untracked();
        let (mut st, ovr, rx, ry) = match &ns.settings {
            Some(api_types::NodeSettings::SubtitleTrack { styles, segments, resolution_x, resolution_y, .. }) =>
                (styles.clone(), segments.clone(), *resolution_x, *resolution_y),
            _ => (vec![], Vec::new(), 1920, 1080),
        };
        let name = format!("Style{}", st.len());
        st.push(api_types::SubtitleStyle { name, ..api_types::SubtitleStyle::default() });
        editing.set(st.len() - 1);
        on_save(api_types::NodeSettings::SubtitleTrack { styles: st, segments: ovr, resolution_x: rx, resolution_y: ry, fps: 30 });
    };

    view! {
        <div class="modal-backdrop" on:click=move |_| on_close()>
            <div class="modal" style="width:340px;" on:click=|ev: MouseEvent| ev.stop_propagation()>
                <div class="modal-header">
                    <h3>"Стили субтитров"</h3>
                    <button class="modal-close" on:click=move |_| on_close()>"✕"</button>
                </div>
                <div class="modal-body" style="display:flex;gap:8px;">
                    <div style="min-width:80px;border-right:1px solid var(--border);padding-right:8px;">
                        {move || styles.get().into_iter().enumerate().map(|(i, s)| {
                            view! {
                                <div class="subtrack-style-item"
                                    class:active=move || editing.get() == i
                                    on:click=move |_| editing.set(i)
                                >{s.name}</div>
                            }
                        }).collect_view()}
                        <button class="subtrack-btn" style="margin-top:4px;width:100%;" on:click=add_style>"+"</button>
                    </div>
                    <div style="flex:1;">
                        {move || {
                            let st = styles.get();
                            let idx = editing.get();
                            let s = st.get(idx).cloned().unwrap_or_default();
                            let size = create_rw_signal(s.size);
                            let color = create_rw_signal(s.color.clone());
                            let outline_color = create_rw_signal(s.outline_color.clone());
                            let outline_w = create_rw_signal(s.outline_width);
                            let bold = create_rw_signal(s.bold);
                            let name = store_value(s.name.clone());
                            let do_commit = move || {
                                commit(api_types::SubtitleStyle {
                                    name: name.get_value(),
                                    size: size.get_untracked(),
                                    color: color.get_untracked(),
                                    outline_color: outline_color.get_untracked(),
                                    outline_width: outline_w.get_untracked(),
                                    bold: bold.get_untracked(),
                                    ..api_types::SubtitleStyle::default()
                                });
                            };
                            view! {
                                <div class="subtrack-style-form">
                                    <label class="overlay-slider-row">
                                        <span>"size"</span>
                                        <input type="range" min="12" max="120" step="1"
                                            prop:value=move || format!("{}", size.get())
                                            on:input=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<u32>() { size.set(v); } }
                                            on:change=move |_| do_commit()
                                        />
                                        <span class="overlay-slider-val">{move || format!("{}", size.get())}</span>
                                    </label>
                                    <div class="overlay-xy-row">
                                        <label class="overlay-xy-field">
                                            <span>"цвет"</span>
                                            <input type="color" style="width:30px;height:18px;padding:0;border:none;"
                                                prop:value=move || color.get()
                                                on:input=move |ev| { color.set(event_target_value(&ev)); do_commit(); }
                                            />
                                        </label>
                                        <label class="overlay-xy-field">
                                            <span>"обвод"</span>
                                            <input type="color" style="width:30px;height:18px;padding:0;border:none;"
                                                prop:value=move || outline_color.get()
                                                on:input=move |ev| { outline_color.set(event_target_value(&ev)); do_commit(); }
                                            />
                                        </label>
                                    </div>
                                    <div class="overlay-xy-row">
                                        <label class="overlay-xy-field">
                                            <span>"обвод px"</span>
                                            <input type="text" prop:value=move || format!("{}", outline_w.get())
                                                on:change=move |ev| {
                                                    if let Ok(v) = event_target_value(&ev).parse::<u32>() { outline_w.set(v); do_commit(); }
                                                }
                                            />
                                        </label>
                                        <label class="overlay-xy-field">
                                            <input type="checkbox" prop:checked=move || bold.get()
                                                on:change=move |_| { bold.update(|v| *v = !*v); do_commit(); }
                                            />
                                            <span>"Bold"</span>
                                        </label>
                                    </div>
                                </div>
                            }.into_view()
                        }}
                    </div>
                </div>
            </div>
        </div>
    }
}

#[component]
pub fn SubtitleSegmentModal(
    index: usize,
    text: String,
    start_ms: f64,
    end_ms: f64,
    node_signal: RwSignal<Node>,
    on_save: impl Fn(api_types::NodeSettings) + Copy + 'static,
    on_close: impl Fn() + Copy + 'static,
) -> impl IntoView {
    let text_sig = create_rw_signal(text);
    let start_sig = create_rw_signal(format!("{:.0}", start_ms));
    let end_sig = create_rw_signal(format!("{:.0}", end_ms));
    let pos_x = create_rw_signal(0.5_f64);
    let pos_y = create_rw_signal(0.9_f64);
    let style_name = create_rw_signal("Default".to_string());

    // Load from segment data
    {
        let ns = node_signal.get_untracked();
        if let Some(api_types::NodeSettings::SubtitleTrack { ref segments, .. }) = ns.settings {
            if let Some(seg) = segments.get(index) {
                if let Some(ref sn) = seg.style_name { style_name.set(sn.clone()); }
            }
        }
    }

    let styles = Signal::derive(move || {
        match &node_signal.get().settings {
            Some(api_types::NodeSettings::SubtitleTrack { styles, .. }) => styles.clone(),
            _ => vec![api_types::SubtitleStyle::default()],
        }
    });

    let save = move |_: MouseEvent| {
        let ns = node_signal.get_untracked();
        let (st, mut segs, rx, ry, fp) = match &ns.settings {
            Some(api_types::NodeSettings::SubtitleTrack { styles, segments, resolution_x, resolution_y, fps }) =>
                (styles.clone(), segments.clone(), *resolution_x, *resolution_y, *fps),
            _ => (vec![], Vec::new(), 1920, 1080, 30),
        };
        if let Some(seg) = segs.get_mut(index) {
            seg.text = text_sig.get_untracked();
            seg.start_ms = start_sig.get_untracked().parse::<f64>().unwrap_or(seg.start_ms);
            seg.end_ms = end_sig.get_untracked().parse::<f64>().unwrap_or(seg.end_ms);
            seg.style_name = Some(style_name.get_untracked());
        }
        on_save(api_types::NodeSettings::SubtitleTrack { styles: st, segments: segs, resolution_x: rx, resolution_y: ry, fps: fp });
        on_close();
    };

    view! {
        <div class="modal-backdrop" on:click=move |_| on_close()>
            <div class="modal" style="width:320px;" on:click=|ev: MouseEvent| ev.stop_propagation()>
                <div class="modal-header">
                    <h3>{format!("Сегмент #{}", index + 1)}</h3>
                    <button class="modal-close" on:click=move |_| on_close()>"✕"</button>
                </div>
                <div class="modal-body">
                    <textarea class="spell-textarea" style="min-height:60px;"
                        prop:value=move || text_sig.get()
                        on:input=move |ev| text_sig.set(event_target_value(&ev))
                    />
                    <div class="overlay-xy-row" style="margin:6px 0;">
                        <label class="overlay-xy-field">
                            <span>"start"</span>
                            <input type="text" prop:value=move || start_sig.get()
                                on:input=move |ev| start_sig.set(event_target_value(&ev))
                            />
                        </label>
                        <label class="overlay-xy-field">
                            <span>"end"</span>
                            <input type="text" prop:value=move || end_sig.get()
                                on:input=move |ev| end_sig.set(event_target_value(&ev))
                            />
                        </label>
                    </div>
                    <div class="overlay-xy-row" style="margin:6px 0;">
                        <label class="overlay-xy-field">
                            <span>"стиль"</span>
                            <select class="overlay-interp-select"
                                on:change=move |ev| style_name.set(event_target_value(&ev))
                            >
                                {move || styles.get().into_iter().map(|s| {
                                    let n = s.name.clone();
                                    let selected = n == style_name.get();
                                    view! { <option value=n.clone() selected=selected>{n}</option> }
                                }).collect_view()}
                            </select>
                        </label>
                    </div>
                    <div class="overlay-xy-row" style="margin:6px 0;">
                        <label class="overlay-xy-field">
                            <span>"x"</span>
                            <input type="text" prop:value=move || format!("{:.2}", pos_x.get())
                                on:change=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<f64>() { pos_x.set(v); } }
                            />
                        </label>
                        <label class="overlay-xy-field">
                            <span>"y"</span>
                            <input type="text" prop:value=move || format!("{:.2}", pos_y.get())
                                on:change=move |ev| { if let Ok(v) = event_target_value(&ev).parse::<f64>() { pos_y.set(v); } }
                            />
                        </label>
                    </div>
                    <button class="run-btn" style="width:100%;margin-top:8px;" on:click=save>"Сохранить"</button>
                </div>
            </div>
        </div>
    }
}
