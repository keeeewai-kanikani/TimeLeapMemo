slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;

    ui.on_pointer_down(|pos| {
        println!("Pointer down at: {}, {}", pos.x, pos.y);
    });

    ui.on_pointer_move(|pos| {
        // Here we will add logic to record strokes
    });

    ui.on_pointer_up(|| {
        println!("Pointer up");
    });

    ui.run()
}
