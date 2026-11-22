use egui::{Color32, RichText};
use x806_core::model::{DeviceConfig, LedMode, ReactiveStyle, Rgb4};

use crate::tabs::{active_profile_index, led_mode_label, reactive_style_label};

const STANDARD_LED_MODES: [LedMode; 3] = [
    LedMode::CycleBreath,
    LedMode::ButtonResponse,
    LedMode::Mute,
];

const ADVANCED_LED_MODES: [LedMode; 5] = [
    LedMode::DpiBreath,
    LedMode::Constant,
    LedMode::SingleBreath,
    LedMode::ConstantAlt,
    LedMode::CometTail,
];

const REACTIVE_STYLES: [ReactiveStyle; 4] = [
    ReactiveStyle::CycleBreath,
    ReactiveStyle::FlowWater,
    ReactiveStyle::CometTail,
    ReactiveStyle::SingleWater,
];

/// Renders the `LED Setting` tab: LED modes, active-low 4-bit RGB cycle colors, per-key reactive
/// configuration, cycle speed (`0..=6`), and collapsible advanced firmware modes & brightness.
pub fn ui(ui: &mut egui::Ui, cfg: &mut DeviceConfig) {
    let profile_idx = active_profile_index(cfg.active_profile);

    egui::ScrollArea::vertical()
        .id_salt("led_tab_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.heading("LED Lighting Settings");
            ui.add_space(8.0);

            // 1. LED Mode + Cycle Speed
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.label(RichText::new("Lighting Mode").strong().size(14.5));
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        for mode in STANDARD_LED_MODES {
                            if ui
                                .selectable_label(cfg.led.mode == mode, led_mode_label(mode))
                                .clicked()
                            {
                                cfg.led.mode = mode;
                            }
                        }
                    });

                    ui.add_space(10.0);
                    ui.add_enabled_ui(cfg.led.mode != LedMode::Mute, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Cycle Speed (0 = fastest, 6 = slowest):");
                            ui.add(egui::Slider::new(&mut cfg.led.speed, 0..=6));
                        });
                    });
                    if cfg.led.mode == LedMode::Mute {
                        ui.label(
                            RichText::new(
                                "Mute mode writes speed=7 (LED off) to EEPROM 0x001C automatically.",
                            )
                            .small()
                            .weak(),
                        );
                    }

                    ui.add_space(8.0);
                    egui::CollapsingHeader::new("Advanced Firmware Modes & Brightness")
                        .default_open(false)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                for mode in ADVANCED_LED_MODES {
                                    if ui
                                        .selectable_label(
                                            cfg.led.mode == mode,
                                            led_mode_label(mode),
                                        )
                                        .clicked()
                                    {
                                        cfg.led.mode = mode;
                                    }
                                }
                            });
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label("Hardware Brightness (0–7):");
                                ui.add(egui::Slider::new(&mut cfg.led.brightness, 0..=7));
                            });
                            ui.horizontal(|ui| {
                                ui.checkbox(
                                    &mut cfg.led.user_color_en,
                                    "Enable User Palette (0x001D[7])",
                                );
                                ui.checkbox(&mut cfg.led.symmetry, "Symmetry");
                                ui.label("Direction:");
                                ui.add(egui::DragValue::new(&mut cfg.led.direction).range(0..=3));
                            });
                        });
                });

            ui.add_space(10.0);

            // 2. 4-Slot Cycle Colors (CYC_7COL_EN bits 0..3 + DPI0..3 active-low 4-bit RGB)
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(12))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new("Cycle Color Palette (4-bit Active-Low Hardware RGB)")
                            .strong()
                            .size(14.5),
                    );
                    ui.label(
                        RichText::new(
                            "Hardware PWM levels are active-low 0..15 (0 = 100% ON, 15 = OFF). Pickers quantize automatically.",
                        )
                        .small()
                        .weak(),
                    );
                    ui.add_space(8.0);

                    egui::Grid::new("led_cycle_colors_grid")
                        .num_columns(4)
                        .spacing([14.0, 6.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Cycle Enable").strong());
                            ui.label(RichText::new("Slot").strong());
                            ui.label(RichText::new("Color").strong());
                            ui.label(RichText::new("Active-Low PWM (0..15)").strong());
                            ui.end_row();

                            for slot in 0..4 {
                                let mask_bit = 1u8 << slot;
                                let mut enabled = (cfg.led.cycle_mask & mask_bit) != 0;
                                if ui.checkbox(&mut enabled, format!("Slot {}", slot + 1)).changed() {
                                    if enabled {
                                        cfg.led.cycle_mask |= mask_bit;
                                    } else {
                                        cfg.led.cycle_mask &= !mask_bit;
                                    }
                                }

                                ui.label(format!("DPI {}", slot + 1));

                                let mut rgb8 = cfg.led.dpi_colors[slot].to_rgb8();
                                if ui.color_edit_button_srgb(&mut rgb8).changed() {
                                    cfg.led.dpi_colors[slot] =
                                        Rgb4::from_rgb8(rgb8[0], rgb8[1], rgb8[2]);
                                }

                                let c = cfg.led.dpi_colors[slot];
                                ui.label(
                                    RichText::new(format!("R:{:2}  G:{:2}  B:{:2}", c.r, c.g, c.b))
                                        .monospace()
                                        .small()
                                        .weak(),
                                );
                                ui.end_row();
                            }
                        });

                    ui.add_space(6.0);
                    egui::CollapsingHeader::new("Optional Extra DPI Colors (DPI 5–6)")
                        .default_open(false)
                        .show(ui, |ui| {
                            for slot in 4..6 {
                                ui.horizontal(|ui| {
                                    ui.label(format!("DPI {}:", slot + 1));
                                    let mut rgb8 = cfg.led.dpi_colors[slot].to_rgb8();
                                    if ui.color_edit_button_srgb(&mut rgb8).changed() {
                                        cfg.led.dpi_colors[slot] =
                                            Rgb4::from_rgb8(rgb8[0], rgb8[1], rgb8[2]);
                                    }
                                    let c = cfg.led.dpi_colors[slot];
                                    ui.label(
                                        RichText::new(format!(
                                            "R:{:2}  G:{:2}  B:{:2}",
                                            c.r, c.g, c.b
                                        ))
                                        .monospace()
                                        .small()
                                        .weak(),
                                    );
                                });
                            }
                        });
                });

            // 3. Per-key reactive settings when Button Response mode is active
            if cfg.led.mode == LedMode::ButtonResponse {
                ui.add_space(10.0);
                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        let prof = &mut cfg.profiles[profile_idx];
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!(
                                    "Per-Key Button Response — {}",
                                    prof.name
                                ))
                                .strong()
                                .size(14.5)
                                .color(Color32::from_rgb(130, 210, 255)),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.small_button("Disable All").clicked() {
                                        prof.led_react_en = [false; 8];
                                    }
                                    if ui.small_button("Enable All").clicked() {
                                        prof.led_react_en = [true; 8];
                                    }
                                },
                            );
                        });
                        ui.add_space(6.0);

                        egui::Grid::new("led_reactive_keys_grid")
                            .striped(true)
                            .num_columns(4)
                            .spacing([14.0, 6.0])
                            .show(ui, |ui| {
                                for row in 0..4 {
                                    for side in 0..2 {
                                        let key_idx = side * 4 + row;
                                        ui.checkbox(
                                            &mut prof.led_react_en[key_idx],
                                            format!("Key {}", key_idx + 1),
                                        );
                                        let style = &mut prof.led_react_type[key_idx];
                                        egui::ComboBox::from_id_salt((
                                            "react_style",
                                            profile_idx,
                                            key_idx,
                                        ))
                                        .selected_text(reactive_style_label(*style))
                                        .width(120.0)
                                        .show_ui(ui, |ui| {
                                            for opt in REACTIVE_STYLES {
                                                ui.selectable_value(
                                                    style,
                                                    opt,
                                                    reactive_style_label(opt),
                                                );
                                            }
                                        });
                                    }
                                    ui.end_row();
                                }
                            });
                    });
            }
        });
}
