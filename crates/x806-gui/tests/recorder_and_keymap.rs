use std::fs;

use x806_core::defaults;
use x806_core::model::{KeyAction, MacroEventKind};
use x806_gui::app::validate_and_save_offline;
use x806_gui::keymap::egui_key_to_hid;
use x806_gui::recorder::{MacroRecorder, ModState, RecordOutcome};
use x806_gui::tabs::base::BaseTabState;
use x806_gui::tabs::macro_tab::{MacroTabState, clamp_macro_name_utf16};

#[test]
fn recorder_deduplicates_held_repeat_and_tracks_delays() {
    let mut rec = MacroRecorder::new();
    rec.start(0);
    assert!(rec.is_recording());

    assert_eq!(rec.on_key_event(0xE0, true, 100), RecordOutcome::Recorded);
    assert_eq!(
        rec.on_key_event(0xE0, true, 130),
        RecordOutcome::IgnoredRepeat
    );
    assert_eq!(rec.on_key_event(0x21, true, 150), RecordOutcome::Recorded);
    assert_eq!(rec.on_key_event(0x21, false, 190), RecordOutcome::Recorded);
    assert_eq!(rec.on_key_event(0xE0, false, 210), RecordOutcome::Recorded);

    let events = rec.stop();
    assert!(!rec.is_recording());
    assert_eq!(events.len(), 4);

    assert_eq!(events[0].kind, MacroEventKind::KeyDown { hid: 0xE0 });
    assert_eq!(events[1].kind, MacroEventKind::KeyDown { hid: 0x21 });
    assert_eq!(events[2].kind, MacroEventKind::KeyUp { hid: 0x21 });
    assert_eq!(events[3].kind, MacroEventKind::KeyUp { hid: 0xE0 });

    let delays: Vec<u16> = events.iter().map(|e| e.delay_ms).collect();
    assert_eq!(delays, vec![50, 40, 20, 20]);
}

#[test]
fn recorder_ignores_orphan_keyup() {
    let mut rec = MacroRecorder::new();
    rec.start(0);

    assert_eq!(
        rec.on_key_event(0x04, false, 100),
        RecordOutcome::IgnoredOrphanUp
    );
    assert!(rec.events().is_empty());
}

#[test]
fn recorder_caps_at_110_events() {
    let mut rec = MacroRecorder::new();
    rec.start(0);

    let mut reached_limit = false;
    let mut t = 10u64;
    for _ in 0..120 {
        let down = rec.on_key_event(0x04, true, t);
        t += 10;
        let up = rec.on_key_event(0x04, false, t);
        t += 10;
        if down == RecordOutcome::LimitReached || up == RecordOutcome::LimitReached {
            reached_limit = true;
        }
    }

    assert!(reached_limit);
    assert_eq!(rec.events().len(), 110);
}

#[test]
fn recorder_modifier_delta_emits_lctrl_down_up() {
    let mut rec = MacroRecorder::new();
    rec.start(0);

    let down_outcomes = rec.on_modifiers_changed(
        ModState {
            ctrl: true,
            ..ModState::default()
        },
        100,
    );
    assert_eq!(down_outcomes, vec![RecordOutcome::Recorded]);

    let up_outcomes = rec.on_modifiers_changed(ModState::default(), 145);
    assert_eq!(up_outcomes, vec![RecordOutcome::Recorded]);

    let events = rec.stop();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].kind, MacroEventKind::KeyDown { hid: 0xE0 });
    assert_eq!(events[0].delay_ms, 45);
    assert_eq!(events[1].kind, MacroEventKind::KeyUp { hid: 0xE0 });
    assert_eq!(events[1].delay_ms, 20);
}

#[test]
fn egui_key_mapping_covers_letters_digits_fkeys_and_arrows() {
    assert_eq!(egui_key_to_hid(egui::Key::A), Some(0x04));
    assert_eq!(egui_key_to_hid(egui::Key::Z), Some(0x1D));
    assert_eq!(egui_key_to_hid(egui::Key::Num1), Some(0x1E));
    assert_eq!(egui_key_to_hid(egui::Key::Num0), Some(0x27));
    assert_eq!(egui_key_to_hid(egui::Key::F1), Some(0x3A));
    assert_eq!(egui_key_to_hid(egui::Key::F12), Some(0x45));
    assert_eq!(egui_key_to_hid(egui::Key::Backtick), Some(0x35));
    assert_eq!(egui_key_to_hid(egui::Key::Enter), Some(0x28));
    assert_eq!(egui_key_to_hid(egui::Key::ArrowUp), Some(0x52));
    assert_eq!(egui_key_to_hid(egui::Key::ArrowDown), Some(0x51));
    assert_eq!(egui_key_to_hid(egui::Key::ArrowLeft), Some(0x50));
    assert_eq!(egui_key_to_hid(egui::Key::ArrowRight), Some(0x4F));
}

#[test]
fn macro_name_clamps_to_30_utf16_code_units() {
    let long_ascii = "a".repeat(45);
    let clamped = clamp_macro_name_utf16(&long_ascii);
    assert_eq!(clamped.encode_utf16().count(), 30);

    // 29 ASCII chars + 1 emoji (2 UTF-16 units) must stop at 29 units rather than exceeding 30.
    let mut boundary = "x".repeat(29);
    boundary.push('😀');
    let clamped_boundary = clamp_macro_name_utf16(&boundary);
    assert_eq!(clamped_boundary, "x".repeat(29));
    assert!(clamped_boundary.encode_utf16().count() <= 30);
}

#[test]
fn record_key_modal_skips_opening_frame_and_captures_next_frame() {
    let mut state = BaseTabState::default();
    state.open_record_modal();
    assert!(state.record_modal_open);

    // Same frame that activated "Record Key..." via Enter/Space must NOT be captured.
    assert_eq!(state.poll_record_modal_capture(Some(0x28)), None);
    assert!(state.record_modal_open);

    // Subsequent frame with no key pressed keeps modal open.
    assert_eq!(state.poll_record_modal_capture(None), None);
    assert!(state.record_modal_open);

    // Next real keypress captures HID and closes modal.
    assert_eq!(state.poll_record_modal_capture(Some(0x04)), Some(0x04));
    assert!(!state.record_modal_open);
}

#[test]
fn macro_tab_auto_cancels_recording_when_config_loses_selected_macro() {
    let mut state = MacroTabState::default();
    state.recorder.start(0);
    assert!(state.recorder.is_recording());

    // Factory defaults have 0 macro groups (e.g. after Reset/Cancel/Import).
    let empty_cfg = defaults::factory_default_config();
    state.sync_with_config(&empty_cfg);
    assert!(!state.recorder.is_recording());
}

#[test]
fn offline_apply_validates_before_writing_and_handles_missing_config_path() {
    let tmp_dir = std::env::temp_dir().join(format!(
        "x806_gui_offline_apply_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&tmp_dir);
    let cfg_path = tmp_dir.join("config.toml");

    // 1. Invalid config (dangling UserMacro reference) must NOT write config.toml.
    let mut invalid_cfg = defaults::factory_default_config();
    invalid_cfg.profiles[0].keys[0] = KeyAction::UserMacro { group: 9, index: 0 };
    let err = validate_and_save_offline(&invalid_cfg, Some(&cfg_path))
        .expect_err("invalid macro reference must fail validation");
    assert!(err.contains("validation failed"));
    assert!(
        !cfg_path.exists(),
        "invalid config must never be written to disk"
    );

    // 2. Valid config with None path validates cleanly and explicitly reports no file saved.
    let valid_cfg = defaults::factory_default_config();
    let (plan, saved_path) =
        validate_and_save_offline(&valid_cfg, None).expect("valid config should compile");
    assert!(!plan.writes.is_empty());
    assert!(saved_path.is_none());

    // 3. Valid config with Some(path) validates and writes config.toml.
    let (_plan, saved_path) =
        validate_and_save_offline(&valid_cfg, Some(&cfg_path)).expect("valid config should save");
    assert_eq!(saved_path.as_deref(), Some(cfg_path.as_path()));
    assert!(cfg_path.exists());

    let _ = fs::remove_dir_all(&tmp_dir);
}
