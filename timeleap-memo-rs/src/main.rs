slint::include_modules!();

mod logic;
mod storage;

use logic::{Stroke, VirtualTime};
use std::rc::Rc;
use std::cell::RefCell;

fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;
    
    // Application State
    let strokes = Rc::new(RefCell::new(Vec::<Stroke>::new()));
    let current_stroke = Rc::new(RefCell::new(None::<Stroke>));
    let virtual_time = Rc::new(RefCell::new(VirtualTime::new()));
    
    let ui_handle = ui.as_weak();
    
    // Pointer Events
    ui.on_pointer_down({
        let virtual_time = virtual_time.clone();
        let current_stroke = current_stroke.clone();
        move |pos| {
            let vt = virtual_time.borrow().get_current();
            let mut stroke = Stroke::new(vt);
            stroke.add_point(pos.x, pos.y, 1.0);
            *current_stroke.borrow_mut() = Some(stroke);
            println!("Stroke started at vt: {}", vt);
        }
    });

    ui.on_pointer_move({
        let current_stroke = current_stroke.clone();
        move |pos| {
            if let Some(ref mut stroke) = *current_stroke.borrow_mut() {
                stroke.add_point(pos.x, pos.y, 1.0);
            }
        }
    });

    ui.on_pointer_up({
        let strokes = strokes.clone();
        let current_stroke = current_stroke.clone();
        let virtual_time = virtual_time.clone();
        move || {
            if let Some(stroke) = current_stroke.borrow_mut().take() {
                let vt = stroke.virtual_time_created;
                virtual_time.borrow_mut().update_max(vt);
                strokes.borrow_mut().push(stroke);
                println!("Stroke finished. Total strokes: {}", strokes.borrow().len());
            }
        }
    });

    ui.on_toggle_play({
        let virtual_time = virtual_time.clone();
        move || {
            virtual_time.borrow_mut().toggle_play();
        }
    });

    ui.on_go_to_now({
        let virtual_time = virtual_time.clone();
        move || {
            let mut vt = virtual_time.borrow_mut();
            vt.set_current(7.5); // Placeholder for max or latest
            // In actual implementation, we'd use vt.max
        }
    });

    // Update Timer (approx 60fps)
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(16), {
        let ui_handle = ui_handle.clone();
        let virtual_time = virtual_time.clone();
        move || {
            if let Some(ui) = ui_handle.upgrade() {
                let mut vt = virtual_time.borrow_mut();
                vt.advance(0.016);
                
                ui.set_virtual_time(vt.get_current());
                ui.set_is_playing(vt.is_playing());
            }
        }
    });

    ui.run()
}
