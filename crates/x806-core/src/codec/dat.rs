use std::collections::HashMap;
use std::fmt::Write as _;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use crate::X806Error;
use crate::keycode::{hid_to_vk, vk_to_hid};
use crate::model::{Macro, MacroEvent, MacroEventKind, RepeatMode};

const MAX_EVENTS: usize = 180;
const MAX_LED_BLOCKS: usize = 5;
const LED_BLOCK_LEN: usize = 68;
const RECORD_BYTES_V3: usize = 1861;
const RECORD_BYTES_LEGACY: usize = 1855;
const NAME_UTF16_UNITS: usize = 30;

/// Parses a `.dat` macro library file (`DAT_VER=3` or legacy, plain decimal or `ISD=1` Base64).
pub fn parse_dat(bytes: &[u8]) -> Result<Vec<Macro>, X806Error> {
    let text = super::decode_text_bytes(bytes);
    let sections = super::parse_ini_sections(&text, false);

    let init_kvs = sections
        .iter()
        .find(|(name, _)| name == "Init")
        .map(|(_, kvs)| kvs.as_slice())
        .unwrap_or(&[]);

    let mut dat_ver: u32 = 3;
    let mut isd_enabled = false;
    for (k, v) in init_kvs {
        match k.as_str() {
            "DAT_VER" => {
                dat_ver = v
                    .parse::<u32>()
                    .map_err(|_| X806Error::DatParse(format!("invalid DAT_VER: {v}")))?;
            }
            "ISD" => {
                let isd = v
                    .parse::<u32>()
                    .map_err(|_| X806Error::DatParse(format!("invalid ISD: {v}")))?;
                isd_enabled = isd != 0;
            }
            _ => {}
        }
    }

    let macros_kvs = sections
        .iter()
        .find(|(name, _)| name == "Macros")
        .map(|(_, kvs)| kvs.as_slice())
        .ok_or_else(|| X806Error::DatParse("missing [Macros] section".to_string()))?;

    let mut map: HashMap<&str, &str> = HashMap::with_capacity(macros_kvs.len());
    for (k, v) in macros_kvs {
        map.insert(k.as_str(), v.as_str());
    }

    let macro_num_str = map
        .get("macroNum")
        .ok_or_else(|| X806Error::DatParse("missing macroNum in [Macros]".to_string()))?;
    let macro_num = macro_num_str
        .parse::<usize>()
        .map_err(|_| X806Error::DatParse(format!("invalid macroNum: {macro_num_str}")))?;

    if macro_num > map.len() {
        return Err(X806Error::DatParse(format!(
            "macroNum {macro_num} exceeds available entries ({})",
            map.len()
        )));
    }

    let mut out = Vec::with_capacity(macro_num);
    for idx in 0..macro_num {
        let key = idx.to_string();
        let raw_val = map
            .get(key.as_str())
            .ok_or_else(|| X806Error::DatParse(format!("missing macro entry {idx}")))?;
        let decoded = decode_macro_record(raw_val, dat_ver, isd_enabled)?;
        out.push(decoded);
    }
    Ok(out)
}

/// Encodes macros as a `DAT_VER=3` `.dat` library (plain 3-digit decimal or `ISD=1` Base64).
pub fn encode_dat(macros: &[Macro], isd_base64: bool) -> String {
    let mut out = String::new();
    out.push_str("[Init]\r\nDAT_VER=3\r\n");
    if isd_base64 {
        out.push_str("ISD=1\r\n");
    }
    let _ = writeln!(out, "[Macros]\r\nmacroNum={}\r", macros.len());

    for (idx, mac) in macros.iter().enumerate() {
        let raw = encode_macro_record_v3(mac);
        let mut decimal = String::with_capacity(RECORD_BYTES_V3 * 3);
        for b in raw {
            let _ = write!(decimal, "{b:03}");
        }
        let payload = if isd_base64 {
            STANDARD.encode(decimal.as_bytes())
        } else {
            decimal
        };
        let _ = write!(out, "{idx}={payload}\r\n");
    }
    out
}

fn decode_macro_record(raw_val: &str, dat_ver: u32, isd_enabled: bool) -> Result<Macro, X806Error> {
    let trimmed = raw_val.trim();
    let decimal_owned: String;
    let decimal: &str = if isd_enabled {
        let decoded_bytes = STANDARD
            .decode(trimmed.as_bytes())
            .map_err(|e| X806Error::DatParse(format!("invalid Base64 in ISD=1 record: {e}")))?;
        decimal_owned = String::from_utf8(decoded_bytes)
            .map_err(|_| X806Error::DatParse("ISD=1 Base64 payload is not valid UTF-8".into()))?;
        decimal_owned.trim()
    } else {
        trimmed
    };

    if !decimal.len().is_multiple_of(3) {
        return Err(X806Error::DatParse(format!(
            "decimal stream length {} is not a multiple of 3",
            decimal.len()
        )));
    }
    if !decimal.bytes().all(|b| b.is_ascii_digit()) {
        return Err(X806Error::DatParse(
            "decimal stream contains non-digit characters".to_string(),
        ));
    }

    let v3 = dat_ver == 3;
    let expected_bytes = if v3 {
        RECORD_BYTES_V3
    } else {
        RECORD_BYTES_LEGACY
    };
    if decimal.len() / 3 < expected_bytes {
        return Err(X806Error::DatParse(format!(
            "truncated DAT record: got {} bytes, expected at least {}",
            decimal.len() / 3,
            expected_bytes
        )));
    }

    let mut raw = Vec::with_capacity(expected_bytes);
    for chunk in decimal.as_bytes()[..expected_bytes * 3].as_chunks::<3>().0 {
        let val = (u16::from(chunk[0] - b'0') * 100)
            + (u16::from(chunk[1] - b'0') * 10)
            + u16::from(chunk[2] - b'0');
        let byte = u8::try_from(val)
            .map_err(|_| X806Error::DatParse(format!("decimal byte value out of range: {val}")))?;
        raw.push(byte);
    }

    let name_units: Vec<u16> = raw[0..60]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&u| u != 0)
        .collect();
    let name = String::from_utf16_lossy(&name_units);

    let flags = raw[0x3C];
    let repeat_count = u16::from_le_bytes([raw[0x3D], raw[0x3E]]);
    let (repeat_mode, finish_cycle) = RepeatMode::from_dat_flags(flags, repeat_count);

    let step_count = usize::from(u16::from_le_bytes([raw[0x3F], raw[0x40]]));
    if step_count > MAX_EVENTS {
        return Err(X806Error::DatParse(format!(
            "step_count {step_count} exceeds max {MAX_EVENTS}"
        )));
    }

    let (opt_flag, opt_val, events_offset, led_offset) = if v3 {
        (raw[0x4B], raw[0x4C], 0x51usize, 0x5F1usize)
    } else {
        (0u8, 10u8, 0x4Busize, 0x5EBusize)
    };

    let mut events = Vec::with_capacity(step_count);
    for i in 0..step_count {
        let off = events_offset + i * 8;
        let ev = &raw[off..off + 8];
        let event_type = ev[0];
        let vk = ev[1];
        let arg1 = i16::from_le_bytes([ev[2], ev[3]]);
        let arg2 = i16::from_le_bytes([ev[4], ev[5]]);
        let delay_ms = u16::from_le_bytes([ev[6], ev[7]]);
        let kind = decode_event_kind(event_type, vk, arg1, arg2);
        events.push(MacroEvent { kind, delay_ms });
    }

    let declared_blocks = (usize::from(raw[0x49]) + usize::from(raw[0x4A])).min(MAX_LED_BLOCKS);
    let mut highest_nonzero = 0usize;
    for b_idx in 0..MAX_LED_BLOCKS {
        let start = led_offset + b_idx * LED_BLOCK_LEN;
        if raw[start..start + LED_BLOCK_LEN].iter().any(|&b| b != 0) {
            highest_nonzero = b_idx + 1;
        }
    }
    let block_count = declared_blocks.max(highest_nonzero);
    let mut led_blocks = Vec::with_capacity(block_count);
    for b_idx in 0..block_count {
        let start = led_offset + b_idx * LED_BLOCK_LEN;
        led_blocks.push(raw[start..start + LED_BLOCK_LEN].to_vec());
    }

    Ok(Macro {
        name,
        repeat_mode,
        finish_cycle,
        opt_flag,
        opt_val,
        events,
        led_blocks,
    })
}

fn decode_event_kind(event_type: u8, vk: u8, arg1: i16, arg2: i16) -> MacroEventKind {
    match event_type {
        0 if vk == 0 && arg1 == 0 && arg2 == 0 => MacroEventKind::Delay,
        1 => {
            let hid = vk_to_hid(vk);
            if hid != 0 {
                MacroEventKind::KeyDown { hid }
            } else {
                MacroEventKind::Raw {
                    event_type,
                    hid: vk,
                    arg1,
                    arg2,
                }
            }
        }
        2 => {
            if vk == 0 && arg1 == 0 && arg2 == 0 {
                MacroEventKind::Delay
            } else {
                let hid = vk_to_hid(vk);
                if hid != 0 {
                    MacroEventKind::KeyUp { hid }
                } else {
                    MacroEventKind::Raw {
                        event_type,
                        hid: vk,
                        arg1,
                        arg2,
                    }
                }
            }
        }
        3 => MacroEventKind::MoveRelative {
            dx: arg1.wrapping_neg(),
            dy: arg2.wrapping_neg(),
        },
        4 => MacroEventKind::MoveRelative {
            dx: arg1.wrapping_neg(),
            dy: arg2,
        },
        5 => MacroEventKind::MoveRelative {
            dx: arg1,
            dy: arg2.wrapping_neg(),
        },
        6 => MacroEventKind::MoveRelative { dx: arg1, dy: arg2 },
        9 => MacroEventKind::DpiSet {
            dpi_code: arg1 as u8,
        },
        10 => MacroEventKind::DpiRestore {
            dpi_code: arg1 as u8,
        },
        13..=15 => MacroEventKind::LedAnim {
            channel: event_type - 13,
            index: arg1 as u16,
        },
        _ => MacroEventKind::Raw {
            event_type,
            hid: vk,
            arg1,
            arg2,
        },
    }
}

fn encode_macro_record_v3(mac: &Macro) -> [u8; RECORD_BYTES_V3] {
    let mut buf = [0u8; RECORD_BYTES_V3];

    let mut units_written = 0usize;
    let mut utf16_tmp = [0u16; 2];
    for ch in mac.name.chars() {
        let encoded = ch.encode_utf16(&mut utf16_tmp);
        if units_written + encoded.len() > NAME_UTF16_UNITS {
            break;
        }
        for u in encoded.iter().copied() {
            let off = units_written * 2;
            buf[off..off + 2].copy_from_slice(&u.to_le_bytes());
            units_written += 1;
        }
    }

    let (flags, repeat_count) = mac.repeat_mode.to_dat_flags_and_count(mac.finish_cycle);
    buf[0x3C] = flags;
    buf[0x3D..0x3F].copy_from_slice(&repeat_count.to_le_bytes());

    let step_count = mac.events.len().min(MAX_EVENTS);
    buf[0x3F..0x41].copy_from_slice(&(step_count as u16).to_le_bytes());

    let mut xy_count: u16 = 0;
    let mut dpi_count: u16 = 0;
    let mut led0_count: u16 = 0;
    let mut led1_count: u16 = 0;

    for (i, ev) in mac.events.iter().take(MAX_EVENTS).enumerate() {
        let (ev_type, vk, arg1, arg2) = match &ev.kind {
            MacroEventKind::KeyDown { hid } => (1u8, hid_to_vk(*hid), 0i16, 0i16),
            MacroEventKind::KeyUp { hid } => (2u8, hid_to_vk(*hid), 0i16, 0i16),
            MacroEventKind::Delay => (0u8, 0u8, 0i16, 0i16),
            MacroEventKind::MoveRelative { dx, dy } => {
                xy_count = xy_count.saturating_add(1);
                let quadrant = match (*dx < 0, *dy < 0) {
                    (true, true) => 3u8,
                    (true, false) => 4u8,
                    (false, true) => 5u8,
                    (false, false) => 6u8,
                };
                (quadrant, 0xF6u8, dx.saturating_abs(), dy.saturating_abs())
            }
            MacroEventKind::DpiSet { dpi_code } => {
                dpi_count = dpi_count.saturating_add(1);
                (9u8, 0xF8u8, i16::from(*dpi_code), 0i16)
            }
            MacroEventKind::DpiRestore { dpi_code } => {
                dpi_count = dpi_count.saturating_add(1);
                (10u8, 0xF8u8, i16::from(*dpi_code), 0i16)
            }
            MacroEventKind::LedAnim { channel, index } => {
                let ch = (*channel).min(2);
                if ch == 0 {
                    led0_count = led0_count.saturating_add(1);
                } else {
                    led1_count = led1_count.saturating_add(1);
                }
                (13 + ch, 0xF9u8, *index as i16, 0i16)
            }
            MacroEventKind::Raw {
                event_type,
                hid,
                arg1,
                arg2,
            } => (*event_type, *hid, *arg1, *arg2),
        };

        let off = 0x51 + i * 8;
        buf[off] = ev_type;
        buf[off + 1] = vk;
        buf[off + 2..off + 4].copy_from_slice(&arg1.to_le_bytes());
        buf[off + 4..off + 6].copy_from_slice(&arg2.to_le_bytes());
        buf[off + 6..off + 8].copy_from_slice(&ev.delay_ms.to_le_bytes());
    }

    buf[0x41..0x43].copy_from_slice(&xy_count.to_le_bytes());
    buf[0x43..0x45].copy_from_slice(&dpi_count.to_le_bytes());
    buf[0x45..0x47].copy_from_slice(&led0_count.to_le_bytes());
    buf[0x47..0x49].copy_from_slice(&led1_count.to_le_bytes());

    let block_count = mac.led_blocks.len().min(MAX_LED_BLOCKS);
    buf[0x49] = block_count as u8;
    buf[0x4B] = mac.opt_flag;
    buf[0x4C] = mac.opt_val;

    for (bi, block) in mac.led_blocks.iter().take(MAX_LED_BLOCKS).enumerate() {
        let dest = 0x5F1 + bi * LED_BLOCK_LEN;
        let copy_len = block.len().min(LED_BLOCK_LEN);
        buf[dest..dest + copy_len].copy_from_slice(&block[..copy_len]);
    }

    buf
}
