use std::collections::BTreeMap;

use x806_core::X806Error;
use x806_core::defaults::factory_default_config;
use x806_core::eeprom::{CompileContext, EepromPlan, compile_macro_bytecode, compile_plan};
use x806_core::model::*;

/// Flattens an `EepromPlan` into an `addr -> byte` map while asserting block invariants.
fn flatten_plan(plan: &EepromPlan) -> BTreeMap<u16, u8> {
    let mut mem = BTreeMap::new();
    for w in &plan.writes {
        assert!(
            (1..=8).contains(&w.data.len()),
            "write at {:#06x} has invalid length {}",
            w.addr,
            w.data.len()
        );
        for (offset, &byte) in w.data.iter().enumerate() {
            let addr = w.addr + offset as u16;
            mem.insert(addr, byte);
        }
    }
    mem
}

fn read_slice(mem: &BTreeMap<u16, u8>, start: u16, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| {
            let addr = start + i as u16;
            *mem.get(&addr)
                .unwrap_or_else(|| panic!("missing EEPROM byte at {addr:#06x}"))
        })
        .collect()
}

fn simple_macro(name: &str, mode: RepeatMode, events: Vec<MacroEvent>) -> Macro {
    Macro {
        name: name.to_string(),
        repeat_mode: mode,
        finish_cycle: false,
        opt_flag: 0,
        opt_val: 10,
        events,
        led_blocks: Vec::new(),
    }
}

#[test]
fn never_writes_protected_factory_bytes() {
    let cfg = factory_default_config();
    let plan = compile_plan(&cfg, CompileContext::default()).expect("default config compiles");

    assert!(!plan.writes.is_empty());
    assert_eq!(plan.writes[0].addr, 0x0001);
    assert_eq!(plan.writes[0].data.len(), 1);
    assert_eq!(plan.writes[0].data[0], 0x03);

    for w in &plan.writes {
        assert!((1..=8).contains(&w.data.len()));
        for offset in 0..w.data.len() as u16 {
            let addr = w.addr + offset;
            assert_ne!(addr, 0x0000, "must never write factory magic 0x0000");
            assert!(
                !(0x0002..=0x0007).contains(&addr),
                "must never write factory VID/PID bytes at {addr:#06x}"
            );
        }
    }
}

#[test]
fn header_offsets_profiles_0_2_3_and_0x76_rmw() {
    let mut cfg = factory_default_config();
    cfg.active_profile = 2;
    cfg.polling_rate = PollingRate::Hz1000;
    cfg.fire_time_ms = 64; // 64 / 8 = 8
    cfg.dpi_level = 3;
    cfg.dpi_stages = [0, 1, 14, 15, 16, 2]; // reg: [0x01, 0x02, 0x10, 0x11, 0x12, 0x03]
    cfg.dpi_enable = [true, false, true, true, false, false];
    cfg.led.brightness = 5;
    cfg.led.symmetry = true;
    cfg.led.direction = 2;
    cfg.led.user_color_en = true;
    cfg.led.cycle_mask = 0b0000_1011; // c0=1, c1=1, c2=0, c3=1

    // Clear all keys first so only explicitly bound keys are populated
    for p in &mut cfg.profiles {
        p.keys = [KeyAction::Disabled; 8];
    }

    // Profile 0 (`Game I`): Key 1 (KEYMAP[0]=7 -> 0x000E), Key 5 (KEYMAP[4]=9 -> 0x0010)
    cfg.profiles[0].keys[0] = KeyAction::LeftButton; // hw_key_code = 0x01
    cfg.profiles[0].led_react_en[0] = true;
    cfg.profiles[0].led_react_type[0] = ReactiveStyle::CometTail; // 2 -> (2<<6)|(1<<5)|1 = 0xA1
    cfg.profiles[0].keys[4] = KeyAction::Fire; // hw_key_code = 0x09

    // Profile 1 has non-disabled keys in config, but EEPROM 0x0044..=0x004E MUST remain zeroed
    cfg.profiles[1].keys[0] = KeyAction::RightButton;

    // Profile 2 (`Game II`): Key 1 (hw_slot=7 -> 0x0056), Key 4 (hw_slot=1 -> 0x0050), Key 5 (hw_slot=9 -> 0x0058)
    cfg.profiles[2].keys[0] = KeyAction::DpiUp; // 0x0B
    cfg.profiles[2].keys[3] = KeyAction::ShowDesktop; // 0x07
    cfg.profiles[2].keys[4] = KeyAction::LedLoop; // 0x15

    // Profile 3 (`Game III`): Key 2 (hw_slot=3 -> 0x0062), Key 8 (hw_slot=4 -> 0x0063)
    cfg.profiles[3].keys[1] = KeyAction::DoubleClick; // 0x08
    cfg.profiles[3].keys[7] = KeyAction::FnKey; // 0x14

    let ctx = CompileContext {
        hw_flags: 0x0F,
        existing_0x76: 0xAC,
        macro_limit: 0x0FF8,
    };
    let plan = compile_plan(&cfg, ctx).expect("compile header");
    let mem = flatten_plan(&plan);

    // 0x0001 active profile & layout mode
    assert_eq!(mem[&0x0001], (2 << 5) | 0x03); // 0x43

    // Profile 0: 0x0008..=0x0010 (hw_slot 1..=9)
    assert_eq!(mem[&0x000E], (2 << 6) | (1 << 5) | 0x01); // Key 1 (hw_slot 7)
    assert_eq!(mem[&0x0010], 0x09); // Key 5 (hw_slot 9)
    assert_eq!(mem[&0x0008], 0x00); // Key 4 (hw_slot 1) disabled

    // DPI registers: [0x01, 0x02, 0x10, 0x11, 0x12, 0x03]
    // bit 4 set on stages 2, 3, 4 -> (1<<4)|(1<<3)|(1<<2) = 0x1C
    assert_eq!(mem[&0x0013], 0x1C);
    assert_eq!(mem[&0x0014], 0x10);
    assert_eq!(mem[&0x0015], 0x12);
    assert_eq!(mem[&0x0016], 0x01);
    assert_eq!(mem[&0x0017], 0x23);
    assert_eq!(mem[&0x0018], 0x18 | 3);
    assert_eq!(mem[&0x0019], 0b0000_1101);
    assert_eq!(read_slice(&mem, 0x001A, 2), 8u16.to_le_bytes());

    // 0x001D: user_color_en=1 (0x80) | c3<<6 (0x40) | c2<<4 (0) | c1<<2 (0x04) | c0 (0x01) = 0xC5
    assert_eq!(mem[&0x001D], 0xC5);
    assert_eq!(mem[&0x001E], 0x00); // 1000 Hz divider

    // 0x0020..=0x0031 active-low DPI RGB (stage 0 is Red = [0, 15, 15])
    assert_eq!(read_slice(&mem, 0x0020, 3), vec![0, 15, 15]);
    // 0x0032..=0x0033 extended layout marker
    assert_eq!(read_slice(&mem, 0x0032, 2), vec![0x07, 0x30]);

    // 0x003A symmetry(1<<6) | dir(2<<4) | hw_flags(0x0F) = 0x6F
    assert_eq!(mem[&0x003A] & 0x0F, 0x0F);
    assert_eq!(mem[&0x003A], (1 << 6) | (2 << 4) | 0x0F);
    // 0x003F brightness
    assert_eq!(mem[&0x003F], 5 << 4);
    // 0x0043 constant 0xFC
    assert_eq!(mem[&0x0043], 0xFC);
    // 0x0044..=0x004E Profile 1 always zeroed
    assert_eq!(read_slice(&mem, 0x0044, 11), vec![0u8; 11]);

    // Profile 2: 0x0050..=0x005A (hw_slot 1..=11)
    assert_eq!(mem[&0x0050], 0x07); // hw_slot 1
    assert_eq!(mem[&0x0056], 0x0B); // hw_slot 7
    assert_eq!(mem[&0x0058], 0x15); // hw_slot 9

    // Profile 3: 0x0060..=0x006A (hw_slot 1..=11)
    assert_eq!(mem[&0x0062], 0x08); // hw_slot 3
    assert_eq!(mem[&0x0063], 0x14); // hw_slot 4

    // 0x0076 RMW preserving bits 7:2
    assert_eq!(mem[&0x0076], (0xAC & 0xFC) | 2); // 0xAE
}

#[test]
fn descriptor_slot_indexing_single_key_media_and_dedup() {
    let mut cfg = factory_default_config();
    cfg.polling_rate = PollingRate::Hz500; // divider = 1, tick_ms = 2
    for p in &mut cfg.profiles {
        p.keys = [KeyAction::Disabled; 8];
    }

    let zoomit = simple_macro(
        "ZoomIt",
        RepeatMode::Once,
        vec![
            MacroEvent {
                kind: MacroEventKind::KeyDown { hid: 0xE0 },
                delay_ms: 2,
            },
            MacroEvent {
                kind: MacroEventKind::KeyDown { hid: 0x21 },
                delay_ms: 2,
            },
        ],
    );
    cfg.macros.groups.push(MacroGroup {
        name: "Streaming".into(),
        macros: vec![zoomit],
    });

    // Key 1 (ui=0, hw_slot=7, slot_idx=6) -> UserMacro(0, 0)
    cfg.profiles[0].keys[0] = KeyAction::UserMacro { group: 0, index: 0 };
    // Key 2 (ui=1, hw_slot=3, slot_idx=2) -> SAME UserMacro(0, 0) -> must deduplicate to 0x0400!
    cfg.profiles[0].keys[1] = KeyAction::UserMacro { group: 0, index: 0 };
    // Key 3 (ui=2, hw_slot=2, slot_idx=1) -> SingleKey { hid: 0x1F }
    cfg.profiles[0].keys[2] = KeyAction::SingleKey { hid: 0x1F };
    // Key 4 (ui=3, hw_slot=1, slot_idx=0) -> Media(VolUp)
    cfg.profiles[0].keys[3] = KeyAction::Media(MediaKey::VolUp);
    // Key 5 (ui=4, hw_slot=9, slot_idx=8) -> ANOTHER SingleKey { hid: 0x1F } -> MUST NOT dedup!
    cfg.profiles[0].keys[4] = KeyAction::SingleKey { hid: 0x1F };

    let plan = compile_plan(&cfg, CompileContext::default()).expect("compile descriptors");
    let mem = flatten_plan(&plan);

    // Header hw_key_codes
    assert_eq!(mem[&0x000E], 0x0D); // Key 1 user macro (RepeatMode::Once, left_mac=false)
    assert_eq!(mem[&0x000A], 0x0D); // Key 2 same user macro
    assert_eq!(mem[&0x0009], 0x0E); // Key 3 single key
    assert_eq!(mem[&0x0008], 0x0F); // Key 4 media key
    assert_eq!(mem[&0x0010], 0x0E); // Key 5 second single key

    // Key 1 descriptor at 0x00A0 + 6 * 10 = 0x00DC
    let d1 = read_slice(&mem, 0x00DC, 10);
    let addr1 = u16::from(d1[5]) | (u16::from(d1[6] & 0x3F) << 8);
    assert_eq!(d1[0], 1); // 500 Hz divider
    assert_eq!(d1[1], 2); // 2 steps, not toggle
    assert_eq!(d1[2], 0); // not single-key
    assert_eq!(addr1, 0x0400);
    assert_eq!(d1[8], 10); // opt_flag=0, opt_val=10

    // Key 2 descriptor at 0x00A0 + 2 * 10 = 0x00B4 -> shares macro_addr == 0x0400
    let d2 = read_slice(&mem, 0x00B4, 10);
    let addr2 = u16::from(d2[5]) | (u16::from(d2[6] & 0x3F) << 8);
    assert_eq!(addr2, 0x0400);
    assert_eq!(d2, d1);

    // Key 3 descriptor at 0x00A0 + 1 * 10 = 0x00AA -> SingleKey 0x1F at 0x0408
    let d3 = read_slice(&mem, 0x00AA, 10);
    let addr3 = u16::from(d3[5]) | (u16::from(d3[6] & 0x3F) << 8);
    assert_eq!(d3[0], 1);
    assert_eq!(d3[1], 0x01);
    assert_eq!(d3[2], 0x1F);
    assert_eq!(addr3, 0x0408);
    assert_eq!(
        read_slice(&mem, addr3, 8),
        vec![0x01, 0x00, 0x80, 0x1F, 0x00, 0x00, 0x00, 0x00]
    );

    // Key 4 descriptor at 0x00A0 + 0 * 10 = 0x00A0 -> Media(VolUp) = 0x00E9, advances ptr by 0
    let d4 = read_slice(&mem, 0x00A0, 10);
    assert_eq!(d4[0], 1);
    assert_eq!(d4[1], 0x00);
    assert_eq!(d4[2], 0x00);
    assert_eq!(&d4[3..5], &[0xE9, 0x00]);

    // Key 5 descriptor at 0x00A0 + 8 * 10 = 0x00F0 -> SingleKey 0x1F at 0x0410 (NOT deduplicated, and Media advanced by 0!)
    let d5 = read_slice(&mem, 0x00F0, 10);
    let addr5 = u16::from(d5[5]) | (u16::from(d5[6] & 0x3F) << 8);
    assert_eq!(addr5, 0x0410);
    assert_eq!(d5[2], 0x1F);
    assert_eq!(
        read_slice(&mem, addr5, 8),
        vec![0x01, 0x00, 0x80, 0x1F, 0x00, 0x00, 0x00, 0x00]
    );

    // Total macro bytes: 8 (ZoomIt) + 8 (Key 3) + 0 (Key 4 Media) + 8 (Key 5) = 24
    assert_eq!(plan.macro_bytes_used, 24);
}

#[test]
fn led_mute_vs_active_speed_clamping() {
    let mut cfg = factory_default_config();

    // CycleBreath with speed = 7 clamps to 6 -> 0x80 | (1 << 4) | 6 == 0x96
    cfg.led.mode = LedMode::CycleBreath;
    cfg.led.speed = 7;
    let mem = flatten_plan(&compile_plan(&cfg, CompileContext::default()).unwrap());
    assert_eq!(mem[&0x001C], 0x96);

    // Mute writes 0x80 | 7 == 0x87 regardless of speed field
    cfg.led.mode = LedMode::Mute;
    cfg.led.speed = 2;
    let mem = flatten_plan(&compile_plan(&cfg, CompileContext::default()).unwrap());
    assert_eq!(mem[&0x001C], 0x87);

    // ButtonResponse with speed = 3 writes 0x80 | (1 << 4) | (1 << 3) | 3 == 0x9B
    cfg.led.mode = LedMode::ButtonResponse;
    cfg.led.speed = 3;
    let mem = flatten_plan(&compile_plan(&cfg, CompileContext::default()).unwrap());
    assert_eq!(mem[&0x001C], 0x9B);
}

#[test]
fn delay_quantization_and_splitting_boundaries() {
    let make_single = |delay_ms: u16| {
        simple_macro(
            "DelayTest",
            RepeatMode::Once,
            vec![MacroEvent {
                kind: MacroEventKind::KeyDown { hid: 0x04 },
                delay_ms,
            }],
        )
    };

    // delay_ms = 0 at 1000 Hz -> clamped to 1 tick, 0 extra packets
    let (bytes, steps) = compile_macro_bytecode(&make_single(0), 1, 0x0400).unwrap();
    assert_eq!(steps, 1);
    assert_eq!(bytes, vec![0x01, 0x00, 0x81, 0x04, 0x00, 0x00, 0x00, 0x00]);

    // delay_ms = 127 at 1000 Hz -> 0 extra packets, rem = 127 (0x7F | 0x80 = 0xFF)
    let (bytes, steps) = compile_macro_bytecode(&make_single(127), 1, 0x0400).unwrap();
    assert_eq!(steps, 1);
    assert_eq!(bytes, vec![0x01, 0x00, 0xFF, 0x04, 0x00, 0x00, 0x00, 0x00]);

    // delay_ms = 128 at 1000 Hz -> 1 trailing [0x7F, 0x00], rem = 1 (0x81), step_count = 2
    let (bytes, steps) = compile_macro_bytecode(&make_single(128), 1, 0x0400).unwrap();
    assert_eq!(steps, 2);
    assert_eq!(bytes, vec![0x01, 0x00, 0x81, 0x04, 0x7F, 0x00, 0x00, 0x00]);

    // delay_ms = 254 at 1000 Hz -> 1 trailing [0x7F, 0x00], rem = 127 (0xFF), step_count = 2
    let (bytes, steps) = compile_macro_bytecode(&make_single(254), 1, 0x0400).unwrap();
    assert_eq!(steps, 2);
    assert_eq!(bytes, vec![0x01, 0x00, 0xFF, 0x04, 0x7F, 0x00, 0x00, 0x00]);

    // delay_ms = 20000 at 1000 Hz -> clamped to 10000 ms -> 78 trailing [0x7F, 0x00], rem = 94
    let (bytes, steps) = compile_macro_bytecode(&make_single(20000), 1, 0x0400).unwrap();
    assert_eq!(steps, 1 + 78);
    assert_eq!(bytes[2], 94 | 0x80);
    assert_eq!(bytes[3], 0x04);
    for i in 0..78 {
        assert_eq!(&bytes[4 + i * 2..6 + i * 2], &[0x7F, 0x00]);
    }
    assert_eq!(bytes.len() % 8, 0);

    // 125 Hz (polling_ms = 8): delay_ms = 1024 -> 128 ticks -> 1 trailing [0x7F, 0x00], rem = 1
    let (bytes, steps) = compile_macro_bytecode(&make_single(1024), 8, 0x0400).unwrap();
    assert_eq!(steps, 2);
    assert_eq!(bytes, vec![0x01, 0x00, 0x81, 0x04, 0x7F, 0x00, 0x00, 0x00]);

    // 125 Hz (polling_ms = 8): delay_ms = 4 -> 0 ticks -> clamped to 1 tick
    let (bytes, steps) = compile_macro_bytecode(&make_single(4), 8, 0x0400).unwrap();
    assert_eq!(steps, 1);
    assert_eq!(bytes, vec![0x01, 0x00, 0x81, 0x04, 0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn recoil_loop_expansion_for_short_toggle_macro() {
    // flags == 0x02 (RepeatMode::UntilClick, finish_cycle = false) with 3 events totaling 9 ms -> expands 40 / 9 = 4 times (12 events)
    let akm = Macro {
        name: "AKM".into(),
        repeat_mode: RepeatMode::UntilClick,
        finish_cycle: false,
        opt_flag: 1,
        opt_val: 2,
        events: vec![
            MacroEvent {
                kind: MacroEventKind::KeyDown { hid: 0xF1 },
                delay_ms: 3,
            },
            MacroEvent {
                kind: MacroEventKind::MoveRelative { dx: 0, dy: 2 },
                delay_ms: 3,
            },
            MacroEvent {
                kind: MacroEventKind::KeyUp { hid: 0xF1 },
                delay_ms: 3,
            },
        ],
        led_blocks: Vec::new(),
    };

    let (bytes, steps) = compile_macro_bytecode(&akm, 1, 0x0400).unwrap();
    assert_eq!(steps, 12);
    // Header repeat count = 1 for toggle mode (flags & 3 == 2)
    assert_eq!(&bytes[0..2], &[0x01, 0x00]);
    // Each iteration = 2 (KeyDown) + 6 (MoveRelative) + 2 (KeyUp) = 10 bytes -> 4 * 10 = 40 bytes + 2 header = 42 -> padded to 48
    assert_eq!(bytes.len(), 48);
    // Verify first iteration bytes
    assert_eq!(
        &bytes[2..12],
        &[0x83, 0xF1, 0x03, 0xF6, 0x00, 0x00, 0x02, 0x00, 0x03, 0xF1]
    );
    // Verify 4th iteration matches 1st iteration
    assert_eq!(&bytes[32..42], &bytes[2..12]);
}

#[test]
fn macro_overflow_fails_before_flash() {
    let mut cfg = factory_default_config();
    cfg.polling_rate = PollingRate::Hz1000;
    for p in &mut cfg.profiles {
        p.keys = [KeyAction::Disabled; 8];
    }

    // Create 6 distinct macros with 100 events each (~208 bytes each = 1248 bytes > 0x07F8 - 0x0400 = 1016 bytes)
    let mut macros = Vec::new();
    for m_idx in 0..6u8 {
        let events = (0..100)
            .map(|_| MacroEvent {
                kind: MacroEventKind::KeyDown { hid: 0x04 + m_idx },
                delay_ms: 5,
            })
            .collect();
        macros.push(simple_macro(
            &format!("Big{m_idx}"),
            RepeatMode::Once,
            events,
        ));
    }
    cfg.macros.groups.push(MacroGroup {
        name: "Big".into(),
        macros,
    });
    for i in 0..6u8 {
        cfg.profiles[0].keys[i as usize] = KeyAction::UserMacro { group: 0, index: i };
    }

    let ctx = CompileContext {
        hw_flags: 0x0F,
        existing_0x76: 0x00,
        macro_limit: 0x07F8,
    };
    let err = compile_plan(&cfg, ctx).expect_err("must fail with MacroOverflow");
    match err {
        X806Error::MacroOverflow { used, limit } => {
            assert_eq!(limit, 0x07F8 - 0x0400);
            assert!(used > limit, "expected used ({used}) > limit ({limit})");
        }
        other => panic!("expected MacroOverflow, got {other:?}"),
    }
}

#[test]
fn macro_bytecode_xy_dpi_and_led_anim_blocks() {
    let mut led_block = vec![0u8; 68];
    led_block[0] = 2; // raw_len = 2 * 4 + 4 = 12 -> padded to 16; written with out[0] = 1
    led_block[1] = 0xAA;
    led_block[2] = 0xBB;
    led_block[11] = 0xCC;

    let mac = Macro {
        name: "Specials".into(),
        repeat_mode: RepeatMode::Count(5),
        finish_cycle: false,
        opt_flag: 0,
        opt_val: 10,
        events: vec![
            MacroEvent {
                kind: MacroEventKind::MoveRelative { dx: -5, dy: 10 },
                delay_ms: 4,
            },
            MacroEvent {
                kind: MacroEventKind::DpiSet { dpi_code: 0x0A },
                delay_ms: 6,
            },
            MacroEvent {
                kind: MacroEventKind::DpiRestore { dpi_code: 0x04 },
                delay_ms: 7,
            },
            MacroEvent {
                kind: MacroEventKind::LedAnim {
                    channel: 2,
                    index: 0,
                },
                delay_ms: 8,
            },
        ],
        led_blocks: vec![led_block],
    };

    // Base address 0x0420:
    // Header: 2 bytes [0x05, 0x00]
    // MoveRelative (-5, 10): 6 bytes [0x04, 0xF6, 0xFB, 0xFF, 0x0A, 0x00]
    // DpiSet(0x0A): 4 bytes [0x86, 0xF8, 0x0A, 0x00]
    // DpiRestore(0x04): 4 bytes [0x07, 0xF8, 0x04, 0x00]
    // LedAnim(ch=2, idx=0): 4 bytes [0x08, 0xF9, addr_lo, (addr_hi & 0x3F) | (2 << 6)]
    // Stream length before pad = 2 + 6 + 4 + 4 + 4 = 20 -> padded to 24 (0x18)
    // LED block absolute addr = 0x0420 + 0x18 = 0x0438
    let (bytes, steps) = compile_macro_bytecode(&mac, 1, 0x0420).unwrap();
    assert_eq!(steps, 4);
    assert_eq!(bytes.len(), 24 + 16);
    assert_eq!(&bytes[0..2], &5u16.to_le_bytes());
    assert_eq!(&bytes[2..8], &[0x04, 0xF6, 0xFB, 0xFF, 0x0A, 0x00]);
    assert_eq!(&bytes[8..12], &[0x86, 0xF8, 0x0A, 0x00]);
    assert_eq!(&bytes[12..16], &[0x07, 0xF8, 0x04, 0x00]);
    // 0x0438 -> lo = 0x38, hi = 0x04 | (2 << 6) = 0x84
    assert_eq!(&bytes[16..20], &[0x08, 0xF9, 0x38, 0x84]);
    assert_eq!(&bytes[20..24], &[0x00, 0x00, 0x00, 0x00]);
    // Appended LED block: out[0] = b[0] - 1 = 1
    assert_eq!(bytes[24], 1);
    assert_eq!(bytes[25], 0xAA);
    assert_eq!(bytes[26], 0xBB);
    assert_eq!(bytes[35], 0xCC);
    assert_eq!(&bytes[36..40], &[0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn descriptor_repeat_flags_left_mac_and_invalid_macro_ref() {
    let mut cfg = factory_default_config();
    for p in &mut cfg.profiles {
        p.keys = [KeyAction::Disabled; 8];
    }

    let mut hold_mac = simple_macro(
        "Hold",
        RepeatMode::UntilRelease,
        vec![MacroEvent {
            kind: MacroEventKind::KeyDown { hid: 0x04 },
            delay_ms: 10,
        }],
    );
    hold_mac.finish_cycle = true;

    let toggle_mac = simple_macro(
        "Toggle",
        RepeatMode::UntilClick,
        vec![MacroEvent {
            kind: MacroEventKind::KeyDown { hid: 0x05 },
            delay_ms: 50,
        }],
    );

    let count_mac = simple_macro(
        "Count3",
        RepeatMode::Count(3),
        vec![MacroEvent {
            kind: MacroEventKind::KeyDown { hid: 0x06 },
            delay_ms: 10,
        }],
    );

    cfg.macros.groups.push(MacroGroup {
        name: "Modes".into(),
        macros: vec![hold_mac, toggle_mac, count_mac],
    });

    // Key 1 (hw_slot 7 -> 0x000E, desc 0x00DC): UntilRelease + finish_cycle + left_mac=true -> hw_key_code 0x1E
    cfg.profiles[0].keys[0] = KeyAction::UserMacro { group: 0, index: 0 };
    cfg.profiles[0].left_mac[0] = true;
    // Key 2 (hw_slot 3 -> 0x000A, desc 0x00B4): UntilClick + left_mac=true -> hw_key_code 0x1F
    cfg.profiles[0].keys[1] = KeyAction::UserMacro { group: 0, index: 1 };
    cfg.profiles[0].left_mac[1] = true;
    // Key 3 (hw_slot 2 -> 0x0009, desc 0x00AA): Count(3) + left_mac=false -> hw_key_code 0x0D
    cfg.profiles[0].keys[2] = KeyAction::UserMacro { group: 0, index: 2 };

    let mem = flatten_plan(&compile_plan(&cfg, CompileContext::default()).unwrap());
    assert_eq!(mem[&0x000E], 0x1E);
    assert_eq!(mem[&0x000A], 0x1F);
    assert_eq!(mem[&0x0009], 0x0D);

    // Hold descriptor: +6 bit 7 (repeat_until_release), +7 bit 6 (finish_cycle)
    let d_hold = read_slice(&mem, 0x00DC, 10);
    assert_eq!(d_hold[1] & 0x80, 0);
    assert_eq!(d_hold[6] & 0x80, 0x80);
    assert_eq!(d_hold[6] & 0x40, 0);
    assert_eq!(d_hold[7] & 0x40, 0x40);

    // Toggle descriptor: +1 bit 7 (repeat_until_click), +7 bit 6 = 0
    let d_toggle = read_slice(&mem, 0x00B4, 10);
    assert_eq!(d_toggle[1] & 0x80, 0x80);
    assert_eq!(d_toggle[6] & 0xC0, 0);
    assert_eq!(d_toggle[7] & 0x40, 0);

    // Count descriptor: +6 bit 6 (repeat_fixed_count)
    let d_count = read_slice(&mem, 0x00AA, 10);
    assert_eq!(d_count[6] & 0xC0, 0x40);

    // Invalid macro reference returns Err(X806Error::InvalidMacroRef)
    cfg.profiles[0].keys[3] = KeyAction::UserMacro { group: 2, index: 9 };
    let err = compile_plan(&cfg, CompileContext::default()).expect_err("invalid macro ref");
    assert!(matches!(
        err,
        X806Error::InvalidMacroRef { group: 2, index: 9 }
    ));
}

#[test]
fn raw_xy_quadrants_and_builtin_shortcut_pubg_compilation() {
    let mac = simple_macro(
        "RawQuad",
        RepeatMode::Once,
        vec![
            MacroEvent {
                kind: MacroEventKind::Raw {
                    event_type: 3,
                    hid: 0xF6,
                    arg1: 5,
                    arg2: 7,
                },
                delay_ms: 2,
            },
            MacroEvent {
                kind: MacroEventKind::Raw {
                    event_type: 4,
                    hid: 0xF6,
                    arg1: 5,
                    arg2: 7,
                },
                delay_ms: 2,
            },
            MacroEvent {
                kind: MacroEventKind::Raw {
                    event_type: 5,
                    hid: 0xF6,
                    arg1: 5,
                    arg2: 7,
                },
                delay_ms: 2,
            },
            MacroEvent {
                kind: MacroEventKind::Raw {
                    event_type: 6,
                    hid: 0xF6,
                    arg1: 5,
                    arg2: 7,
                },
                delay_ms: 2,
            },
        ],
    );
    let (bytes, steps) = compile_macro_bytecode(&mac, 1, 0x0400).unwrap();
    assert_eq!(steps, 4);
    assert_eq!(&bytes[2..8], &[0x02, 0xF6, 0xFB, 0xFF, 0xF9, 0xFF]); // (-5, -7)
    assert_eq!(&bytes[8..14], &[0x02, 0xF6, 0xFB, 0xFF, 0x07, 0x00]); // (-5, +7)
    assert_eq!(&bytes[14..20], &[0x02, 0xF6, 0x05, 0x00, 0xF9, 0xFF]); // (+5, -7)
    assert_eq!(&bytes[20..26], &[0x02, 0xF6, 0x05, 0x00, 0x07, 0x00]); // (+5, +7)

    // Built-in shortcut & PUBG macro hw_key_codes and deduplication across profiles
    let mut cfg = factory_default_config();
    for p in &mut cfg.profiles {
        p.keys = [KeyAction::Disabled; 8];
    }
    cfg.macros.pubg_macros.push(simple_macro(
        "GunToggle",
        RepeatMode::UntilClick,
        vec![MacroEvent {
            kind: MacroEventKind::MoveRelative { dx: 0, dy: 2 },
            delay_ms: 50,
        }],
    ));
    cfg.profiles[0].keys[0] = KeyAction::Shortcut(BuiltinShortcut::Copy);
    cfg.profiles[2].keys[0] = KeyAction::Shortcut(BuiltinShortcut::Copy); // deduplicated across profiles!
    cfg.profiles[0].keys[1] = KeyAction::PubgMacro { index: 0 }; // hw_key_code 0x1F

    let plan = compile_plan(&cfg, CompileContext::default()).unwrap();
    let mem = flatten_plan(&plan);
    assert_eq!(mem[&0x000E], 0x0D); // Profile 0 Key 1 Copy
    assert_eq!(mem[&0x0056], 0x0D); // Profile 2 Key 1 Copy
    assert_eq!(mem[&0x000A], 0x1F); // Profile 0 Key 2 PUBG toggle

    // Verify Profile 0 and Profile 2 Copy descriptors point to same 0x0400 address
    let d_p0 = read_slice(&mem, 0x00A0 + 6 * 10, 10);
    let d_p2 = read_slice(&mem, 0x00A0 + (2 * 14 + 6) * 10, 10);
    assert_eq!(d_p0[5], d_p2[5]);
    assert_eq!(d_p0[6] & 0x3F, d_p2[6] & 0x3F);
    assert_eq!(
        u16::from(d_p0[5]) | (u16::from(d_p0[6] & 0x3F) << 8),
        0x0400
    );
}

#[test]
fn rejects_dead_profile_slot_1_and_out_of_range_active_profile() {
    for bad_profile in [1u8, 4u8, 255u8] {
        let mut cfg = factory_default_config();
        cfg.active_profile = bad_profile;
        let err = compile_plan(&cfg, CompileContext::default())
            .expect_err("must reject invalid or dead active_profile");
        assert!(
            matches!(err, X806Error::SafetyViolation(_)),
            "expected SafetyViolation for active_profile={bad_profile}, got {err:?}"
        );
    }
}

#[test]
fn rejects_duplicate_keymap_hardware_slots() {
    let mut cfg = factory_default_config();
    cfg.keymap[1] = cfg.keymap[0]; // duplicate slot 7
    let err = compile_plan(&cfg, CompileContext::default())
        .expect_err("must reject duplicate keymap hw_slot");
    assert!(
        matches!(err, X806Error::SafetyViolation(_)),
        "expected SafetyViolation for duplicate hw_slot, got {err:?}"
    );
}

#[test]
fn rejects_led_anim_data_addr_exceeding_14bit_limit() {
    let mut led_block = vec![0u8; 68];
    led_block[0] = 1;
    let mac = Macro {
        name: "LedHighAddr".into(),
        repeat_mode: RepeatMode::Once,
        finish_cycle: false,
        opt_flag: 0,
        opt_val: 10,
        events: vec![MacroEvent {
            kind: MacroEventKind::LedAnim {
                channel: 0,
                index: 0,
            },
            delay_ms: 5,
        }],
        led_blocks: vec![led_block],
    };

    // Base 0x3FF8 + padded event stream (8 bytes) = 0x4000 > 0x3FFF
    let err = compile_macro_bytecode(&mac, 1, 0x3FF8)
        .expect_err("must reject LedAnim data_addr > 0x3FFF");
    assert!(
        matches!(err, X806Error::SafetyViolation(_)),
        "expected SafetyViolation for LedAnim addr > 0x3FFF, got {err:?}"
    );
}
