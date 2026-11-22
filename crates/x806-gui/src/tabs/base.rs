use egui::{Color32, RichText, Stroke, Vec2};
use x806_core::keycode::hid_display_name;
use x806_core::model::{BuiltinShortcut, DeviceConfig, KeyAction, MediaKey};

use crate::keymap::egui_key_to_hid;
use crate::tabs::{active_profile_index, key_action_label, media_key_label, shortcut_label};

/// Interactive state for the `Base Setting` tab.
#[derive(Debug, Default)]
pub struct BaseTabState {
    pub selected_key: usize,
    pub record_modal_open: bool,
    record_modal_just_opened: bool,
}

impl BaseTabState {
    /// Opens the single-key capture modal and skips key capture on the opening frame
    /// so activating `"Record Key..."` via `Enter`/`Space` cannot immediately bind itself.
    pub fn open_record_modal(&mut self) {
        self.record_modal_open = true;
        self.record_modal_just_opened = true;
    }

    /// Closes the single-key capture modal and clears pending opening-frame state.
    pub fn close_record_modal(&mut self) {
        self.record_modal_open = false;
        self.record_modal_just_opened = false;
    }

    /// Processes a pressed HID key candidate for the `Record Key` modal.
    ///
    /// Ignores the opening frame (`record_modal_just_opened`) and closes the modal once
    /// a subsequent frame captures a valid HID key.
    pub fn poll_record_modal_capture(&mut self, pressed_hid: Option<u8>) -> Option<u8> {
        if !self.record_modal_open {
            return None;
        }
        if self.record_modal_just_opened {
            self.record_modal_just_opened = false;
            return None;
        }
        if let Some(hid) = pressed_hid {
            self.close_record_modal();
            return Some(hid);
        }
        None
    }
}

const DIRECT_ITEMS: [(&str, KeyAction); 6] = [
    ("Left Button", KeyAction::LeftButton),
    ("Mid Button", KeyAction::MidButton),
    ("Right Button", KeyAction::RightButton),
    ("Backward", KeyAction::Backward),
    ("Forward", KeyAction::Forward),
    ("DPI Loop", KeyAction::DpiLoop),
];

const EXTEND_ITEMS: [(&str, KeyAction); 7] = [
    ("Show Desktop", KeyAction::ShowDesktop),
    ("Double Click", KeyAction::DoubleClick),
    ("Fire", KeyAction::Fire),
    ("LED Loop", KeyAction::LedLoop),
    ("DPI+", KeyAction::DpiUp),
    ("DPI-", KeyAction::DpiDown),
    ("Fn function", KeyAction::FnKey),
];

const MODE_ITEMS: [(&str, KeyAction); 3] = [
    ("Game I", KeyAction::ProfileGame1),
    ("Game II", KeyAction::ProfileGame2),
    ("Game III", KeyAction::ProfileGame3),
];

const MODAL_MODIFIER_KEYS: [(&str, u8); 8] = [
    ("LCtrl", 0xE0),
    ("LShift", 0xE1),
    ("LAlt", 0xE2),
    ("LWin", 0xE3),
    ("RCtrl", 0xE4),
    ("RShift", 0xE5),
    ("RAlt", 0xE6),
    ("RWin", 0xE7),
];

/// Renders the `Base Setting` tab: 2×4 keypad visual grid + categorized action picker + `Record Key` modal.
pub fn ui(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    cfg: &mut DeviceConfig,
    state: &mut BaseTabState,
) {
    let profile_idx = active_profile_index(cfg.active_profile);
    if state.selected_key >= 8 {
        state.selected_key = 0;
    }

    ui.columns(2, |cols| {
        // Left column: visual 2x4 physical keypad layout
        cols[0].heading(format!(
            "Keypad Layout — {}",
            cfg.profiles[profile_idx].name
        ));
        cols[0].label(
            RichText::new("Select a physical key (1–8) to assign an action.").weak(),
        );
        cols[0].add_space(12.0);

        egui::Frame::group(cols[0].style())
            .inner_margin(egui::Margin::same(16))
            .corner_radius(10.0)
            .show(&mut cols[0], |ui| {
                // Top row: keys 1..4 (indices 0..4), Bottom row: keys 5..8 (indices 4..8)
                for row in 0..2 {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);
                        for col in 0..4 {
                            let idx = row * 4 + col;
                            let action = cfg.profiles[profile_idx].keys[idx];
                            let is_selected = state.selected_key == idx;
                            draw_keycap_button(ui, idx, action, cfg, is_selected, state);
                        }
                    });
                    if row == 0 {
                        ui.add_space(10.0);
                    }
                }
            });

        cols[0].add_space(12.0);
        let current_action = cfg.profiles[profile_idx].keys[state.selected_key];
        egui::Frame::group(cols[0].style())
            .inner_margin(egui::Margin::same(10))
            .show(&mut cols[0], |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Selected: Key {}", state.selected_key + 1)).strong());
                    ui.separator();
                    ui.label(
                        RichText::new(key_action_label(current_action, cfg))
                            .color(Color32::from_rgb(130, 210, 255)),
                    );
                });
                ui.label(
                    RichText::new(format!(
                        "Hardware slot: K{}_MAP = {}",
                        state.selected_key + 1,
                        cfg.keymap[state.selected_key]
                    ))
                    .small()
                    .weak(),
                );
            });

        // Right column: categorized X806.exe action picker
        cols[1].heading(format!("Action for Key {}", state.selected_key + 1));
        cols[1].add_space(6.0);

        egui::ScrollArea::vertical()
            .id_salt("base_action_picker_scroll")
            .auto_shrink([false, false])
            .show(&mut cols[1], |ui| {
                render_action_picker(ui, cfg, profile_idx, state);
            });
    });

    if state.record_modal_open {
        render_record_key_modal(ctx, cfg, profile_idx, state);
    }
}

fn draw_keycap_button(
    ui: &mut egui::Ui,
    idx: usize,
    action: KeyAction,
    cfg: &DeviceConfig,
    is_selected: bool,
    state: &mut BaseTabState,
) {
    let size = Vec2::new(102.0, 86.0);
    let fill = if is_selected {
        Color32::from_rgb(38, 72, 118)
    } else if action == KeyAction::Disabled {
        Color32::from_rgb(32, 34, 38)
    } else {
        Color32::from_rgb(44, 48, 56)
    };
    let stroke = if is_selected {
        Stroke::new(2.0_f32, Color32::from_rgb(95, 180, 255))
    } else {
        Stroke::new(1.0_f32, Color32::from_rgb(76, 82, 92))
    };

    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        state.selected_key = idx;
    }

    let hovered_fill = if response.hovered() && !is_selected {
        Color32::from_rgb(54, 60, 72)
    } else {
        fill
    };

    ui.painter()
        .rect(rect, 8.0, hovered_fill, stroke, egui::StrokeKind::Inside);

    let key_title = format!("Key {}", idx + 1);
    let binding = key_action_label(action, cfg);

    let inner = rect.shrink(6.0);
    ui.painter().text(
        inner.left_top(),
        egui::Align2::LEFT_TOP,
        key_title,
        egui::FontId::proportional(11.5),
        Color32::from_rgb(165, 178, 198),
    );
    ui.painter().text(
        inner.center(),
        egui::Align2::CENTER_CENTER,
        truncate_label(&binding, 14),
        egui::FontId::proportional(13.0),
        if action == KeyAction::Disabled {
            Color32::from_rgb(128, 128, 134)
        } else {
            Color32::WHITE
        },
    );

    response.on_hover_text(format!("Key {} — {binding}", idx + 1));
}

fn render_action_picker(
    ui: &mut egui::Ui,
    cfg: &mut DeviceConfig,
    profile_idx: usize,
    state: &mut BaseTabState,
) {
    let key_idx = state.selected_key;
    let current = cfg.profiles[profile_idx].keys[key_idx];

    // Quick top actions: Record Key modal + Disabled
    ui.horizontal(|ui| {
        if ui
            .button(RichText::new("Record Key...").strong())
            .on_hover_text("Press any keyboard key to bind as a single key (42,<hid>)")
            .clicked()
        {
            state.open_record_modal();
        }
        if ui
            .selectable_label(current == KeyAction::Disabled, "Disabled")
            .clicked()
        {
            cfg.profiles[profile_idx].keys[key_idx] = KeyAction::Disabled;
        }
    });
    ui.add_space(8.0);

    // 1. Direct (6 items)
    egui::CollapsingHeader::new("Direct Mouse & DPI")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (label, action) in DIRECT_ITEMS {
                    if ui.selectable_label(current == action, label).clicked() {
                        cfg.profiles[profile_idx].keys[key_idx] = action;
                    }
                }
            });
        });

    // 2. Extend (7 items)
    egui::CollapsingHeader::new("Extend")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (label, action) in EXTEND_ITEMS {
                    if ui.selectable_label(current == action, label).clicked() {
                        cfg.profiles[profile_idx].keys[key_idx] = action;
                    }
                }
            });
        });

    // 3. Media (10 items)
    egui::CollapsingHeader::new("Media")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for media in MediaKey::ALL {
                    let action = KeyAction::Media(media);
                    if ui
                        .selectable_label(current == action, media_key_label(media))
                        .clicked()
                    {
                        cfg.profiles[profile_idx].keys[key_idx] = action;
                    }
                }
            });
        });

    // 4. Macro (Group -> Macro)
    egui::CollapsingHeader::new("Macro")
        .default_open(true)
        .show(ui, |ui| {
            if cfg.macros.groups.is_empty() {
                ui.label(
                    RichText::new("No user macro groups yet. Create macros in the Macro tab.")
                        .weak(),
                );
            } else {
                let mut picked: Option<KeyAction> = None;
                for (g_idx, group) in cfg.macros.groups.iter().enumerate() {
                    let group_title = if group.name.is_empty() {
                        format!("Group {}", g_idx + 1)
                    } else {
                        group.name.clone()
                    };
                    egui::CollapsingHeader::new(group_title)
                        .id_salt(("base_macro_grp", g_idx))
                        .default_open(true)
                        .show(ui, |ui| {
                            if group.macros.is_empty() {
                                ui.label(RichText::new("(empty group)").weak());
                            } else {
                                ui.horizontal_wrapped(|ui| {
                                    for (m_idx, mac) in group.macros.iter().enumerate() {
                                        let (Ok(group_u8), Ok(index_u8)) =
                                            (u8::try_from(g_idx), u8::try_from(m_idx))
                                        else {
                                            continue;
                                        };
                                        let action = KeyAction::UserMacro {
                                            group: group_u8,
                                            index: index_u8,
                                        };
                                        let name = if mac.name.is_empty() {
                                            format!("Macro {}", m_idx + 1)
                                        } else {
                                            mac.name.clone()
                                        };
                                        if ui.selectable_label(current == action, name).clicked() {
                                            picked = Some(action);
                                        }
                                    }
                                });
                            }
                        });
                }
                if let Some(action) = picked {
                    cfg.profiles[profile_idx].keys[key_idx] = action;
                }
            }

            if !cfg.macros.pubg_macros.is_empty() {
                egui::CollapsingHeader::new("Imported PUBG Macros")
                    .id_salt("base_pubg_macros")
                    .default_open(false)
                    .show(ui, |ui| {
                        let mut picked: Option<KeyAction> = None;
                        ui.horizontal_wrapped(|ui| {
                            for (idx, mac) in cfg.macros.pubg_macros.iter().enumerate() {
                                let action = KeyAction::PubgMacro { index: idx as u8 };
                                if ui.selectable_label(current == action, &mac.name).clicked() {
                                    picked = Some(action);
                                }
                            }
                        });
                        if let Some(action) = picked {
                            cfg.profiles[profile_idx].keys[key_idx] = action;
                        }
                    });
            }
        });

    // 5. Shortcut (8 items)
    egui::CollapsingHeader::new("Shortcut")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for shortcut in BuiltinShortcut::ALL {
                    let action = KeyAction::Shortcut(shortcut);
                    if ui
                        .selectable_label(current == action, shortcut_label(shortcut))
                        .clicked()
                    {
                        cfg.profiles[profile_idx].keys[key_idx] = action;
                    }
                }
            });
        });

    // 6. Mode (3 items)
    egui::CollapsingHeader::new("Mode")
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (label, action) in MODE_ITEMS {
                    if ui.selectable_label(current == action, label).clicked() {
                        cfg.profiles[profile_idx].keys[key_idx] = action;
                    }
                }
            });
        });
}

fn render_record_key_modal(
    ctx: &egui::Context,
    cfg: &mut DeviceConfig,
    profile_idx: usize,
    state: &mut BaseTabState,
) {
    let key_idx = state.selected_key;
    let mut pressed_hid: Option<u8> = None;

    ctx.input(|input| {
        for ev in &input.events {
            if let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                ..
            } = ev
                && let Some(hid) = egui_key_to_hid(*key)
            {
                pressed_hid = Some(hid);
                break;
            }
        }
    });

    if let Some(hid) = state.poll_record_modal_capture(pressed_hid) {
        cfg.profiles[profile_idx].keys[key_idx] = KeyAction::SingleKey { hid };
        return;
    }

    let mut open = state.record_modal_open;
    egui::Window::new(format!("Record Key — Key {}", key_idx + 1))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_min_width(340.0);
            ui.label(
                RichText::new("Press any keyboard key now to bind it.")
                    .size(15.0)
                    .strong(),
            );
            ui.add_space(6.0);
            if let KeyAction::SingleKey { hid } = cfg.profiles[profile_idx].keys[key_idx] {
                ui.label(
                    RichText::new(format!(
                        "Current: {} (HID 0x{hid:02X})",
                        hid_display_name(hid)
                    ))
                    .weak(),
                );
            }
            ui.separator();
            ui.label(RichText::new("Or select a bare modifier key:").small().weak());
            ui.horizontal_wrapped(|ui| {
                for (label, hid) in MODAL_MODIFIER_KEYS {
                    if ui.button(label).clicked() {
                        cfg.profiles[profile_idx].keys[key_idx] = KeyAction::SingleKey { hid };
                        state.close_record_modal();
                    }
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    state.close_record_modal();
                }
            });
        });

    if !open {
        state.close_record_modal();
    }
}

fn truncate_label(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let collected: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{collected}…")
    } else {
        collected
    }
}
