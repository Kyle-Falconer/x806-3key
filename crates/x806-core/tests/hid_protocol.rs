use std::collections::VecDeque;
use std::time::Duration;

use x806_core::X806Error;
use x806_core::defaults::factory_default_config;
use x806_core::hid::{DeviceStatus, HidApiTransport, MockTransport, X806Device};
use x806_core::model::{KeyAction, Macro, MacroEvent, MacroEventKind, MacroGroup, RepeatMode};

fn live_status_report() -> [u8; 8] {
    [0x07, 0x00, 0x40, 0x09, 0x40, 0x00, 0x23, 0x1F]
}

#[test]
fn status_parses_live_captured_report() {
    let mut mock = MockTransport {
        feature_responses: VecDeque::from([
            Ok(live_status_report()),
            // Also verify 2 KB EEPROM layout + non-zero profile decoding:
            // buf[6] = 0b0000_1100 (profile 3, bit 1 = 0 -> macro_limit 0x07F8), buf[7] = 0xA5 (hw_flags 0x05)
            Ok([0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0xA5]),
        ]),
        ..Default::default()
    };
    let mut dev = X806Device::with_strings(
        &mut mock,
        Some("INSTANT".to_string()),
        Some("USB GAMING MOUSE ".to_string()),
    );

    let status: DeviceStatus = dev.read_status().expect("read_status should succeed");
    assert_eq!(status.active_profile, 0);
    assert_eq!(status.macro_limit, 0x0FF8);
    assert_eq!(status.hw_flags, 0x0F);
    assert_eq!(status.raw_report, live_status_report());
    assert_eq!(status.manufacturer.as_deref(), Some("INSTANT"));
    assert_eq!(status.product.as_deref(), Some("USB GAMING MOUSE "));

    let status_2k = dev.read_status().expect("read_status 2k should succeed");
    assert_eq!(status_2k.active_profile, 3);
    assert_eq!(status_2k.macro_limit, 0x07F8);
    assert_eq!(status_2k.hw_flags, 0x05);
}

#[test]
fn read_eeprom_byte_matches_exact_8_step_sequence() {
    let mock = MockTransport {
        feature_responses: VecDeque::from([Ok([0x07, 0xAC, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]),
        ..Default::default()
    };
    let mut dev = X806Device::new(mock);

    let val = dev
        .read_eeprom_byte(0x0076)
        .expect("read_eeprom_byte(0x0076) should succeed");
    assert_eq!(val, 0xAC);

    let transport = dev.into_transport();
    assert_eq!(
        transport.sent_reports,
        vec![
            vec![0x07, 0x18, 0x03, 0x00, 0x76, 0x00, 0x00, 0x00],
            vec![0x07, 0x18, 0x00, 0x00, 0x76, 0x00, 0x00, 0x00],
            vec![0x07, 0x18, 0x05, 0x00, 0x76, 0x00, 0x00, 0x00],
            vec![0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        ]
    );
    assert_eq!(
        transport.sleeps,
        vec![
            Duration::from_millis(4),
            Duration::from_millis(2),
            Duration::from_millis(2),
        ]
    );
    assert!(transport.feature_responses.is_empty());
}

#[test]
fn write_eeprom_block_rejects_protected_addresses_and_emits_exact_packets() {
    let mut dev = X806Device::new(MockTransport::default());

    assert!(matches!(
        dev.write_eeprom_block(0x0000, &[0x5A]),
        Err(X806Error::SafetyViolation(_))
    ));
    assert!(matches!(
        dev.write_eeprom_block(0x0002, &[0x00]),
        Err(X806Error::SafetyViolation(_))
    ));
    assert!(matches!(
        dev.write_eeprom_block(0x0007, &[0x00]),
        Err(X806Error::SafetyViolation(_))
    ));
    assert!(matches!(
        dev.write_eeprom_block(0x0001, &[0x03, 0x00]),
        Err(X806Error::SafetyViolation(_))
    ));

    // Rejected writes must emit zero HID reports
    dev.write_eeprom_block(0x0001, &[0x03])
        .expect("write_eeprom_block(0x0001, &[0x03]) should succeed");

    let transport = dev.into_transport();
    assert_eq!(
        transport.sent_reports,
        vec![
            vec![0x07, 0x18, 0x03, 0x00, 0x01, 0x00, 0x03, 0x00],
            vec![0x07, 0x18, 0x09, 0x00, 0x01, 0x00, 0x00, 0x00],
            vec![0x07, 0x18, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00],
        ]
    );
    assert_eq!(
        transport.sleeps,
        vec![Duration::from_millis(4), Duration::from_millis(2)]
    );
}

#[test]
fn flash_config_propagates_io_error_without_panicking() {
    let mock = MockTransport {
        feature_responses: VecDeque::from([
            Ok(live_status_report()),
            Ok([0x07, 0xFC, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
        ]),
        fail_on_send_index: Some(10),
        ..Default::default()
    };
    let mut dev = X806Device::new(mock);

    let err = dev
        .flash_config(&factory_default_config())
        .expect_err("flash_config should propagate HID error");
    assert!(matches!(err, X806Error::Hid(_)));
}

#[test]
fn flash_config_ends_with_apply_reload_and_20ms_sleep() {
    let mock = MockTransport {
        feature_responses: VecDeque::from([
            Ok(live_status_report()),
            // Existing 0x0076 value: bits 7:2 = 0xA8 should be preserved with active_profile = 0
            Ok([0x07, 0xAB, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
        ]),
        ..Default::default()
    };
    let mut dev = X806Device::new(mock);

    let cfg = factory_default_config();
    let plan = dev
        .flash_config(&cfg)
        .expect("flash_config should succeed on MockTransport");
    assert!(!plan.writes.is_empty());

    // Verify 0x0076 RMW preserved upper 6 bits (0xAB & 0xFC = 0xA8) | active_profile (0) = 0xA8
    let rmw_block = plan
        .writes
        .iter()
        .find(|w| w.addr == 0x0070)
        .expect("plan must include 0x0070..=0x0077 block");
    assert_eq!(rmw_block.data[6], 0xA8);

    let mut transport = dev.into_transport();
    let n_reports = transport.sent_reports.len();
    assert!(n_reports >= 2);
    assert_eq!(
        transport.sent_reports[n_reports - 2],
        vec![0x07, 0x18, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        transport.sent_reports[n_reports - 1],
        vec![0x07, 0x18, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );

    let n_sleeps = transport.sleeps.len();
    assert!(n_sleeps >= 1);
    assert_eq!(transport.sleeps[n_sleeps - 1], Duration::from_millis(20));

    // Also verify send_exit packet format
    let mut exit_dev = X806Device::new(std::mem::take(&mut transport));
    exit_dev.send_exit().expect("send_exit should succeed");
    let exit_transport = exit_dev.into_transport();
    assert_eq!(
        exit_transport.sent_reports.last().unwrap(),
        &vec![0x07, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
}

#[test]
fn write_eeprom_block_8_bytes_stages_indices_and_len_minus_1() {
    let mut dev = X806Device::new(MockTransport::default());
    let payload = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
    dev.write_eeprom_block(0x0400, &payload)
        .expect("8-byte block write should succeed");

    let transport = dev.into_transport();
    assert_eq!(transport.sent_reports.len(), 10);
    for (i, &byte) in payload.iter().enumerate() {
        assert_eq!(
            transport.sent_reports[i],
            vec![0x07, 0x18, 0x03, i as u8, 0x00, 0x04, byte, 0x07]
        );
    }
    assert_eq!(
        transport.sent_reports[8],
        vec![0x07, 0x18, 0x09, 0x00, 0x00, 0x04, 0x00, 0x07]
    );
    assert_eq!(
        transport.sent_reports[9],
        vec![0x07, 0x18, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00]
    );
    assert_eq!(
        transport.sleeps,
        vec![Duration::from_millis(4), Duration::from_millis(2)]
    );
}

#[test]
fn flash_config_aborts_before_any_write_on_macro_overflow() {
    // Status reports 2 KB unit (macro_limit = 0x07F8 -> 1016 bytes usable from 0x0400)
    let mock = MockTransport {
        feature_responses: VecDeque::from([
            Ok([0x07, 0x00, 0x40, 0x09, 0x40, 0x00, 0x21, 0x1F]),
            Ok([0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]),
        ]),
        ..Default::default()
    };
    let mut dev = X806Device::new(mock);

    let mut cfg = factory_default_config();
    cfg.macros.groups.push(MacroGroup {
        name: "Big".to_string(),
        macros: vec![Macro {
            name: "Overflow".to_string(),
            repeat_mode: RepeatMode::Once,
            finish_cycle: false,
            opt_flag: 0,
            opt_val: 0,
            events: vec![
                MacroEvent {
                    kind: MacroEventKind::KeyDown { hid: 0x04 },
                    delay_ms: 10,
                };
                600
            ],
            led_blocks: Vec::new(),
        }],
    });
    cfg.profiles[0].keys[0] = KeyAction::UserMacro { group: 0, index: 0 };

    let err = dev
        .flash_config(&cfg)
        .expect_err("overflowing macro must fail before writing");
    assert!(matches!(err, X806Error::MacroOverflow { limit: 1016, .. }));

    // Only the 4 reports from read_eeprom_byte(0x0076) should have been sent; zero writes!
    let transport = dev.into_transport();
    assert_eq!(transport.sent_reports.len(), 4);
}

#[test]
#[ignore = "requires attached X806 hardware (strictly read-only)"]
fn live_hardware_readonly_status() {
    let (transport, manufacturer, product) =
        HidApiTransport::open_first().expect("failed to open attached X806 control interface");
    let mut dev = X806Device::with_strings(transport, manufacturer, product);

    let status = dev.read_status().expect("read_status failed");
    let byte_0x76 = dev
        .read_eeprom_byte(0x0076)
        .expect("read_eeprom_byte(0x0076) failed");

    println!(
        "Live X806 status: manufacturer={:?}, product={:?}, raw={:02X?}, active_profile={}, macro_limit={:#06X}, hw_flags={:#04X}, eeprom[0x0076]={:#04X}",
        status.manufacturer,
        status.product,
        status.raw_report,
        status.active_profile,
        status.macro_limit,
        status.hw_flags,
        byte_0x76
    );
    assert!(status.macro_limit == 0x07F8 || status.macro_limit == 0x0FF8);
    assert!(status.active_profile <= 3);
}
