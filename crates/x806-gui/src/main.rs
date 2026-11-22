use std::sync::Arc;

use eframe::egui;
use x806_gui::app::X806App;

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_app_id("x806-gui")
        .with_inner_size([1060.0, 700.0])
        .with_min_inner_size([920.0, 620.0]);

    if let Ok(icon) = x806_gui::load_icon() {
        viewport = viewport.with_icon(Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "X806 Keypad Configurator",
        options,
        Box::new(|cc| Ok(Box::new(X806App::new(cc)))),
    )
}
