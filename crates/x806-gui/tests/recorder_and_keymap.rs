use x806_core::model::MacroEventKind;
use x806_gui::keymap::egui_key_to_hid;
use x806_gui::recorder::{MacroRecorder, ModState, RecordOutcome};
use x806_gui::tabs::macro_tab::clamp_macro_name_utf16;

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
