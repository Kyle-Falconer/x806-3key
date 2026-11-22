use std::time::Instant;

use egui::{Color32, RichText};
use x806_core::eeprom::compile_macro_bytecode;
use x806_core::keycode::hid_display_name;
use x806_core::model::{
    DeviceConfig, KeyAction, Macro, MacroEvent, MacroEventKind, MacroGroup, RepeatMode,
};

use crate::keymap::egui_key_to_hid;
use crate::recorder::{DEFAULT_TRAILING_DELAY_MS, MAX_RECORDED_EVENTS, MacroRecorder, ModState};

/// Maximum macro name length in UTF-16 code units matching `.dat` binary records (`60 bytes`).
pub const MAX_MACRO_NAME_UTF16_UNITS: usize = 30;
/// Maximum events per macro record in `.dat` files.
pub const MAX_MACRO_EVENTS: usize = 180;
/// Maximum number of user macro groups (`group_idx + 1` fits in `u8` for `config.ini` encoding).
pub const MAX_MACRO_GROUPS: usize = 254;
/// Maximum number of macros per group (`item_idx` fits in `u8`).
pub const MAX_MACROS_PER_GROUP: usize = 256;

const QUICK_MODIFIERS: [(&str, u8); 8] = [
    ("LCtrl", 0xE0),
    ("LShift", 0xE1),
    ("LAlt", 0xE2),
    ("LWin", 0xE3),
    ("RCtrl", 0xE4),
    ("RShift", 0xE5),
    ("RAlt", 0xE6),
    ("RWin", 0xE7),
];

const QUICK_MOUSE: [(&str, u8); 3] = [("LMB", 0xF1), ("MMB", 0xF2), ("RMB", 0xF3)];

/// Clamps a macro name so its UTF-16 encoding never exceeds `30` code units (`60` bytes).
pub fn clamp_macro_name_utf16(name: &str) -> String {
    let mut out = String::new();
    let mut units = 0usize;
    for ch in name.chars() {
        let len = ch.len_utf16();
        if units + len > MAX_MACRO_NAME_UTF16_UNITS {
            break;
        }
        out.push(ch);
        units += len;
    }
    out
}

/// Extracts recordable `(hid, pressed)` transitions from `egui::Event`s, including
/// `Event::Copy` (`0x06`), `Event::Cut` (`0x1B`), and `Event::Paste(_)` (`0x19`)
/// emitted by `egui-winit` in place of key-down for `Ctrl+C`/`X`/`V`.
pub fn extract_recordable_key_events(events: &[egui::Event]) -> Vec<(u8, bool)> {
    let mut keys = Vec::new();
    for ev in events {
        match ev {
            egui::Event::Key { key, pressed, .. } => {
                if let Some(hid) = egui_key_to_hid(*key) {
                    keys.push((hid, *pressed));
                }
            }
            egui::Event::Copy => keys.push((0x06, true)),
            egui::Event::Cut => keys.push((0x1B, true)),
            egui::Event::Paste(_) => keys.push((0x19, true)),
            _ => {}
        }
    }
    keys
}

/// Appends `captured` events into `target` up to [`MAX_MACRO_EVENTS`], ensuring the appended
/// slice never leaves unmatched `KeyDown` events stuck down on the host OS.
///
/// Returns `Some(warning)` when truncation occurs.
pub fn append_recorded_events_safely(
    target: &mut Vec<MacroEvent>,
    captured: Vec<MacroEvent>,
) -> Option<String> {
    let orig_len = captured.len();
    let remaining = MAX_MACRO_EVENTS.saturating_sub(target.len());
    if orig_len <= remaining {
        target.extend(captured);
        return None;
    }

    if remaining == 0 {
        return Some(format!(
            "Macro already reached the {MAX_MACRO_EVENTS}-event limit; {orig_len} recorded event(s) were not appended."
        ));
    }

    let mut kept: Vec<MacroEvent> = Vec::with_capacity(remaining);
    let mut held: Vec<u8> = Vec::new();

    for ev in captured {
        match ev.kind {
            MacroEventKind::KeyDown { hid } => {
                if held.contains(&hid) {
                    continue;
                }
                // Reserve room for this KeyDown + its eventual KeyUp + all currently held KeyUps.
                if kept.len() + held.len() + 2 > remaining {
                    break;
                }
                held.push(hid);
                kept.push(ev);
            }
            MacroEventKind::KeyUp { hid } => {
                if let Some(pos) = held.iter().position(|&h| h == hid) {
                    held.remove(pos);
                    kept.push(ev);
                } else {
                    if kept.len() + held.len() + 1 > remaining {
                        break;
                    }
                    kept.push(ev);
                }
            }
            _ => {
                if kept.len() + held.len() + 1 > remaining {
                    break;
                }
                kept.push(ev);
            }
        }
    }

    for hid in held.drain(..).rev() {
        kept.push(MacroEvent {
            kind: MacroEventKind::KeyUp { hid },
            delay_ms: DEFAULT_TRAILING_DELAY_MS,
        });
    }

    let kept_len = kept.len();
    target.extend(kept);
    Some(format!(
        "Macro reached the {MAX_MACRO_EVENTS}-event limit: kept {kept_len} of {orig_len} recorded event(s) (held keys closed safely)."
    ))
}

/// Validates a macro group name against path-traversal characters and case-insensitive duplicates
/// so `<group>.dat` exports never collide or escape the macro directory.
pub fn validate_group_name(
    name: &str,
    existing: &[MacroGroup],
    ignore_idx: Option<usize>,
) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    if name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name.contains('\0')
        || name.contains(':')
        || trimmed == "."
    {
        return Err(format!(
            "Invalid group name '{name}': path separators and '..' are not allowed."
        ));
    }
    let duplicate = existing.iter().enumerate().any(|(idx, g)| {
        Some(idx) != ignore_idx && !g.name.trim().is_empty() && g.name.eq_ignore_ascii_case(trimmed)
    });
    if duplicate {
        return Err(format!(
            "Group name '{trimmed}' already exists (names must be unique for <group>.dat export)."
        ));
    }
    Ok(())
}

/// Interactive state for the `Macro` tab.
pub struct MacroTabState {
    pub selected_group: usize,
    pub selected_macro: usize,
    pub recorder: MacroRecorder,
    pub record_started_at: Option<Instant>,
    pub new_group_name: String,
    pub new_macro_name: String,
    pub insert_dx: i16,
    pub insert_dy: i16,
    pub insert_dpi_code: u8,
    pub insert_delay_ms: u16,
    pub insert_tap_mode: bool,
    pub repeat_count_buf: u16,
    pub pending_warning: Option<String>,
}

impl MacroTabState {
    /// Cancels any active live recording session without committing captured events.
    pub fn cancel_recording(&mut self) {
        if self.recorder.is_recording() {
            let _ = self.recorder.stop();
        }
        self.record_started_at = None;
    }

    /// Takes any pending warning message raised by macro tab actions (truncation, caps, duplicates).
    pub fn take_warning(&mut self) -> Option<String> {
        self.pending_warning.take()
    }

    /// Clamps group/macro selection indices to `cfg` and auto-cancels any active recording
    /// if no valid macro remains selected (e.g. after Reset, Cancel, or Import).
    pub fn sync_with_config(&mut self, cfg: &DeviceConfig) {
        clamp_selections(cfg, self);
        let has_macro = cfg
            .macros
            .groups
            .get(self.selected_group)
            .is_some_and(|g| self.selected_macro < g.macros.len());
        if !has_macro && self.recorder.is_recording() {
            self.cancel_recording();
        }
    }
}

impl Default for MacroTabState {
    fn default() -> Self {
        Self {
            selected_group: 0,
            selected_macro: 0,
            recorder: MacroRecorder::new(),
            record_started_at: None,
            new_group_name: String::new(),
            new_macro_name: String::new(),
            insert_dx: 0,
            insert_dy: 5,
            insert_dpi_code: 3,
            insert_delay_ms: 50,
            insert_tap_mode: true,
            repeat_count_buf: 2,
            pending_warning: None,
        }
    }
}

/// Renders the `Macro` tab: group/macro CRUD, live recorder, quick inserts, step table, repeat modes.
pub fn ui(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    cfg: &mut DeviceConfig,
    state: &mut MacroTabState,
) {
    state.sync_with_config(cfg);

    // Isolate keyboard focus and poll live keyboard & modifier events while recording.
    if state.recorder.is_recording() {
        ctx.memory_mut(|mem| {
            if let Some(focused) = mem.focused() {
                mem.surrender_focus(focused);
            }
        });

        let now_ms = state
            .record_started_at
            .map_or(0, |t| t.elapsed().as_millis() as u64);
        let (mod_state, key_events) = ctx.input(|input| {
            let mods = ModState::from_egui(input.modifiers);
            let keys = extract_recordable_key_events(&input.events);
            (mods, keys)
        });

        state.recorder.on_modifiers_changed(mod_state, now_ms);
        for (hid, pressed) in key_events {
            state.recorder.on_key_event(hid, pressed, now_ms);
        }
        ctx.request_repaint();
    }

    let available_width = ui.available_width();
    let left_width = (available_width * 0.27).clamp(210.0, 290.0);
    let right_width = (available_width * 0.24).clamp(190.0, 250.0);

    ui.horizontal_top(|ui| {
        // Left pane: macro groups, macros CRUD, Record/Stop
        ui.vertical(|ui| {
            ui.set_width(left_width);
            render_left_pane(ui, cfg, state);
        });

        ui.separator();

        // Middle pane: quick insert + editable step table
        let mid_width = (ui.available_width() - right_width - 24.0).max(260.0);
        ui.vertical(|ui| {
            ui.set_width(mid_width);
            render_middle_pane(ui, cfg, state);
        });

        ui.separator();

        // Right pane: repeat mode & options
        ui.vertical(|ui| {
            ui.set_width(right_width);
            render_right_pane(ui, cfg, state);
        });
    });
}

fn clamp_selections(cfg: &DeviceConfig, state: &mut MacroTabState) {
    if cfg.macros.groups.is_empty() {
        state.selected_group = 0;
        state.selected_macro = 0;
    } else {
        if state.selected_group >= cfg.macros.groups.len() {
            state.selected_group = cfg.macros.groups.len() - 1;
        }
        let mac_len = cfg.macros.groups[state.selected_group].macros.len();
        if mac_len == 0 {
            state.selected_macro = 0;
        } else if state.selected_macro >= mac_len {
            state.selected_macro = mac_len - 1;
        }
    }
}

fn render_left_pane(ui: &mut egui::Ui, cfg: &mut DeviceConfig, state: &mut MacroTabState) {
    let is_recording = state.recorder.is_recording();

    ui.add_enabled_ui(!is_recording, |ui| {
        ui.heading("Macro Groups");
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut state.new_group_name)
                    .desired_width(130.0)
                    .hint_text("Group name"),
            );
            let add_clicked = ui.button("+ Group").clicked()
                || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if add_clicked {
                if cfg.macros.groups.len() >= MAX_MACRO_GROUPS {
                    state.pending_warning = Some(format!(
                        "Cannot add more than {MAX_MACRO_GROUPS} macro groups (config.ini 8-bit group limit)."
                    ));
                } else {
                    let trimmed = state.new_group_name.trim();
                    let name = if trimmed.is_empty() {
                        next_unique_group_name(&cfg.macros.groups)
                    } else {
                        clamp_macro_name_utf16(trimmed)
                    };
                    match validate_group_name(&name, &cfg.macros.groups, None) {
                        Ok(()) => {
                            cfg.macros.groups.push(MacroGroup {
                                name,
                                macros: Vec::new(),
                            });
                            state.selected_group = cfg.macros.groups.len() - 1;
                            state.selected_macro = 0;
                            state.new_group_name.clear();
                        }
                        Err(msg) => {
                            state.pending_warning = Some(msg);
                        }
                    }
                }
            }
        });

        crate::theme::cad_frame().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("macro_groups_list")
                .max_height(130.0)
                .show(ui, |ui| {
                    if cfg.macros.groups.is_empty() {
                        ui.label(RichText::new("No macro groups.").weak());
                    } else {
                        for (idx, grp) in cfg.macros.groups.iter().enumerate() {
                            let label = format!("{} ({})", grp.name, grp.macros.len());
                            if ui
                                .selectable_label(state.selected_group == idx, label)
                                .clicked()
                            {
                                state.selected_group = idx;
                                state.selected_macro = 0;
                            }
                        }
                    }
                });
        });

        if !cfg.macros.groups.is_empty() {
            ui.horizontal(|ui| {
                ui.label("Rename:");
                let mut edited = cfg.macros.groups[state.selected_group].name.clone();
                if ui
                    .add(egui::TextEdit::singleline(&mut edited).desired_width(110.0))
                    .changed()
                {
                    let candidate = clamp_macro_name_utf16(&edited);
                    match validate_group_name(
                        &candidate,
                        &cfg.macros.groups,
                        Some(state.selected_group),
                    ) {
                        Ok(()) => {
                            cfg.macros.groups[state.selected_group].name = candidate;
                        }
                        Err(msg) => {
                            state.pending_warning = Some(msg);
                        }
                    }
                }
                if ui
                    .button("Del")
                    .on_hover_text("Delete selected macro group")
                    .clicked()
                {
                    let removed = state.selected_group;
                    cfg.macros.groups.remove(removed);
                    if let Ok(removed_u8) = u8::try_from(removed) {
                        adjust_bindings_after_group_delete(cfg, removed_u8);
                    }
                    clamp_selections(cfg, state);
                }
            });
        }

        ui.add_space(10.0);
        ui.heading("Macros");

        if !cfg.macros.groups.is_empty() {
            ui.horizontal(|ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut state.new_macro_name)
                        .desired_width(130.0)
                        .hint_text("Macro name"),
                );
                let add_clicked = ui.button("+ Macro").clicked()
                    || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if add_clicked {
                    let grp = &mut cfg.macros.groups[state.selected_group];
                    if grp.macros.len() >= MAX_MACROS_PER_GROUP {
                        state.pending_warning = Some(format!(
                            "Cannot add more than {MAX_MACROS_PER_GROUP} macros in one group (8-bit index limit)."
                        ));
                    } else {
                        let trimmed = state.new_macro_name.trim();
                        let name = if trimmed.is_empty() {
                            format!("Macro{}", grp.macros.len() + 1)
                        } else {
                            clamp_macro_name_utf16(trimmed)
                        };
                        grp.macros.push(Macro {
                            name,
                            repeat_mode: RepeatMode::Once,
                            finish_cycle: false,
                            opt_flag: 0,
                            opt_val: 10,
                            events: Vec::new(),
                            led_blocks: Vec::new(),
                        });
                        state.selected_macro = grp.macros.len() - 1;
                        state.new_macro_name.clear();
                    }
                }
            });

            crate::theme::cad_frame().show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("macro_items_list")
                    .max_height(150.0)
                    .show(ui, |ui| {
                        let macros = &cfg.macros.groups[state.selected_group].macros;
                        if macros.is_empty() {
                            ui.label(RichText::new("No macros in this group.").weak());
                        } else {
                            for (idx, mac) in macros.iter().enumerate() {
                                let label = format!("{} ({} steps)", mac.name, mac.events.len());
                                if ui
                                    .selectable_label(state.selected_macro == idx, label)
                                    .clicked()
                                {
                                    state.selected_macro = idx;
                                }
                            }
                        }
                    });
            });

            if !cfg.macros.groups[state.selected_group].macros.is_empty() {
                let mac = &mut cfg.macros.groups[state.selected_group].macros[state.selected_macro];
                let units = mac.name.encode_utf16().count();
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    let mut edited = mac.name.clone();
                    if ui
                        .add(egui::TextEdit::singleline(&mut edited).desired_width(115.0))
                        .changed()
                    {
                        mac.name = clamp_macro_name_utf16(&edited);
                    }
                    ui.label(
                        RichText::new(format!("{units}/{MAX_MACRO_NAME_UTF16_UNITS}"))
                            .small()
                            .weak(),
                    );
                });
            }
        } else {
            ui.label(RichText::new("Create a group first.").weak());
        }
    });

    // Record / Stop controls stay interactive while recording so Stop is always reachable.
    if cfg
        .macros
        .groups
        .get(state.selected_group)
        .is_some_and(|g| state.selected_macro < g.macros.len())
    {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if is_recording {
                let stop_btn = egui::Button::new(
                    RichText::new("Stop Recording")
                        .color(Color32::WHITE)
                        .strong(),
                )
                .fill(crate::theme::RECORD_RED);
                if ui.add(stop_btn).clicked() {
                    let captured = state.recorder.stop();
                    state.record_started_at = None;
                    let target =
                        &mut cfg.macros.groups[state.selected_group].macros[state.selected_macro];
                    if let Some(warning) =
                        append_recorded_events_safely(&mut target.events, captured)
                    {
                        state.pending_warning = Some(warning);
                    }
                }
            } else {
                let rec_btn =
                    egui::Button::new(RichText::new("Record").color(Color32::WHITE).strong())
                        .fill(crate::theme::COMMIT_GREEN);
                if ui.add(rec_btn).clicked() {
                    state.record_started_at = Some(Instant::now());
                    state.recorder.start(0);
                }
                if ui
                    .button("Delete Macro")
                    .on_hover_text("Delete selected macro")
                    .clicked()
                {
                    let g = state.selected_group;
                    let m = state.selected_macro;
                    cfg.macros.groups[g].macros.remove(m);
                    if let (Ok(g_u8), Ok(m_u8)) = (u8::try_from(g), u8::try_from(m)) {
                        adjust_bindings_after_macro_delete(cfg, g_u8, m_u8);
                    }
                    clamp_selections(cfg, state);
                }
            }
        });

        if state.recorder.is_recording() {
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "Recording... {}/{} events (type keys now)",
                    state.recorder.events().len(),
                    MAX_RECORDED_EVENTS
                ))
                .color(crate::theme::RECORD_RED)
                .small(),
            );
        }
    }
}

fn render_middle_pane(ui: &mut egui::Ui, cfg: &mut DeviceConfig, state: &mut MacroTabState) {
    ui.heading("Key List & Event Editor");

    let has_macro = cfg
        .macros
        .groups
        .get(state.selected_group)
        .is_some_and(|g| state.selected_macro < g.macros.len());

    if !has_macro {
        ui.add_space(8.0);
        ui.label(RichText::new("Select or create a macro to edit its event steps.").weak());
        return;
    }

    let is_recording = state.recorder.is_recording();
    let polling_ms = cfg.polling_rate.tick_ms();
    let mac = &mut cfg.macros.groups[state.selected_group].macros[state.selected_macro];

    // Quick insert panel (disabled while recording so keystrokes go exclusively to MacroRecorder)
    ui.add_enabled_ui(!is_recording, |ui| {
        crate::theme::cad_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Quick Insert").strong());
                ui.checkbox(&mut state.insert_tap_mode, "Insert Down + Up pair")
                    .on_hover_text(
                        "When checked, modifier & mouse buttons insert both KeyDown and KeyUp.",
                    );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add_enabled(!mac.events.is_empty(), egui::Button::new("Clear All"))
                        .clicked()
                    {
                        mac.events.clear();
                    }
                });
            });

            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Modifiers:").small().weak());
                for (label, hid) in QUICK_MODIFIERS {
                    if ui.small_button(label).clicked() {
                        insert_key_or_tap(&mut mac.events, hid, state.insert_tap_mode);
                    }
                }
            });

            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Mouse:").small().weak());
                for (label, hid) in QUICK_MOUSE {
                    if ui.small_button(label).clicked() {
                        insert_key_or_tap(&mut mac.events, hid, state.insert_tap_mode);
                    }
                }
                ui.separator();
                ui.label(RichText::new("Delay:").small().weak());
                ui.add(
                    egui::DragValue::new(&mut state.insert_delay_ms)
                        .range(1..=10_000)
                        .suffix(" ms"),
                );
                if ui.small_button("+ Delay").clicked() && mac.events.len() < MAX_MACRO_EVENTS {
                    mac.events.push(MacroEvent {
                        kind: MacroEventKind::Delay,
                        delay_ms: state.insert_delay_ms,
                    });
                }
            });

            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("XY Move:").small().weak());
                ui.label("dX");
                ui.add(egui::DragValue::new(&mut state.insert_dx).range(-5000..=5000));
                ui.label("dY");
                ui.add(egui::DragValue::new(&mut state.insert_dy).range(-5000..=5000));
                if ui.small_button("+ XY").clicked() && mac.events.len() < MAX_MACRO_EVENTS {
                    mac.events.push(MacroEvent {
                        kind: MacroEventKind::MoveRelative {
                            dx: state.insert_dx,
                            dy: state.insert_dy,
                        },
                        delay_ms: DEFAULT_TRAILING_DELAY_MS,
                    });
                }
                ui.separator();
                ui.label(RichText::new("DPI:").small().weak());
                ui.add(egui::DragValue::new(&mut state.insert_dpi_code).range(0..=16));
                if ui.small_button("+ DPI Set").clicked() && mac.events.len() < MAX_MACRO_EVENTS {
                    mac.events.push(MacroEvent {
                        kind: MacroEventKind::DpiSet {
                            dpi_code: state.insert_dpi_code,
                        },
                        delay_ms: DEFAULT_TRAILING_DELAY_MS,
                    });
                }
                if ui.small_button("+ DPI Restore").clicked() && mac.events.len() < MAX_MACRO_EVENTS
                {
                    mac.events.push(MacroEvent {
                        kind: MacroEventKind::DpiRestore {
                            dpi_code: state.insert_dpi_code,
                        },
                        delay_ms: DEFAULT_TRAILING_DELAY_MS,
                    });
                }
            });
        });
    });

    ui.add_space(6.0);

    // Live preview during recording OR editable step table
    if state.recorder.is_recording() {
        ui.label(RichText::new("Live Capture Buffer (click Stop Recording to commit):").strong());
        egui::ScrollArea::vertical()
            .id_salt("macro_live_buffer")
            .max_height(260.0)
            .show(ui, |ui| {
                for (i, ev) in state.recorder.events().iter().enumerate() {
                    ui.label(format!(
                        "{:03}: {} ({} ms)",
                        i + 1,
                        describe_event_kind(&ev.kind),
                        ev.delay_ms
                    ));
                }
            });
        return;
    }

    let mut swap_up: Option<usize> = None;
    let mut swap_down: Option<usize> = None;
    let mut delete_idx: Option<usize> = None;

    let step_len = mac.events.len();
    crate::theme::cad_frame().show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("macro_steps_table")
            .max_height(290.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if mac.events.is_empty() {
                    ui.label(
                        RichText::new("No steps yet. Click Record or use Quick Insert above.")
                            .weak(),
                    );
                } else {
                    egui::Grid::new("macro_steps_grid")
                        .striped(true)
                        .num_columns(4)
                        .spacing([8.0, 4.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("#").strong());
                            ui.label(RichText::new("Event").strong());
                            ui.label(RichText::new("Delay").strong());
                            ui.label(RichText::new("Order").strong());
                            ui.end_row();

                            for (idx, ev) in mac.events.iter_mut().enumerate() {
                                ui.label(format!("{:03}", idx + 1));

                                ui.horizontal(|ui| match &mut ev.kind {
                                    MacroEventKind::KeyDown { hid } => {
                                        ui.label(
                                            RichText::new("DOWN")
                                                .color(crate::theme::STATUS_ONLINE)
                                                .small(),
                                        );
                                        ui.label(format!(
                                            "{} (0x{hid:02X})",
                                            hid_display_name(*hid)
                                        ));
                                    }
                                    MacroEventKind::KeyUp { hid } => {
                                        ui.label(
                                            RichText::new("UP")
                                                .color(crate::theme::STATUS_OFFLINE)
                                                .small(),
                                        );
                                        ui.label(format!(
                                            "{} (0x{hid:02X})",
                                            hid_display_name(*hid)
                                        ));
                                    }
                                    MacroEventKind::Delay => {
                                        ui.label(
                                            RichText::new("DELAY")
                                                .color(crate::theme::SECONDARY_NEON)
                                                .small(),
                                        );
                                    }
                                    MacroEventKind::MoveRelative { dx, dy } => {
                                        ui.label("XY");
                                        ui.add(
                                            egui::DragValue::new(dx)
                                                .prefix("x:")
                                                .range(-5000..=5000),
                                        );
                                        ui.add(
                                            egui::DragValue::new(dy)
                                                .prefix("y:")
                                                .range(-5000..=5000),
                                        );
                                    }
                                    MacroEventKind::DpiSet { dpi_code } => {
                                        ui.label("DPI Set");
                                        ui.add(egui::DragValue::new(dpi_code).range(0..=16));
                                    }
                                    MacroEventKind::DpiRestore { dpi_code } => {
                                        ui.label("DPI Restore");
                                        ui.add(egui::DragValue::new(dpi_code).range(0..=16));
                                    }
                                    MacroEventKind::LedAnim { channel, index } => {
                                        ui.label(format!("LED ch={channel} idx={index}"));
                                    }
                                    MacroEventKind::Raw {
                                        event_type, hid, ..
                                    } => {
                                        ui.label(format!("Raw type={event_type} hid=0x{hid:02X}"));
                                    }
                                });

                                ui.add(
                                    egui::DragValue::new(&mut ev.delay_ms)
                                        .range(1..=10_000)
                                        .suffix(" ms"),
                                );

                                ui.horizontal(|ui| {
                                    if ui
                                        .add_enabled(idx > 0, egui::Button::new("^").small())
                                        .clicked()
                                    {
                                        swap_up = Some(idx);
                                    }
                                    if ui
                                        .add_enabled(
                                            idx + 1 < step_len,
                                            egui::Button::new("v").small(),
                                        )
                                        .clicked()
                                    {
                                        swap_down = Some(idx);
                                    }
                                    if ui.small_button("X").clicked() {
                                        delete_idx = Some(idx);
                                    }
                                });

                                ui.end_row();
                            }
                        });
                }
            });
    });

    if let Some(i) = swap_up {
        mac.events.swap(i - 1, i);
    } else if let Some(i) = swap_down {
        mac.events.swap(i, i + 1);
    } else if let Some(i) = delete_idx {
        mac.events.remove(i);
    }

    // Footer bytecode summary
    ui.add_space(4.0);
    let summary = match compile_macro_bytecode(mac, polling_ms, 0x0400) {
        Ok((bytes, steps)) => format!(
            "{} source events -> {} EEPROM steps, {} bytes @ {} ms tick",
            mac.events.len(),
            steps,
            bytes.len(),
            polling_ms
        ),
        Err(e) => format!("Compile warning: {e}"),
    };
    ui.label(RichText::new(summary).small().weak());
}

fn render_right_pane(ui: &mut egui::Ui, cfg: &mut DeviceConfig, state: &mut MacroTabState) {
    let is_recording = state.recorder.is_recording();

    ui.heading("Repeat Mode");
    let Some(mac) = cfg
        .macros
        .groups
        .get_mut(state.selected_group)
        .and_then(|g| g.macros.get_mut(state.selected_macro))
    else {
        ui.label(RichText::new("No macro selected.").weak());
        return;
    };

    if let RepeatMode::Count(n) = mac.repeat_mode {
        state.repeat_count_buf = n.max(1);
    }

    ui.add_enabled_ui(!is_recording, |ui| {
        crate::theme::cad_frame().show(ui, |ui| {
            if ui
                .radio(
                    matches!(mac.repeat_mode, RepeatMode::UntilRelease),
                    "Run Until Release",
                )
                .clicked()
            {
                mac.repeat_mode = RepeatMode::UntilRelease;
            }
            if ui
                .radio(
                    matches!(mac.repeat_mode, RepeatMode::UntilClick),
                    "Run Until Click",
                )
                .clicked()
            {
                mac.repeat_mode = RepeatMode::UntilClick;
            }

            let is_count = matches!(mac.repeat_mode, RepeatMode::Count(_));
            if ui.radio(is_count, "Repeat On").clicked() {
                mac.repeat_mode = RepeatMode::Count(state.repeat_count_buf.max(1));
            }
            if let RepeatMode::Count(ref mut count) = mac.repeat_mode {
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    ui.label("Times:");
                    if ui
                        .add(egui::DragValue::new(count).range(1..=65535))
                        .changed()
                    {
                        state.repeat_count_buf = *count;
                    }
                });
            }

            if ui
                .radio(matches!(mac.repeat_mode, RepeatMode::Once), "Run Once")
                .clicked()
            {
                mac.repeat_mode = RepeatMode::Once;
            }

            ui.separator();
            ui.checkbox(&mut mac.finish_cycle, "Finish cycle on stop")
                .on_hover_text("Sets flags bit 3 (0x08) for Until Release / Until Click loops.");
        });
    });
}

fn insert_key_or_tap(events: &mut Vec<MacroEvent>, hid: u8, tap: bool) {
    if tap {
        if events.len() + 2 <= MAX_MACRO_EVENTS {
            events.push(MacroEvent {
                kind: MacroEventKind::KeyDown { hid },
                delay_ms: DEFAULT_TRAILING_DELAY_MS,
            });
            events.push(MacroEvent {
                kind: MacroEventKind::KeyUp { hid },
                delay_ms: DEFAULT_TRAILING_DELAY_MS,
            });
        }
    } else if events.len() < MAX_MACRO_EVENTS {
        events.push(MacroEvent {
            kind: MacroEventKind::KeyDown { hid },
            delay_ms: DEFAULT_TRAILING_DELAY_MS,
        });
    }
}

fn describe_event_kind(kind: &MacroEventKind) -> String {
    match *kind {
        MacroEventKind::KeyDown { hid } => format!("DOWN {} (0x{hid:02X})", hid_display_name(hid)),
        MacroEventKind::KeyUp { hid } => format!("UP   {} (0x{hid:02X})", hid_display_name(hid)),
        MacroEventKind::Delay => "DELAY".to_string(),
        MacroEventKind::MoveRelative { dx, dy } => format!("XY ({dx}, {dy})"),
        MacroEventKind::DpiSet { dpi_code } => format!("DPI Set #{dpi_code}"),
        MacroEventKind::DpiRestore { dpi_code } => format!("DPI Restore #{dpi_code}"),
        MacroEventKind::LedAnim { channel, index } => format!("LED ch={channel} idx={index}"),
        MacroEventKind::Raw {
            event_type, hid, ..
        } => format!("Raw({event_type}, 0x{hid:02X})"),
    }
}

fn next_unique_group_name(existing: &[MacroGroup]) -> String {
    let mut n = existing.len() + 1;
    loop {
        let candidate = format!("Group{n}");
        if !existing
            .iter()
            .any(|g| g.name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
        n += 1;
    }
}

fn adjust_bindings_after_group_delete(cfg: &mut DeviceConfig, removed_group: u8) {
    for prof in &mut cfg.profiles {
        for key in &mut prof.keys {
            if let KeyAction::UserMacro { group, index } = *key {
                if group == removed_group {
                    *key = KeyAction::Disabled;
                } else if group > removed_group {
                    *key = KeyAction::UserMacro {
                        group: group - 1,
                        index,
                    };
                }
            }
        }
    }
}

fn adjust_bindings_after_macro_delete(cfg: &mut DeviceConfig, target_group: u8, removed_macro: u8) {
    for prof in &mut cfg.profiles {
        for key in &mut prof.keys {
            if let KeyAction::UserMacro { group, index } = *key
                && group == target_group
            {
                if index == removed_macro {
                    *key = KeyAction::Disabled;
                } else if index > removed_macro {
                    *key = KeyAction::UserMacro {
                        group,
                        index: index - 1,
                    };
                }
            }
        }
    }
}
