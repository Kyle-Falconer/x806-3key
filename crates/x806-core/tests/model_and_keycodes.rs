use x806_core::{defaults, keycode, model::*};

#[test]
fn vk_hid_roundtrip_and_quirks() {
    assert_eq!(keycode::vk_to_hid(0x41), 0x04); // 'A'
    assert_eq!(keycode::vk_to_hid(0x31), 0x1E); // '1'
    assert_eq!(keycode::vk_to_hid(0x30), 0x27); // '0'
    assert_eq!(keycode::vk_to_hid(0xA2), 0xE0); // LCtrl
    assert_eq!(keycode::vk_to_hid(0xC0), 0x35); // `~ fixed from X806.exe single-key 0x32 bug
    assert_eq!(keycode::vk_to_hid(0xF0), 0xF1); // LMB
    assert_eq!(keycode::vk_to_hid(0xF1), 0xF3); // RMB
    assert_eq!(keycode::vk_to_hid(0xF2), 0xF2); // MMB
    for hid in [
        0x04, 0x1D, 0x1E, 0x27, 0x28, 0x35, 0x3A, 0x45, 0x52, 0x62, 0xE0, 0xE7, 0xF1, 0xF2, 0xF3,
    ] {
        assert_eq!(
            keycode::vk_to_hid(keycode::hid_to_vk(hid)),
            hid,
            "roundtrip failed for 0x{hid:02x}"
        );
    }
    assert_eq!(keycode::hid_display_name(0xE0), "LCONTROL");
    assert_eq!(keycode::hid_display_name(0x04), "A");
    assert_eq!(keycode::hid_display_name(0xF1), "L-KEY");
}

#[test]
fn rgb4_active_low_conversion() {
    let red = Rgb4 { r: 0, g: 15, b: 15 };
    assert_eq!(red.to_rgb8(), [255, 0, 0]);
    assert_eq!(Rgb4::from_rgb8(255, 0, 0), red);
    assert_eq!(Rgb4::from_rgb8(0, 0, 0), Rgb4 { r: 15, g: 15, b: 15 });
}

#[test]
fn factory_defaults_match_config_reset() {
    let cfg = defaults::factory_default_config();
    assert_eq!(cfg.active_profile, 0);
    assert_eq!(cfg.polling_rate, PollingRate::Hz500);
    assert_eq!(cfg.fire_time_ms, 50);
    assert_eq!(cfg.led.mode, LedMode::CycleBreath);
    for p in [0usize, 2, 3] {
        for k in 0..8usize {
            assert_eq!(
                cfg.profiles[p].keys[k],
                KeyAction::SingleKey { hid: 0x1E + k as u8 }
            );
        }
    }
    assert_eq!(defaults::builtin_shortcuts().len(), 8);
    assert_eq!(defaults::builtin_shortcuts()[0].name, "Copy");
}

#[test]
fn polling_rate_and_media_key_tables() {
    let expected_rates = [
        (PollingRate::Hz125, 7u8, 8u16, 0u8),
        (PollingRate::Hz250, 3u8, 4u16, 1u8),
        (PollingRate::Hz500, 1u8, 2u16, 2u8),
        (PollingRate::Hz1000, 0u8, 1u16, 3u8),
    ];
    for (rate, div, tick, ini) in expected_rates {
        assert_eq!(rate.eeprom_divider(), div);
        assert_eq!(rate.tick_ms(), tick);
        assert_eq!(rate.to_ini_code(), ini);
        assert_eq!(PollingRate::from_ini_code(ini), rate);
    }

    let expected_media = [
        (MediaKey::Browser, 0x0223u16, 23u32),
        (MediaKey::Player, 0x0183u16, 24u32),
        (MediaKey::Mail, 0x018Au16, 25u32),
        (MediaKey::VolUp, 0x00E9u16, 26u32),
        (MediaKey::VolDown, 0x00EAu16, 27u32),
        (MediaKey::Previous, 0x00B6u16, 28u32),
        (MediaKey::Next, 0x00B5u16, 29u32),
        (MediaKey::Mute, 0x00E2u16, 30u32),
        (MediaKey::PlayPause, 0x00CDu16, 31u32),
        (MediaKey::Calculator, 0x0192u16, 32u32),
    ];
    for (key, usage, ini_id) in expected_media {
        assert_eq!(key.consumer_usage(), usage);
        assert_eq!(key.ini_action_id(), ini_id);
    }
}
