slint::include_modules!();

mod logic;
mod storage;

use logic::{Stroke, VirtualTime, WorldLineManager};
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
    active_id: usize,
) {
    let mut ui_segments: Vec<SegmentData> = Vec::new();
    for segment in &wlm.segments {
        ui_segments.push(SegmentData {
            id: segment.id as i32,
            y: 0.0,
            highlighted: segment.id == active_id,
        });
    }
    let segments_model = Rc::new(VecModel::from(ui_segments));
    ui.set_timeline_segments(segments_model.into());
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
    let (initial_vt, initial_strokes) =
        storage::load_from_binary("data.bin").unwrap_or((0.0, Vec::new()));
    let strokes = Rc::new(RefCell::<Vec<Stroke>>::new(initial_strokes));
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
        // Restore last VT from settings
        vt.set_current(settings.last_vt);
    }

    // Calculate segments from loaded strokes
    {
        let mut wlm = world_line_manager.borrow_mut();
        wlm.calculate_segments(&strokes.borrow());
    }

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

                if !state.get_scrubbing() {
                    virtual_time.borrow_mut().set_playing(true);
                }

                if state.get_eraser() {
                    let mut s_list = strokes.borrow_mut();
                    let threshold = 0.05;
                    for stroke in s_list.iter_mut() {
                        if !stroke.is_erased {
                            for p in &stroke.points {
                                let dx = p.x - pos.x;
                                let dy = p.y - pos.y;
                                if (dx * dx + dy * dy).sqrt() < threshold {
                                    stroke.is_erased = true;
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
                    for stroke in s_list.iter_mut() {
                        if !stroke.is_erased {
                            for p in &stroke.points {
                                let dx = p.x - pos.x;
                                let dy = p.y - pos.y;
                                if (dx * dx + dy * dy).sqrt() < threshold {
                                    stroke.is_erased = true;
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
                let _ = storage::save_to_binary("data.bin", &data, current_vt);
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
        let active_segment_id = active_segment_id.clone();
        
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                let wlm = world_line_manager.borrow();
                let active_id = *active_segment_id.borrow();
                update_ui_segments(&ui, &wlm, active_id);
            }
        }
    });

    ui.on_worldline_changed({
        let ui_handle = ui.as_weak();
        let world_line_manager = world_line_manager.clone();
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
                        update_ui_segments(&ui, &wlm, new_id);
                        
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
                        if stroke.is_erased { continue; }
                        
                        let opacity = stroke.get_alpha(current_vt, lambda);
                        let mut r = stroke.color[0];
                        let mut g = stroke.color[1];
                        let mut b = stroke.color[2];

                        if is_chaos_mode {
                            if highlighted.contains(&idx) {
                                r = 1.0; g = 0.0; b = 1.0;
                            } else {
                                r = 0.8; g = 0.8; b = 0.8;
                            }
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
