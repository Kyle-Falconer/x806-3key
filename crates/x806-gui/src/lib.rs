pub mod app;
pub mod keymap;
pub mod recorder;
pub mod tabs;
pub mod theme;

/// Raw PNG bytes for the application icon (`assets/letter-x.png`).
pub const ICON_BYTES: &[u8] = include_bytes!("../../../assets/letter-x.png");

/// Loads the application window icon from bundled PNG bytes.
pub fn load_icon() -> Result<egui::IconData, String> {
    eframe::icon_data::from_png_bytes(ICON_BYTES).map_err(|err| err.to_string())
}
