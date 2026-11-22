use egui::{Color32, RichText};
use x806_core::model::{DeviceConfig, PollingRate};

use crate::tabs::polling_rate_label;

const POLLING_RATES: [PollingRate; 4] = [
    PollingRate::Hz125,
    PollingRate::Hz250,
    PollingRate::Hz500,
    PollingRate::Hz1000,
];

/// Renders the `Advanced Setting` tab: USB Report Rate, Fire Speed, and explicit OS-settings note.
pub fn ui(ui: &mut egui::Ui, cfg: &mut DeviceConfig) {
    ui.heading("Advanced Hardware Settings");
    ui.add_space(10.0);

    // 1. USB Report Rate
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width(520.0);
            ui.label(RichText::new("USB Report Rate (Polling Rate)").strong().size(15.0));
            ui.label(
                RichText::new(
                    "Stored in EEPROM 0x001E. Also controls the firmware macro tick period.",
                )
                .small()
                .weak(),
            );
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                for rate in POLLING_RATES {
                    let selected = cfg.polling_rate == rate;
                    if ui
                        .selectable_label(selected, polling_rate_label(rate))
                        .clicked()
                    {
                        cfg.polling_rate = rate;
                    }
                }
            });

            ui.add_space(6.0);
            ui.label(
                RichText::new(format!(
                    "Current tick period: {} ms (EEPROM divider = {})",
                    cfg.polling_rate.tick_ms(),
                    cfg.polling_rate.eeprom_divider()
                ))
                .small()
                .color(Color32::from_rgb(130, 200, 255)),
            );
        });

    ui.add_space(12.0);

    // 2. Fire Speed
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width(520.0);
            ui.label(RichText::new("Fire Key Speed").strong().size(15.0));
            ui.label(
                RichText::new(
                    "Interval between rapid-fire clicks for keys bound to Extend -> Fire (stored in EEPROM 0x001A..0x001B).",
                )
                .small()
                .weak(),
            );
            ui.add_space(8.0);

            let mut fire_ms = cfg.fire_time_ms.clamp(50, 300);
            let resp = ui.add(
                egui::Slider::new(&mut fire_ms, 50..=300)
                    .text("ms")
                    .step_by(1.0),
            );
            if resp.changed() || cfg.fire_time_ms != fire_ms {
                cfg.fire_time_ms = fire_ms;
            }
            ui.label(
                RichText::new(format!(
                    "EEPROM value: {} (fire_time_ms / 8)",
                    cfg.fire_time_ms / 8
                ))
                .small()
                .weak(),
            );
        });

    ui.add_space(16.0);

    // 3. Explicit note on omitted Windows-only OS pointer/wheel sliders
    egui::Frame::group(ui.style())
        .fill(Color32::from_rgb(36, 40, 48))
        .inner_margin(egui::Margin::same(14))
        .show(ui, |ui| {
            ui.set_min_width(520.0);
            ui.label(
                RichText::new("Note on Pointer Speed & Scroll Wheel Settings")
                    .strong()
                    .color(Color32::from_rgb(235, 195, 110)),
            );
            ui.add_space(4.0);
            ui.label(
                "The Pointer Speed (DPI_SPEED) and Wheel Speed (WHEEL_SPEED) sliders shown in \
                 Windows X806.exe only call the Windows-only SystemParametersInfoW OS API and are \
                 never written to the X806 keypad's EEPROM. They are intentionally omitted in this \
                 cross-platform configurator so every setting shown here reflects real on-board \
                 hardware state. Adjust pointer acceleration and scroll speed in your OS settings; \
                 imported config.ini values are still preserved transparently on Export.",
            );
        });
}
