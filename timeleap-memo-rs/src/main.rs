#![windows_subsystem = "windows"]
slint::include_modules!();

mod logic;
mod storage;

use logic::{Stroke, VirtualTime, WorldLineManager, ImpressionTag};
use slint::{Color, SharedString, VecModel};
use storage::AppSettings;
use std::cell::RefCell;
use std::rc::Rc;

fn stroke_to_svg(stroke: &Stroke, width: f32, height: f32) -> SharedString {
    if stroke.points.is_empty() {
        return SharedString::from("");
    }
    let mut path = format!(
        "M {} {}",
        stroke.points[0].x * width,
        stroke.points[0].y * height
    );
    for p in &stroke.points[1..] {
        path.push_str(&format!(" L {} {}", p.x * width, p.y * height));
    }
    SharedString::from(path)
}

fn update_ui_segments(
    ui: &MainWindow,
    wlm: &WorldLineManager,
    strokes: &[Stroke],
    vt_max: f32,
    active_id: usize,
) {
    let mut ui_segments: Vec<SegmentData> = Vec::new();
    let samples = 100;
    let lambda = 0.13;

    // キャンバスの幅と「高さ」を取得
    let canvas_w = ui.get_canvas_width();
    let canvas_h = ui.get_canvas_height();
    
    // セグメント数から「1バンドあたりの高さ」を計算 (0除算防止のため max(1) を使う)
    let seg_count = wlm.segments.len().max(1) as f32;
    let seg_h = canvas_h / seg_count;

    for segment in &wlm.segments {
        let mut waveform = String::new();
        if vt_max > 0.0 && !segment.stroke_indices.is_empty() {
            let mut densities = Vec::with_capacity(samples);
            let mut max_d = 0.1f32;

            for i in 0..samples {
                let t = (i as f32 / (samples - 1) as f32) * vt_max;
                let mut d = 0.0;
                for &idx in &segment.stroke_indices {
                    let s = &strokes[idx];
                    if s.virtual_time_created <= t {
                        // 消去された時間より前であれば密度に加算する
                        let is_erased_at_t = s.erased_at.map_or(false, |ea| t >= ea);
                        if !is_erased_at_t {
                            let age = t - s.virtual_time_created;
                            let alpha = (-lambda * age).exp();
                            if alpha > 0.01 {
                                d += (s.points.len() as f32) * alpha;
                            }
                        }
                    }
                }
                densities.push(d);
                if d > max_d { max_d = d; }
            }
            if max_d < 1.0 { max_d = 1.0; }

            // Y軸の中心と、波が広がる最大幅を計算
            let center_y = seg_h / 2.0;
            let amplitude = center_y * 0.9; // 0.9を掛けて上下に少しだけ余白を作る

            waveform.push_str(&format!("M 0 {:.1}", center_y));
            for (i, &d) in densities.iter().enumerate() {
                let x = (i as f32 / (samples - 1) as f32) * canvas_w;
                let y = center_y - (d / max_d) * amplitude;
                waveform.push_str(&format!(" L {:.1} {:.1}", x, y));
            }
            for (i, &d) in densities.iter().enumerate().rev() {
                let x = (i as f32 / (samples - 1) as f32) * canvas_w;
                let y = center_y + (d / max_d) * amplitude;
                waveform.push_str(&format!(" L {:.1} {:.1}", x, y));
            }
            waveform.push_str(" Z");
        }

        ui_segments.push(SegmentData {
            id: segment.id as i32,
            y: 0.0,
            highlighted: segment.id == active_id,
            waveform_data: slint::SharedString::from(waveform),
        });
    }
    let segments_model = std::rc::Rc::new(slint::VecModel::from(ui_segments));
    ui.set_timeline_segments(segments_model.into());
}

fn update_ui_tags(ui: &MainWindow, tags: &[ImpressionTag]) {
    let ui_tags: Vec<TagData> = tags.iter().map(|t| TagData {
        vt: t.virtual_time,
        label: slint::SharedString::from(&t.label),
    }).collect();
    let tags_model = std::rc::Rc::new(slint::VecModel::from(ui_tags));
    State::get(ui).set_tags(tags_model.into());
}
fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;
    ui.window().set_maximized(true);

    // Load Settings
    let settings = storage::load_settings("settings.json").unwrap_or_default();
    
    // UI handle for state access
    let state = State::get(&ui);
    state.set_macro_ratio(settings.macro_ratio);
    state.set_middle_ratio(settings.middle_ratio);

    // Apply window maximization explicitly
    if settings.is_maximized {
        ui.window().set_maximized(true);
    }

    // Application State
    let (initial_vt, initial_strokes, initial_tags) =
        storage::load_from_binary("data.bin").unwrap_or((0.0, Vec::new(), Vec::new()));
    let strokes = Rc::new(RefCell::<Vec<Stroke>>::new(initial_strokes));
    let tags = Rc::new(RefCell::<Vec<ImpressionTag>>::new(initial_tags));
    let current_stroke = Rc::new(RefCell::new(None::<Stroke>));
    let virtual_time = Rc::new(RefCell::new(VirtualTime::new()));
    let is_pointer_down = Rc::new(RefCell::new(false));
    let world_line_manager = Rc::new(RefCell::new(WorldLineManager::new()));
    let highlighted_strokes = Rc::new(RefCell::new(Vec::<usize>::new()));
    let active_segment_id = Rc::new(RefCell::new(0usize));

    // Restore virtual time state
    {
        let mut vt = virtual_time.borrow_mut();
        vt.update_max(initial_vt);
        for s in strokes.borrow().iter() {
            vt.update_max(s.virtual_time_created);
        }
        for t in tags.borrow().iter() {
            vt.update_max(t.virtual_time);
        }
        // Restore last VT from settings
        vt.set_current(settings.last_vt);
    }

    // Calculate segments from loaded strokes
    {
        let mut wlm = world_line_manager.borrow_mut();
        wlm.calculate_segments(&strokes.borrow());
    }

    // Initial UI update for tags
    update_ui_tags(&ui, &tags.borrow());

    let ui_handle = ui.as_weak();

    // Handlers
    ui.on_pointer_down({
        let virtual_time = virtual_time.clone();
        let current_stroke = current_stroke.clone();
        let strokes = strokes.clone();
        let is_pointer_down = is_pointer_down.clone();
        let ui_handle = ui.as_weak();
        move |pos| {
            *is_pointer_down.borrow_mut() = true;
            if let Some(ui) = ui_handle.upgrade() {
                let state = State::get(&ui);

                let current_vt = virtual_time.borrow().get_current();

                if !state.get_scrubbing() {
                    virtual_time.borrow_mut().set_playing(true);
                }
                if state.get_eraser() {
                    let mut s_list = strokes.borrow_mut();
                    let threshold = 0.05;
                    for stroke in s_list.iter_mut() {
                        let is_erased_now = stroke.erased_at.map_or(false, |ea| current_vt >= ea);
                        if !is_erased_now {
                            for p in &stroke.points {
                                let dx = p.x - pos.x;
                                let dy = p.y - pos.y;
                                if (dx * dx + dy * dy).sqrt() < threshold {
                                    stroke.erased_at = Some(current_vt);
                                    break;
                                }
                            }
                        }
                    }
                } else {
                    let vt = virtual_time.borrow().get_current();
                    let mut stroke = Stroke::new(vt);
                    stroke.add_point(pos.x, pos.y, 1.0);
                    *current_stroke.borrow_mut() = Some(stroke);
                }
            }
        }
    });

    ui.on_pointer_move({
        let virtual_time = virtual_time.clone();
        let current_stroke = current_stroke.clone();
        let strokes = strokes.clone();
        let is_pointer_down = is_pointer_down.clone();
        let ui_handle = ui.as_weak();
        move |pos| {
            if !*is_pointer_down.borrow() {
                return;
            }
            if let Some(ui) = ui_handle.upgrade() {
                let state = State::get(&ui);
                if state.get_eraser() {
                    let mut s_list = strokes.borrow_mut();
                    let threshold = 0.05;
                    let current_vt = virtual_time.borrow().get_current();
                    for stroke in s_list.iter_mut() {
                        let is_already_erased = stroke.erased_at.map_or(false, |ea| current_vt >= ea);
                        if !is_already_erased {
                            for p in &stroke.points {
                                let dx = p.x - pos.x;
                                let dy = p.y - pos.y;
                                if (dx * dx + dy * dy).sqrt() < threshold {
                                    stroke.erased_at = Some(current_vt);
                                    break;
                                }
                            }
                        }
                    }
                } else {
                    if let Some(ref mut stroke) = *current_stroke.borrow_mut() {
                        stroke.add_point(pos.x, pos.y, 1.0);
                    }
                }
            }
        }
    });

    ui.on_pointer_up({
        let strokes = strokes.clone();
        let current_stroke = current_stroke.clone();
        let virtual_time = virtual_time.clone();
        let is_pointer_down = is_pointer_down.clone();
        let world_line_manager = world_line_manager.clone();
        let tags = tags.clone();
        move || {
            *is_pointer_down.borrow_mut() = false;
            if let Some(stroke) = current_stroke.borrow_mut().take() {
                let vt_created = stroke.virtual_time_created;
                let mut vt = virtual_time.borrow_mut();
                vt.update_max(vt_created);
                let current_vt = vt.get_current();
                strokes.borrow_mut().push(stroke);

                {
                    let mut wlm = world_line_manager.borrow_mut();
                    wlm.calculate_segments(&strokes.borrow());
                }

                let data = strokes.borrow().clone();
                let tag_data = tags.borrow().clone();
                let _ = storage::save_to_binary("data.bin", &data, &tag_data, current_vt);
            }
            virtual_time.borrow_mut().set_playing(false);
        }
    });

    ui.on_scrub({
        let virtual_time = virtual_time.clone();
        move |ratio| {
            let mut vt = virtual_time.borrow_mut();
            let max_vt = vt.get_max();
            vt.set_current(ratio * max_vt);
            vt.set_playing(false);
        }
    });

    ui.on_toggleplay({
        let virtual_time = virtual_time.clone();
        move || {
            virtual_time.borrow_mut().toggle_play();
        }
    });

    ui.on_gonow({
        let virtual_time = virtual_time.clone();
        move || {
            let mut vt = virtual_time.borrow_mut();
            let max_vt = vt.get_max();
            vt.set_current(max_vt);
        }
    });

    ui.on_enter_chaos_pad({
        let ui_handle = ui.as_weak();
        let world_line_manager = world_line_manager.clone();
        let strokes = strokes.clone();
        let virtual_time = virtual_time.clone();
        let active_segment_id = active_segment_id.clone();
        
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                let wlm = world_line_manager.borrow();
                let s_list = strokes.borrow();
                let vt_max = virtual_time.borrow().get_max();
                let active_id = *active_segment_id.borrow();
                update_ui_segments(&ui, &wlm, &s_list, vt_max, active_id);
            }
        }
    });

    ui.on_add_tag({
        let ui_handle = ui.as_weak();
        let tags = tags.clone();
        let virtual_time = virtual_time.clone();
        let strokes = strokes.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                let vt = virtual_time.borrow().get_current();
                let mut tag_list = tags.borrow_mut();
                let label = format!("Tag {}", tag_list.len() + 1);
                tag_list.push(ImpressionTag::new(vt, label));
                update_ui_tags(&ui, &tag_list);

                // Auto-save when tag is added
                let data = strokes.borrow().clone();
                let _ = storage::save_to_binary("data.bin", &data, &tag_list, vt);
            }
        }
    });

    ui.on_worldline_changed({
        let ui_handle = ui.as_weak();
        let world_line_manager = world_line_manager.clone();
        let strokes = strokes.clone();
        let virtual_time = virtual_time.clone();
        let active_segment_id = active_segment_id.clone();
        let highlighted_strokes = highlighted_strokes.clone();

        move |ratio_y| {
            let wlm = world_line_manager.borrow();
            let total_segments = wlm.segment_count();
            if total_segments == 0 { return; }

            let estimated_index = (ratio_y * total_segments as f32) - 0.5;
            let mut target_index = estimated_index.round() as usize;
            target_index = target_index.clamp(0, total_segments.saturating_sub(1));

            if let Some(segment) = wlm.segments.get(target_index) {
                let new_id = segment.id;
                
                if *active_segment_id.borrow() != new_id {
                    *active_segment_id.borrow_mut() = new_id;

                    if let Some(ui) = ui_handle.upgrade() {
                        let s_list = strokes.borrow();
                        let vt_max = virtual_time.borrow().get_max();
                        update_ui_segments(&ui, &wlm, &s_list, vt_max, new_id);
                        
                        let mut highlighted = highlighted_strokes.borrow_mut();
                        highlighted.clear();
                        highlighted.extend(segment.stroke_indices.clone());
                        
                        let state = State::get(&ui);
                        let vec_model: Vec<i32> = highlighted.iter().map(|&idx| idx as i32).collect();
                        let model_rc = Rc::new(VecModel::from(vec_model));
                        state.set_highlighted_strokes(model_rc.into());
                    }
                }
            }
        }
    });

    ui.on_exit_chaos_pad({
        let highlighted_strokes = highlighted_strokes.clone();
        let ui_handle = ui.as_weak();
        move || {
            highlighted_strokes.borrow_mut().clear();
            if let Some(ui) = ui_handle.upgrade() {
                let state = State::get(&ui);
                let empty_model = Rc::new(VecModel::from(Vec::<i32>::new()));
                state.set_highlighted_strokes(empty_model.into());
            }
        }
    });

    ui.on_time_changed({
        let virtual_time = virtual_time.clone();
        move |new_time| {
            virtual_time.borrow_mut().set_current(new_time);
        }
    });

    ui.on_segment_selected({
        let highlighted_strokes = highlighted_strokes.clone();
        let world_line_manager = world_line_manager.clone();
        let ui_handle = ui.as_weak();
        move |segment_id| {
            let mut highlighted = highlighted_strokes.borrow_mut();
            highlighted.clear();
            
            let wlm = world_line_manager.borrow();
            let segment_strokes = wlm.get_segment_strokes(segment_id as usize);
            highlighted.extend(segment_strokes);

            if let Some(ui) = ui_handle.upgrade() {
                let state = State::get(&ui);
                let vec_model: Vec<i32> = highlighted.iter().map(|&idx| idx as i32).collect();
                let model_rc = Rc::new(VecModel::from(vec_model));
                state.set_highlighted_strokes(model_rc.into());
            }
        }
    });

    // Update Timer (approx 60fps)
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(16),
        {
            let ui_handle = ui_handle.clone();
            let strokes = strokes.clone();
            let current_stroke = current_stroke.clone();
            let virtual_time = virtual_time.clone();
            let highlighted_strokes = highlighted_strokes.clone();
            let world_line_manager = world_line_manager.clone();
            let active_segment_id = active_segment_id.clone();
            let mut waveform_update_counter = 0;

            move || {
                if let Some(ui) = ui_handle.upgrade() {
                    let mut vt = virtual_time.borrow_mut();
                    vt.advance(0.016);
                    let current_vt = vt.get_current();

                    let state = State::get(&ui);
                    state.set_vt(current_vt);
                    state.set_vt_max(vt.get_max());
                    state.set_playing(vt.is_playing());
                    let canvas_w = ui.get_canvas_width();
                    let canvas_h = ui.get_canvas_height();

                    let mut render_data = Vec::new();
                    let lambda = 0.13;
                    let highlighted = highlighted_strokes.borrow();
                    let is_chaos_mode = state.get_chaos_pad_mode();

                    for (idx, stroke) in strokes.borrow().iter().enumerate() {
                        // 消去された時間より後であれば表示しない
                        if let Some(ea) = stroke.erased_at {
                            if current_vt >= ea { continue; }
                        }
                        
                        let dt = current_vt - stroke.virtual_time_created;
                        let mut r = stroke.color[0];
                        let mut g = stroke.color[1];
                        let mut b = stroke.color[2];
                        let mut opacity = 0.0;

                        if dt >= 0.0 {
                            // 過去〜現在：通常のフェードアウト表示
                            opacity = stroke.get_alpha(current_vt, lambda);
                            if is_chaos_mode {
                                if highlighted.contains(&idx) {
                                    r = 1.0; g = 0.0; b = 1.0;
                                } else {
                                    r = 0.8; g = 0.8; b = 0.8;
                                }
                            }
                        } else if !is_chaos_mode && dt >= -7.5 {
                            // 未来：オニオンスキン（緑色でフェードイン）
                            r = 0.0; g = 1.0; b = 0.0;
                            // 近づくにつれて不透明度を上げる (fade-in)
                            opacity = 1.0 - (dt.abs() / 7.5);
                            opacity *= 0.6; // オニオンスキン自体の最大透明度を少し抑える
                        }

                        if opacity > 0.01 {
                            render_data.push(StrokeData {
                                color: Color::from_rgb_f32(r, g, b),
                                path_data: stroke_to_svg(stroke, canvas_w, canvas_h),
                                width: stroke.width,
                                opacity,
                            });
                        }
                    }

                    if let Some(ref stroke) = *current_stroke.borrow() {
                        render_data.push(StrokeData {
                            color: Color::from_rgb_f32(stroke.color[0], stroke.color[1], stroke.color[2]),
                            path_data: stroke_to_svg(stroke, canvas_w, canvas_h),
                            width: stroke.width,
                            opacity: 1.0,
                        });
                    }

                    if is_chaos_mode {
                        waveform_update_counter += 1;
                        // 約300msごとに波形を再計算（vt_maxの増大やフェードに対応するため）
                        if waveform_update_counter >= 20 {
                            waveform_update_counter = 0;
                            let wlm = world_line_manager.borrow();
                            let s_list = strokes.borrow();
                            let active_id = *active_segment_id.borrow();
                            let vt_max = vt.get_max();
                            update_ui_segments(&ui, &wlm, &s_list, vt_max, active_id);
                        }
                    } else {
                        waveform_update_counter = 0;
                    }

                    let model = Rc::new(VecModel::from(render_data));
                    ui.set_strokes_to_render(model.into());
                }
            }
        },
    );

    let result = ui.run();

    // Final Save on Exit
    let last_vt = virtual_time.borrow().get_current();
    let state = State::get(&ui);
    let new_settings = AppSettings {
        last_vt,
        is_maximized: ui.window().is_maximized(),
        macro_ratio: state.get_macro_ratio(),
        middle_ratio: state.get_middle_ratio(),
    };
    let _ = storage::save_settings("settings.json", &new_settings);

    result
}
