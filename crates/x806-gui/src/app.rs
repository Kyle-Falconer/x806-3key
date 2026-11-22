use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use egui::{Color32, RichText, Stroke, Vec2};
use x806_core::codec::{ini, toml as toml_codec};
use x806_core::eeprom::{CompileContext, EepromPlan, compile_plan};
use x806_core::hid::{DeviceStatus, HidApiTransport, X806Device};
use x806_core::model::DeviceConfig;
use x806_core::{X806Error, defaults};

use crate::tabs::base::BaseTabState;
use crate::tabs::macro_tab::MacroTabState;
use crate::tabs::{ActiveTab, advanced, base, led, macro_tab};

const ACTIVE_PROFILE_SLOTS: [(u8, &str); 3] = [(0, "Game I"), (2, "Game II"), (3, "Game III")];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToastKind {
    Success,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
struct Toast {
    kind: ToastKind,
    message: String,
}

/// Main `eframe`/`egui` desktop configurator for the X806 / K806 8-key keypad.
pub struct X806App {
    config: DeviceConfig,
    saved_config: DeviceConfig,
    active_tab: ActiveTab,
    config_path: Option<PathBuf>,
    device_status: Option<DeviceStatus>,
    device_error: Option<String>,
    toast: Option<Toast>,
    base_state: BaseTabState,
    macro_state: MacroTabState,
}

impl X806App {
    /// Initializes dark theme, loads local TOML config if present, and probes attached hardware.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::theme::apply_stitch_theme(&cc.egui_ctx);

        let config_path =
            ProjectDirs::from("", "", "x806").map(|proj| proj.config_dir().join("config.toml"));

        let mut startup_toast = None;
        let mut config = match config_path.as_ref().filter(|p| p.exists()) {
            Some(path) => match load_toml_file(path) {
                Ok(cfg) => cfg,
                Err(err) => {
                    startup_toast = Some(Toast {
                        kind: ToastKind::Error,
                        message: format!(
                            "Failed to load '{}': {err}. Loaded factory defaults instead.",
                            path.display()
                        ),
                    });
                    defaults::factory_default_config()
                }
            },
            None => defaults::factory_default_config(),
        };
        normalize_config(&mut config);

        let mut app = Self {
            saved_config: config.clone(),
            config,
            active_tab: ActiveTab::Base,
            config_path,
            device_status: None,
            device_error: None,
            toast: startup_toast,
            base_state: BaseTabState::default(),
            macro_state: MacroTabState::default(),
        };
        app.refresh_hardware();
        app
    }

    fn is_dirty(&self) -> bool {
        self.config != self.saved_config
    }

    fn set_toast(&mut self, kind: ToastKind, message: impl Into<String>) {
        self.toast = Some(Toast {
            kind,
            message: message.into(),
        });
    }

    /// Probes the attached X806 keypad (read-only `GetFeature(0x07)`). Never panics.
    fn refresh_hardware(&mut self) {
        match HidApiTransport::open_first() {
            Ok((transport, manufacturer, product)) => {
                let mut dev = X806Device::with_strings(transport, manufacturer, product);
                match dev.read_status() {
                    Ok(status) => {
                        self.device_status = Some(status);
                        self.device_error = None;
                    }
                    Err(err) => {
                        self.device_status = None;
                        self.device_error = Some(err.to_string());
                    }
                }
            }
            Err(X806Error::DeviceNotFound) => {
                self.device_status = None;
                self.device_error = None;
            }
            Err(err) => {
                self.device_status = None;
                self.device_error = Some(err.to_string());
            }
        }
    }

    fn handle_import(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Import X806 Configuration")
            .add_filter("X806 Config (TOML / INI)", &["toml", "ini"])
            .add_filter("TOML Configuration", &["toml"])
            .add_filter("Windows X806.exe config.ini", &["ini"])
            .pick_file()
        else {
            return;
        };

        let result = if is_ini_path(&path) {
            ini::import_x806_bundle(&path, None, None, None)
        } else {
            load_toml_file(&path)
        };

        match result {
            Ok(mut imported) => {
                self.macro_state.cancel_recording();
                self.base_state.close_record_modal();
                normalize_config(&mut imported);
                let grp_count = imported.macros.groups.len();
                self.config = imported;
                self.set_toast(
                    ToastKind::Success,
                    format!(
                        "Imported '{}' ({grp_count} macro group(s)). Click Apply to flash.",
                        path.display()
                    ),
                );
            }
            Err(err) => {
                self.set_toast(
                    ToastKind::Error,
                    format!("Import failed for '{}': {err}", path.display()),
                );
            }
        }
    }

    fn handle_export(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_title("Export X806 Configuration")
            .set_file_name("x806-config.toml")
            .add_filter("TOML Configuration", &["toml"])
            .add_filter("Windows X806.exe config.ini", &["ini"])
            .save_file()
        else {
            return;
        };

        let result = if is_ini_path(&path) {
            let macros_dir = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .join("macro");
            ini::export_x806_bundle(&self.config, &path, &macros_dir)
        } else {
            save_toml_file(&path, &self.config)
        };

        match result {
            Ok(()) => {
                self.set_toast(
                    ToastKind::Success,
                    format!("Exported configuration to '{}'.", path.display()),
                );
            }
            Err(err) => {
                self.set_toast(
                    ToastKind::Error,
                    format!("Export failed for '{}': {err}", path.display()),
                );
            }
        }
    }

    fn handle_reset(&mut self) {
        self.macro_state.cancel_recording();
        self.base_state.close_record_modal();
        self.config = defaults::factory_default_config();
        self.set_toast(
            ToastKind::Info,
            "Restored factory defaults (keys 1–8, Cycle breath, 500 Hz). Click Apply to flash.",
        );
    }

    fn handle_cancel(&mut self) {
        self.macro_state.cancel_recording();
        self.base_state.close_record_modal();
        self.config = self.saved_config.clone();
        self.set_toast(ToastKind::Info, "Reverted unsaved edits.");
    }

    fn handle_apply(&mut self) {
        self.macro_state.cancel_recording();
        self.base_state.close_record_modal();

        // 1. Validate & flash connected hardware first (never persist invalid/rejected configs).
        match HidApiTransport::open_first() {
            Ok((transport, manufacturer, product)) => {
                let mut dev = X806Device::with_strings(transport, manufacturer, product);
                match dev.flash_config(&self.config) {
                    Ok(plan) => {
                        let refreshed = dev.read_status().ok();
                        let _ = dev.send_exit();
                        drop(dev);

                        if let Some(status) = refreshed {
                            self.device_status = Some(status);
                            self.device_error = None;
                        } else {
                            self.refresh_hardware();
                        }

                        self.saved_config = self.config.clone();
                        let persist_suffix = match &self.config_path {
                            Some(path) => match save_toml_file(path, &self.config) {
                                Ok(()) => String::new(),
                                Err(err) => format!(
                                    " (warning: local save to '{}' failed: {err})",
                                    path.display()
                                ),
                            },
                            None => {
                                " (warning: OS config directory unavailable; not saved locally)"
                                    .to_string()
                            }
                        };
                        self.set_toast(
                            ToastKind::Success,
                            format!(
                                "Applied to X806 ({} EEPROM blocks, {}/{} macro bytes).{persist_suffix}",
                                plan.writes.len(),
                                plan.macro_bytes_used,
                                plan.macro_limit
                            ),
                        );
                    }
                    Err(err) => {
                        drop(dev);
                        self.set_toast(
                            ToastKind::Error,
                            format!("Hardware flash failed (local config not modified): {err}"),
                        );
                    }
                }
            }
            Err(X806Error::DeviceNotFound) => {
                self.device_status = None;
                self.device_error = None;
                match validate_and_save_offline(&self.config, self.config_path.as_deref()) {
                    Ok((plan, Some(saved_path))) => {
                        self.saved_config = self.config.clone();
                        self.set_toast(
                            ToastKind::Info,
                            format!(
                                "Saved local config to '{}' (offline — validated {} EEPROM blocks, {}/{} macro bytes). Connect X806 and click Apply to flash.",
                                saved_path.display(),
                                plan.writes.len(),
                                plan.macro_bytes_used,
                                plan.macro_limit
                            ),
                        );
                    }
                    Ok((plan, None)) => {
                        self.set_toast(
                            ToastKind::Error,
                            format!(
                                "Validated config offline ({} EEPROM blocks, {}/{} macro bytes), but OS config directory is unavailable so nothing was saved to disk.",
                                plan.writes.len(),
                                plan.macro_bytes_used,
                                plan.macro_limit
                            ),
                        );
                    }
                    Err(err) => {
                        self.set_toast(ToastKind::Error, err);
                    }
                }
            }
            Err(err) => {
                self.device_status = None;
                self.device_error = Some(err.to_string());
                self.set_toast(
                    ToastKind::Error,
                    format!("Could not open X806 HID interface (local config not modified): {err}"),
                );
            }
        }
    }

    fn render_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("x806_top_bar")
            .frame(
                egui::Frame::new()
                    .fill(crate::theme::SURFACE_DIM)
                    .stroke(Stroke::new(1.0_f32, crate::theme::BORDER_HAIRLINE))
                    .inner_margin(egui::Margin::symmetric(16, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
                    ui.label(
                        RichText::new("X806")
                            .strong()
                            .size(18.0)
                            .color(crate::theme::PRIMARY_CYAN),
                    );
                    ui.label(
                        RichText::new("CONFIG // STUDIO")
                            .size(11.0)
                            .monospace()
                            .color(crate::theme::TEXT_MUTED),
                    );
                    ui.add_space(12.0);

                    for tab in ActiveTab::ALL {
                        let selected = self.active_tab == tab;
                        if crate::theme::pill_tab(ui, selected, tab.label()).clicked() {
                            if self.active_tab == ActiveTab::Macro && tab != ActiveTab::Macro {
                                self.macro_state.cancel_recording();
                            }
                            if self.active_tab == ActiveTab::Base && tab != ActiveTab::Base {
                                self.base_state.close_record_modal();
                            }
                            self.active_tab = tab;
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::theme::secondary_button(ui, "Refresh").clicked() {
                            self.refresh_hardware();
                        }

                        // Hardware Status Capsule Pill Badge
                        let (status_text, dot_color, tooltip) =
                            if let Some(status) = &self.device_status {
                                let eeprom_kb = match status.macro_limit {
                                    0x0FF8 => "4 KB EEPROM",
                                    0x07F8 => "2 KB EEPROM",
                                    _ => "Custom EEPROM",
                                };
                                let product = status.product.as_deref().unwrap_or("X806");
                                (
                                    format!("● Connected: {product} • {eeprom_kb}"),
                                    crate::theme::STATUS_ONLINE,
                                    None,
                                )
                            } else if let Some(err) = &self.device_error {
                                (
                                    "● HID Error".to_string(),
                                    crate::theme::TOAST_ERROR,
                                    Some(err.clone()),
                                )
                            } else {
                                (
                                    "○ Offline (Disconnected)".to_string(),
                                    crate::theme::STATUS_OFFLINE,
                                    None,
                                )
                            };

                        let badge = egui::Frame::new()
                            .fill(crate::theme::SURFACE_1)
                            .stroke(Stroke::new(1.0_f32, crate::theme::BORDER_HAIRLINE))
                            .corner_radius(egui::CornerRadius::same(12))
                            .inner_margin(egui::Margin::symmetric(10, 4));

                        let resp = badge.show(ui, |ui| {
                            ui.label(
                                RichText::new(status_text)
                                    .monospace()
                                    .size(11.5)
                                    .color(dot_color)
                                    .strong(),
                            );
                        });
                        if let Some(tip) = tooltip {
                            resp.response.on_hover_text(tip);
                        }
                    });
                });
            });
    }

    fn render_toast_bar(&mut self, ctx: &egui::Context) {
        let Some(toast) = self.toast.clone() else {
            return;
        };
        let (fill, text_color) = match toast.kind {
            ToastKind::Success => (crate::theme::SURFACE_1, crate::theme::STATUS_ONLINE),
            ToastKind::Info => (crate::theme::SURFACE_1, crate::theme::PRIMARY_CYAN),
            ToastKind::Warning => (crate::theme::SURFACE_1, crate::theme::STATUS_OFFLINE),
            ToastKind::Error => (crate::theme::SURFACE_1, crate::theme::TOAST_ERROR),
        };

        egui::TopBottomPanel::top("x806_toast_bar")
            .frame(
                egui::Frame::new()
                    .fill(fill)
                    .inner_margin(egui::Margin::symmetric(12, 6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&toast.message).color(text_color));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Dismiss").clicked() {
                            self.toast = None;
                        }
                    });
                });
            });
    }

    fn render_bottom_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("x806_bottom_bar")
            .frame(
                egui::Frame::new()
                    .fill(crate::theme::SURFACE_DIM)
                    .stroke(Stroke::new(1.0_f32, crate::theme::BORDER_HAIRLINE))
                    .inner_margin(egui::Margin::symmetric(16, 8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("PROFILE:")
                            .size(11.0)
                            .monospace()
                            .color(crate::theme::TEXT_MUTED)
                            .strong(),
                    );
                    for (slot, label) in ACTIVE_PROFILE_SLOTS {
                        let selected = self.config.active_profile == slot;
                        if crate::theme::pill_tab(ui, selected, label).clicked() {
                            self.config.active_profile = slot;
                        }
                    }

                    if self.is_dirty() {
                        ui.add_space(8.0);
                        egui::Frame::new()
                            .fill(crate::theme::SURFACE_1)
                            .stroke(Stroke::new(1.0_f32, crate::theme::STATUS_OFFLINE))
                            .corner_radius(egui::CornerRadius::same(10))
                            .inner_margin(egui::Margin::symmetric(8, 2))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("● UNSAVED CHANGES")
                                        .monospace()
                                        .size(10.5)
                                        .color(crate::theme::STATUS_OFFLINE)
                                        .strong(),
                                );
                            });
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let apply_btn = egui::Button::new(
                            RichText::new("APPLY TO EEPROM")
                                .color(Color32::WHITE)
                                .strong()
                                .size(12.5),
                        )
                        .fill(crate::theme::COMMIT_GREEN)
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(16, 185, 129)))
                        .corner_radius(egui::CornerRadius::same(6))
                        .min_size(Vec2::new(140.0, 30.0));

                        if ui.add(apply_btn).clicked() {
                            self.handle_apply();
                        }

                        if self.is_dirty() && crate::theme::secondary_button(ui, "Cancel").clicked()
                        {
                            self.handle_cancel();
                        }

                        ui.separator();

                        if crate::theme::secondary_button(ui, "Reset").clicked() {
                            self.handle_reset();
                        }
                        if crate::theme::secondary_button(ui, "Export").clicked() {
                            self.handle_export();
                        }
                        if crate::theme::secondary_button(ui, "Import").clicked() {
                            self.handle_import();
                        }
                    });
                });
            });
    }
}

impl eframe::App for X806App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.render_top_bar(ctx);
        self.render_toast_bar(ctx);
        self.render_bottom_bar(ctx);

        egui::CentralPanel::default().show(ctx, |ui| match self.active_tab {
            ActiveTab::Base => base::ui(ctx, ui, &mut self.config, &mut self.base_state),
            ActiveTab::Macro => {
                macro_tab::ui(ctx, ui, &mut self.config, &mut self.macro_state);
                if let Some(warning) = self.macro_state.take_warning() {
                    self.set_toast(ToastKind::Warning, warning);
                }
            }
            ActiveTab::Advanced => advanced::ui(ui, &mut self.config),
            ActiveTab::Led => led::ui(ui, &mut self.config),
        });
    }
}

/// Normalizes a loaded or imported [`DeviceConfig`] into valid hardware/UI bounds so rendering
/// tabs never mutates an untouched config and dead EEPROM profile slot `1` is never selected.
pub fn normalize_config(cfg: &mut DeviceConfig) {
    if !matches!(cfg.active_profile, 0 | 2 | 3) {
        cfg.active_profile = 0;
    }
    cfg.fire_time_ms = cfg.fire_time_ms.clamp(50, 300);
    cfg.led.speed = cfg.led.speed.min(6);
}

/// Validates `cfg` against the default EEPROM layout before persisting TOML to `config_path`.
///
/// Guarantees invalid configs are never written to disk, and distinguishes `config_path == None`
/// (`Ok((plan, None))`) from a successful file write (`Ok((plan, Some(path)))`).
pub fn validate_and_save_offline(
    cfg: &DeviceConfig,
    config_path: Option<&Path>,
) -> Result<(EepromPlan, Option<PathBuf>), String> {
    let plan = compile_plan(cfg, CompileContext::default()).map_err(|err| {
        format!("Configuration validation failed (local config not modified): {err}")
    })?;

    let Some(path) = config_path else {
        return Ok((plan, None));
    };

    save_toml_file(path, cfg).map_err(|err| {
        format!(
            "Validated config offline, but failed to save '{}': {err}",
            path.display()
        )
    })?;
    Ok((plan, Some(path.to_path_buf())))
}

fn is_ini_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("ini"))
}

fn load_toml_file(path: &Path) -> Result<DeviceConfig, X806Error> {
    let text = fs::read_to_string(path)?;
    toml_codec::from_toml_str(&text)
}

fn save_toml_file(path: &Path, cfg: &DeviceConfig) -> Result<(), X806Error> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    let text = toml_codec::to_toml_string(cfg)?;
    fs::write(path, text)?;
    Ok(())
}
