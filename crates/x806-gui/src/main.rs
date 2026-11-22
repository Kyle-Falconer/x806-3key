use eframe::egui;
use x806_gui::app::X806App;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1060.0, 700.0])
            .with_min_inner_size([920.0, 620.0]),
        ..Default::default()
    };

    eframe::run_native(
        "X806 Keypad Configurator",
        options,
        Box::new(|cc| Ok(Box::new(X806App::new(cc)))),
    )
}
