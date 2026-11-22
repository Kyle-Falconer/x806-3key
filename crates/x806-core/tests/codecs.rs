use std::path::Path;
use x806_core::codec::{dat, ini, toml as toml_codec};
use x806_core::{defaults, model::*, X806Error};

#[test]
fn dat_roundtrip_plain_and_isd_base64() {
    let zoom_it = Macro {
        name: "ZoomIt".to_string(),
        repeat_mode: RepeatMode::Once,
        finish_cycle: false,
        opt_flag: 0,
        opt_val: 10,
        events: vec![
            MacroEvent {
                kind: MacroEventKind::KeyDown { hid: 0xE0 }, // LCtrl
                delay_ms: 1,
            },
            MacroEvent {
                kind: MacroEventKind::KeyDown { hid: 0x21 }, // '4'
                delay_ms: 1,
            },
        ],
        led_blocks: Vec::new(),
    };

    let plain = dat::encode_dat(std::slice::from_ref(&zoom_it), false);
    assert!(plain.contains("DAT_VER=3"));
    let line0 = plain
        .lines()
        .find(|l| l.starts_with("0="))
        .expect("missing 0= line");
    assert_eq!(line0.len() - 2, 5583);

    let parsed_plain = dat::parse_dat(plain.as_bytes()).expect("parse plain dat");
    assert_eq!(parsed_plain, vec![zoom_it.clone()]);

    let b64 = dat::encode_dat(std::slice::from_ref(&zoom_it), true);
    assert!(b64.contains("ISD=1"));
    let parsed_b64 = dat::parse_dat(b64.as_bytes()).expect("parse ISD=1 base64 dat");
    assert_eq!(parsed_b64, vec![zoom_it]);

    // Name > 30 UTF-16 code units with an emoji surrogate pair right at boundary 29..31
    let mut long_name = "A".repeat(29);
    long_name.push('😀'); // 2 UTF-16 code units; must be dropped rather than split
    long_name.push('Z');
    let long_macro = Macro {
        name: long_name,
        repeat_mode: RepeatMode::Once,
        finish_cycle: false,
        opt_flag: 0,
        opt_val: 10,
        events: vec![],
        led_blocks: Vec::new(),
    };
    let encoded = dat::encode_dat(&[long_macro], false);
    let decoded = dat::parse_dat(encoded.as_bytes()).expect("parse truncated utf16 name");
    assert_eq!(decoded[0].name, "A".repeat(29));
    assert!(decoded[0].name.encode_utf16().count() <= 30);
}

#[test]
fn dat_parses_legacy_v2_and_oversized_padding() {
    // Construct a DAT_VER=2 (1855-byte) record
    let mut raw_v2 = vec![0u8; 1855];
    let name_units: Vec<u16> = "Legacy".encode_utf16().collect();
    for (i, u) in name_units.iter().enumerate() {
        raw_v2[i * 2..i * 2 + 2].copy_from_slice(&u.to_le_bytes());
    }
    raw_v2[0x3C] = 0x04; // RepeatMode::Count
    raw_v2[0x3D..0x3F].copy_from_slice(&3u16.to_le_bytes());
    raw_v2[0x3F..0x41].copy_from_slice(&1u16.to_le_bytes()); // step_count = 1
    // Event 0 at 0x4B for v != 3
    raw_v2[0x4B] = 1; // KeyDown
    raw_v2[0x4C] = 0x41; // VK 'A' -> HID 0x04
    raw_v2[0x51..0x53].copy_from_slice(&25u16.to_le_bytes());

    let digits_v2: String = raw_v2.iter().map(|b| format!("{b:03}")).collect();
    assert_eq!(digits_v2.len(), 5565);
    let dat_v2 = format!("[Init]\r\nDAT_VER=2\r\n[Macros]\r\nmacroNum=1\r\n0={digits_v2}\r\n");
    let parsed_v2 = dat::parse_dat(dat_v2.as_bytes()).expect("parse DAT_VER=2");
    assert_eq!(parsed_v2.len(), 1);
    assert_eq!(parsed_v2[0].name, "Legacy");
    assert_eq!(parsed_v2[0].repeat_mode, RepeatMode::Count(3));
    assert_eq!(
        parsed_v2[0].events,
        vec![MacroEvent {
            kind: MacroEventKind::KeyDown { hid: 0x04 },
            delay_ms: 25
        }]
    );

    // Construct a 9543-char zero-padded DAT_VER=3 record (matching PUBG_CN.dat)
    let m = Macro {
        name: "M4".to_string(),
        repeat_mode: RepeatMode::UntilClick,
        finish_cycle: false,
        opt_flag: 1,
        opt_val: 16,
        events: vec![MacroEvent {
            kind: MacroEventKind::MoveRelative { dx: 0, dy: 4 },
            delay_ms: 3,
        }],
        led_blocks: Vec::new(),
    };
    let encoded_v3 = dat::encode_dat(std::slice::from_ref(&m), false);
    let mut padded = String::new();
    for line in encoded_v3.lines() {
        if let Some(rest) = line.strip_prefix("0=") {
            padded.push_str("0=");
            padded.push_str(rest);
            padded.push_str(&"0".repeat(9543 - rest.len()));
            padded.push_str("\r\n");
        } else {
            padded.push_str(line);
            padded.push_str("\r\n");
        }
    }
    let parsed_padded = dat::parse_dat(padded.as_bytes()).expect("parse 9543-char padded record");
    assert_eq!(parsed_padded, vec![m]);
}

#[test]
fn dat_rejects_corrupt_base64_and_truncation() {
    let bad_b64 = "[Init]\nDAT_VER=3\nISD=1\n[Macros]\nmacroNum=1\n0=!!!not-base64!!!\n";
    let err = dat::parse_dat(bad_b64.as_bytes()).unwrap_err();
    assert!(matches!(err, X806Error::DatParse(_)), "got {err:?}");

    let truncated = "[Init]\nDAT_VER=3\n[Macros]\nmacroNum=1\n0=065000075\n";
    let err2 = dat::parse_dat(truncated.as_bytes()).unwrap_err();
    assert!(matches!(err2, X806Error::DatParse(_)), "got {err2:?}");
}

#[test]
fn ini_parses_comments_mixed_crlf_and_sharedata_precedence() {
    let raw_ini = concat!(
        "[Init]\r",
        "VER_SEL=1\n",
        "CurConfig=0\r\n",
        "[KEYMAP]//inline section comment\r\n",
        "K1_MAP=7//S0_R0\r\n",
        "K2_MAP=3//S0_R1\n",
        "[Config0]\r\n",
        "K0=256\r\n",
        "K1=42,31\r\n",
        "USB_SPEED=0\r\n",
        "LEFT_MAC0=0\r\n\nLEFT_MAC1=0\r\n",
        "[ShareData]\r\n",
        "USB_SPEED=2\r\n",
    );

    let cfg = ini::parse_config_ini(raw_ini.as_bytes()).expect("parse ini");
    assert_eq!(cfg.keymap[0], 7);
    assert_eq!(cfg.keymap[1], 3);
    assert_eq!(cfg.polling_rate, PollingRate::Hz500);
    assert_eq!(
        cfg.profiles[0].keys[0],
        KeyAction::UserMacro { group: 0, index: 0 }
    );
    assert_eq!(cfg.profiles[0].keys[1], KeyAction::SingleKey { hid: 31 });

    // Full encode_config_ini -> parse_config_ini roundtrip
    let encoded = ini::encode_config_ini(&cfg);
    let reparsed = ini::parse_config_ini(encoded.as_bytes()).expect("reparse encoded ini");
    assert_eq!(reparsed, cfg);
}

#[test]
fn macro_grp_utf16le_bom_roundtrip() {
    let bytes = ini::encode_macro_groups(&["Streaming".to_string()]);
    assert!(bytes.starts_with(&[0xFF, 0xFE]));
    let groups = ini::parse_macro_groups(&bytes).expect("parse macroGrp");
    assert_eq!(groups, vec!["Streaming".to_string()]);
}

#[test]
fn toml_roundtrip_full_config() {
    let mut cfg = defaults::factory_default_config();
    cfg.macros.groups.push(MacroGroup {
        name: "Streaming".to_string(),
        macros: vec![Macro {
            name: "ZoomIt".to_string(),
            repeat_mode: RepeatMode::Once,
            finish_cycle: false,
            opt_flag: 0,
            opt_val: 10,
            events: vec![MacroEvent {
                kind: MacroEventKind::KeyDown { hid: 0xE0 },
                delay_ms: 1,
            }],
            led_blocks: Vec::new(),
        }],
    });
    cfg.profiles[0].keys[0] = KeyAction::UserMacro { group: 0, index: 0 };

    let serialized = toml_codec::to_toml_string(&cfg).expect("to_toml_string");
    let deserialized = toml_codec::from_toml_str(&serialized).expect("from_toml_str");
    assert_eq!(deserialized, cfg);
}

#[test]
fn shipped_k806_files_if_present() {
    let root = Path::new(r"C:\Program Files (x86)\K806");
    if !root.exists() {
        return;
    }

    let Ok(const_bytes) = std::fs::read(root.join("constMacro.dat")) else {
        return;
    };
    let const_macros = dat::parse_dat(&const_bytes).expect("parse constMacro.dat");
    assert_eq!(const_macros.len(), 8);

    let streaming_bytes =
        std::fs::read(root.join(r"skins\INI_EN\macro\Streaming.dat")).expect("read Streaming.dat");
    let streaming = dat::parse_dat(&streaming_bytes).expect("parse Streaming.dat");
    assert_eq!(streaming.len(), 1);
    assert_eq!(streaming[0].name, "ZoomIt");

    let pubg_en_bytes =
        std::fs::read(root.join(r"skins\config\PUBG_EN.dat")).expect("read PUBG_EN.dat");
    let pubg_en = dat::parse_dat(&pubg_en_bytes).expect("parse PUBG_EN.dat");
    assert_eq!(pubg_en.len(), 11);
    assert_eq!(pubg_en[0].name, "AKM");
    assert_eq!(pubg_en[0].opt_flag, 1);
    assert_eq!(pubg_en[0].opt_val, 2);
    assert!(pubg_en[0].events.iter().any(|ev| matches!(
        ev.kind,
        MacroEventKind::MoveRelative { dx: 0, dy: 2 }
    )));

    let pubg_cn_bytes =
        std::fs::read(root.join(r"skins\config\PUBG_CN.dat")).expect("read PUBG_CN.dat");
    let pubg_cn = dat::parse_dat(&pubg_cn_bytes).expect("parse PUBG_CN.dat");
    assert_eq!(pubg_cn.len(), 11);

    let grp_bytes =
        std::fs::read(root.join(r"skins\INI_EN\macro\macroGrp.ini")).expect("read macroGrp.ini");
    let groups = ini::parse_macro_groups(&grp_bytes).expect("parse macroGrp.ini");
    assert_eq!(groups, vec!["Streaming".to_string()]);

    let cfg_bytes = std::fs::read(root.join(r"skins\config\config.ini")).expect("read config.ini");
    let cfg = ini::parse_config_ini(&cfg_bytes).expect("parse config.ini");
    assert_eq!(cfg.keymap[0], 7);
    assert_eq!(cfg.polling_rate, PollingRate::Hz500);

    let bundle = ini::import_x806_bundle(&root.join(r"skins\config\config.ini"), None, None, None)
        .expect("import_x806_bundle");
    assert_eq!(bundle.macros.groups.len(), 1);
    assert_eq!(bundle.macros.groups[0].name, "Streaming");
    assert_eq!(bundle.macros.pubg_macros.len(), 11);
    assert_eq!(bundle.macros.builtin_shortcuts.len(), 8);
}

#[test]
fn dat_all_event_kinds_and_utf16le_bom_roundtrip() {
    let rich = Macro {
        name: "Rich".to_string(),
        repeat_mode: RepeatMode::UntilRelease,
        finish_cycle: true,
        opt_flag: 1,
        opt_val: 7,
        events: vec![
            MacroEvent {
                kind: MacroEventKind::KeyDown { hid: 0x35 }, // `~
                delay_ms: 10,
            },
            MacroEvent {
                kind: MacroEventKind::KeyUp { hid: 0x35 },
                delay_ms: 5,
            },
            MacroEvent {
                kind: MacroEventKind::Delay,
                delay_ms: 120,
            },
            MacroEvent {
                kind: MacroEventKind::MoveRelative { dx: -5, dy: -3 },
                delay_ms: 2,
            },
            MacroEvent {
                kind: MacroEventKind::MoveRelative { dx: -5, dy: 3 },
                delay_ms: 2,
            },
            MacroEvent {
                kind: MacroEventKind::MoveRelative { dx: 5, dy: -3 },
                delay_ms: 2,
            },
            MacroEvent {
                kind: MacroEventKind::MoveRelative { dx: 5, dy: 3 },
                delay_ms: 2,
            },
            MacroEvent {
                kind: MacroEventKind::DpiSet { dpi_code: 4 },
                delay_ms: 1,
            },
            MacroEvent {
                kind: MacroEventKind::DpiRestore { dpi_code: 1 },
                delay_ms: 1,
            },
            MacroEvent {
                kind: MacroEventKind::LedAnim {
                    channel: 2,
                    index: 3,
                },
                delay_ms: 1,
            },
        ],
        led_blocks: vec![vec![0xAA; 68]],
    };

    let encoded = dat::encode_dat(std::slice::from_ref(&rich), false);
    // Re-encode as UTF-16LE with BOM to exercise BOM auto-detection on .dat
    let mut utf16 = vec![0xFF, 0xFE];
    for u in encoded.encode_utf16() {
        utf16.extend_from_slice(&u.to_le_bytes());
    }
    let decoded = dat::parse_dat(&utf16).expect("parse utf16le bom dat");
    assert_eq!(decoded, vec![rich]);
}

#[test]
fn ini_bundle_export_import_and_legacy_actions_roundtrip() {
    let mut cfg = defaults::factory_default_config();
    cfg.profiles[0].keys[0] = KeyAction::RawLegacy(22);
    cfg.profiles[0].keys[1] = KeyAction::RawLegacy(41);
    cfg.profiles[0].keys[2] = KeyAction::Media(MediaKey::VolUp);
    cfg.profiles[0].keys[3] = KeyAction::Shortcut(BuiltinShortcut::Copy);
    cfg.profiles[0].keys[4] = KeyAction::PubgMacro { index: 0 };
    cfg.profiles[0].keys[5] = KeyAction::UserMacro { group: 0, index: 0 };
    cfg.macros.groups.push(MacroGroup {
        name: "Work".to_string(),
        macros: vec![Macro {
            name: "Hello".to_string(),
            repeat_mode: RepeatMode::Once,
            finish_cycle: false,
            opt_flag: 0,
            opt_val: 10,
            events: vec![MacroEvent {
                kind: MacroEventKind::KeyDown { hid: 0x04 },
                delay_ms: 5,
            }],
            led_blocks: Vec::new(),
        }],
    });
    cfg.macros.pubg_macros.push(Macro {
        name: "AKM".to_string(),
        repeat_mode: RepeatMode::UntilClick,
        finish_cycle: false,
        opt_flag: 1,
        opt_val: 2,
        events: vec![],
        led_blocks: Vec::new(),
    });

    let dir = std::env::temp_dir().join("x806_task2_bundle_test");
    let _ = std::fs::remove_dir_all(&dir);
    let ini_path = dir.join("config.ini");
    let macros_dir = dir.join("macro");

    ini::export_x806_bundle(&cfg, &ini_path, &macros_dir).expect("export bundle");
    let imported = ini::import_x806_bundle(&ini_path, None, None, None).expect("import bundle");
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(imported, cfg);
}
