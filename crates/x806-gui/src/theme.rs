//! Theme and visual style tokens aligned with the Google Stitch design system.

use egui::{Color32, Stroke, Visuals};

// Surface & Chassis Foundation
pub const CHASSIS_BG: Color32 = Color32::from_rgb(0x18, 0x1A, 0x1F); // #181a1f
pub const SURFACE_DIM: Color32 = Color32::from_rgb(0x11, 0x13, 0x18); // #111318
pub const SURFACE_CONTAINER_LOW: Color32 = Color32::from_rgb(0x1A, 0x1C, 0x21); // #1a1c21
pub const SURFACE_1: Color32 = Color32::from_rgb(0x20, 0x22, 0x28); // #202228
pub const SURFACE_2: Color32 = Color32::from_rgb(0x28, 0x2C, 0x34); // #282c34
pub const SURFACE_CONTAINER_HIGH: Color32 = Color32::from_rgb(0x28, 0x2A, 0x2F); // #282a2f
pub const SURFACE_CONTAINER_HIGHEST: Color32 = Color32::from_rgb(0x33, 0x35, 0x3A); // #33353a
pub const BORDER_HAIRLINE: Color32 = Color32::from_rgb(0x38, 0x3E, 0x4A); // #383e4a
pub const OUTLINE: Color32 = Color32::from_rgb(0x8A, 0x91, 0x9A); // #8a919a
pub const OUTLINE_VARIANT: Color32 = Color32::from_rgb(0x40, 0x48, 0x4F); // #40484f

// Brand & Accent Colors
pub const PRIMARY_CYAN: Color32 = Color32::from_rgb(0x78, 0xC3, 0xFF); // #78c3ff
pub const PRIMARY_LIGHT: Color32 = Color32::from_rgb(0xB7, 0xDC, 0xFF); // #b7dcff
pub const PRIMARY_FIXED_DIM: Color32 = Color32::from_rgb(0x91, 0xCC, 0xFF); // #91ccff
pub const SECONDARY_NEON: Color32 = Color32::from_rgb(0x80, 0xD0, 0xFD); // #80d0fd
pub const SECONDARY_CONTAINER: Color32 = Color32::from_rgb(0x00, 0x70, 0x98); // #007098

// Keycap Geometry States
pub const KEYCAP_BASE: Color32 = Color32::from_rgb(0x2C, 0x30, 0x38); // #2c3038
pub const KEYCAP_DISABLED: Color32 = Color32::from_rgb(0x20, 0x22, 0x26); // #202226
pub const KEYCAP_HOVER_FILL: Color32 = Color32::from_rgb(0x36, 0x3C, 0x48); // #363c48
pub const KEYCAP_SELECTED_FILL: Color32 = Color32::from_rgb(0x26, 0x48, 0x76); // #264876
pub const KEYCAP_SELECTED_BORDER: Color32 = Color32::from_rgb(0x5F, 0xB4, 0xFF); // #5fb4ff

// Functional & Telemetry Indicators
pub const STATUS_ONLINE: Color32 = Color32::from_rgb(0x10, 0xB9, 0x81); // #10b981
pub const COMMIT_GREEN: Color32 = Color32::from_rgb(0x10, 0xB9, 0x81); // #10b981
pub const STATUS_OFFLINE: Color32 = Color32::from_rgb(0xEA, 0xB3, 0x08); // #eab308
pub const RECORD_RED: Color32 = Color32::from_rgb(0xEF, 0x44, 0x44); // #ef4444
pub const TOAST_SUCCESS: Color32 = Color32::from_rgb(0x05, 0x96, 0x69); // #059669
pub const TOAST_ERROR: Color32 = Color32::from_rgb(0xDC, 0x26, 0x26); // #dc2626
pub const TOAST_INFO: Color32 = Color32::from_rgb(0x00, 0x70, 0x98); // #007098

// Typography Colors
pub const TEXT_PRIMARY: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF); // #ffffff
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xA5, 0xB2, 0xC6); // #a5b2c6
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x8A, 0x91, 0x9A); // #8a919a
pub const CODE_TELEMETRY: Color32 = Color32::from_rgb(0x80, 0xD0, 0xFD); // #80d0fd

/// Standard CAD HUD container frame with deep background and hairline border.
pub fn cad_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE_1)
        .stroke(Stroke::new(1.0_f32, BORDER_HAIRLINE))
        .corner_radius(egui::CornerRadius::same(8))
        .inner_margin(egui::Margin::same(12))
}

/// Renders a segmented navigation or tab pill button.
pub fn pill_tab(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let fill = if selected {
        KEYCAP_SELECTED_FILL
    } else {
        SURFACE_2
    };
    let stroke = if selected {
        Stroke::new(1.5_f32, PRIMARY_CYAN)
    } else {
        Stroke::new(1.0_f32, BORDER_HAIRLINE)
    };
    let text_color = if selected {
        TEXT_PRIMARY
    } else {
        TEXT_SECONDARY
    };

    let text = egui::RichText::new(label)
        .size(13.0)
        .strong()
        .color(text_color);

    let btn = egui::Button::new(text)
        .fill(fill)
        .stroke(stroke)
        .corner_radius(egui::CornerRadius::same(6))
        .min_size(egui::Vec2::new(0.0, 28.0));

    ui.add(btn)
}

/// Renders a secondary tool button (e.g. Refresh, Import, Export, Reset).
pub fn secondary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let text = egui::RichText::new(label).size(12.5).color(TEXT_SECONDARY);

    let btn = egui::Button::new(text)
        .fill(SURFACE_2)
        .stroke(Stroke::new(1.0_f32, BORDER_HAIRLINE))
        .corner_radius(egui::CornerRadius::same(5))
        .min_size(egui::Vec2::new(0.0, 26.0));

    ui.add(btn)
}

/// Applies the Google Stitch Precision Industrial CAD HUD theme to egui.
pub fn apply_stitch_theme(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();

    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.panel_fill = CHASSIS_BG;
    visuals.window_fill = SURFACE_1;
    visuals.faint_bg_color = SURFACE_CONTAINER_LOW;
    visuals.extreme_bg_color = SURFACE_DIM;
    visuals.code_bg_color = SURFACE_DIM;

    // Noninteractive elements (group frames, labels)
    visuals.widgets.noninteractive.bg_fill = SURFACE_1;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_HAIRLINE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);

    // Inactive interactive widgets (buttons, checkboxes)
    visuals.widgets.inactive.bg_fill = SURFACE_2;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_HAIRLINE);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT_SECONDARY);

    // Hovered widgets
    visuals.widgets.hovered.bg_fill = KEYCAP_HOVER_FILL;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, KEYCAP_SELECTED_BORDER);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);

    // Active / pressed widgets
    visuals.widgets.active.bg_fill = KEYCAP_SELECTED_FILL;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, PRIMARY_CYAN);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT_PRIMARY);

    // Selection
    visuals.selection.bg_fill = KEYCAP_SELECTED_FILL;
    visuals.selection.stroke = Stroke::new(1.0_f32, PRIMARY_CYAN);

    ctx.set_visuals(visuals);
}
