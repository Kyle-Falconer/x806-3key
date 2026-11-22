use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use egui::{Color32, RichText};
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
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let config_path = ProjectDirs::from("", "", "x806")
            .map(|proj| proj.config_dir().join("config.toml"));

        let mut startup_toast = None;
        let config = match config_path.as_ref().filter(|p| p.exists()) {
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
            Ok(imported) => {
                self.macro_state.cancel_recording();
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
        self.config = defaults::factory_default_config();
        self.set_toast(
            ToastKind::Info,
            "Restored factory defaults (keys 1–8, Cycle breath, 500 Hz). Click Apply to flash.",
        );
    }

    fn handle_cancel(&mut self) {
        self.macro_state.cancel_recording();
        self.config = self.saved_config.clone();
        self.set_toast(ToastKind::Info, "Reverted unsaved edits.");
    }

    fn handle_apply(&mut self) {
        self.macro_state.cancel_recording();

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
                    format!(
                        "Could not open X806 HID interface (local config not modified): {err}"
                    ),
                );
            }
        }
    }

    fn render_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("x806_top_bar")
            .exact_height(46.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(
                        RichText::new("X806")
                            .strong()
                            .size(17.0)
                            .color(Color32::from_rgb(120, 195, 255)),
                    );
                    ui.separator();

                    for tab in ActiveTab::ALL {
                        let selected = self.active_tab == tab;
                        if ui
                            .selectable_label(selected, RichText::new(tab.label()).size(14.0))
                            .clicked()
                        {
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
                        if ui.button("Refresh").clicked() {
                            self.refresh_hardware();
                        }

                        if let Some(status) = &self.device_status {
                            let eeprom_kb = match status.macro_limit {
                                0x0FF8 => "4 KB EEPROM",
                                0x07F8 => "2 KB EEPROM",
                                _ => "Custom EEPROM",
                            };
                            let product = status.product.as_deref().unwrap_or("X806");
                            ui.label(
                                RichText::new(format!("● Connected ({product}, {eeprom_kb})"))
                                    .color(Color32::from_rgb(100, 220, 130))
                                    .strong(),
                            );
                        } else if let Some(err) = &self.device_error {
                            ui.label(
                                RichText::new("● HID Error")
                                    .color(Color32::from_rgb(245, 115, 115))
                                    .strong(),
                            )
                            .on_hover_text(err);
                        } else {
                            ui.label(
                                RichText::new("○ Disconnected (Offline)")
                                    .color(Color32::from_rgb(225, 185, 100)),
                            );
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
            ToastKind::Success => (
                Color32::from_rgb(26, 64, 42),
                Color32::from_rgb(175, 245, 195),
            ),
            ToastKind::Info => (
                Color32::from_rgb(32, 52, 78),
                Color32::from_rgb(185, 220, 255),
            ),
            ToastKind::Error => (
                Color32::from_rgb(82, 28, 32),
                Color32::from_rgb(255, 190, 190),
            ),
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
            .exact_height(48.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("Profile:").weak());
                    for (slot, label) in ACTIVE_PROFILE_SLOTS {
                        let selected = self.config.active_profile == slot;
                        if ui
                            .selectable_label(selected, RichText::new(label).strong())
                            .clicked()
                        {
                            self.config.active_profile = slot;
                        }
                    }

                    if self.is_dirty() {
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("• Unsaved changes")
                                .small()
                                .color(Color32::from_rgb(240, 195, 105)),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let apply_btn = egui::Button::new(
                            RichText::new("Apply").color(Color32::WHITE).strong(),
                        )
                        .fill(Color32::from_rgb(44, 115, 195));
                        if ui.add(apply_btn).clicked() {
                            self.handle_apply();
                        }

                        if ui
                            .add_enabled(self.is_dirty(), egui::Button::new("Cancel"))
                            .clicked()
                        {
                            self.handle_cancel();
                        }

                        ui.separator();

                        if ui.button("Reset").clicked() {
                            self.handle_reset();
                        }
                        if ui.button("Export").clicked() {
                            self.handle_export();
                        }
                        if ui.button("Import").clicked() {
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
            ActiveTab::Macro => macro_tab::ui(ctx, ui, &mut self.config, &mut self.macro_state),
            ActiveTab::Advanced => advanced::ui(ui, &mut self.config),
            ActiveTab::Led => led::ui(ui, &mut self.config),
        });
    }
}

/// Validates `cfg` against the default EEPROM layout before persisting TOML to `config_path`.
///
/// Guarantees invalid configs are never written to disk, and distinguishes `config_path == None`
/// (`Ok((plan, None))`) from a successful file write (`Ok((plan, Some(path)))`).
pub fn validate_and_save_offline(
    cfg: &DeviceConfig,
    config_path: Option<&Path>,
) -> Result<(EepromPlan, Option<PathBuf>), String> {
    let plan = compile_plan(cfg, CompileContext::default())
        .map_err(|err| format!("Configuration validation failed (local config not modified): {err}"))?;

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
