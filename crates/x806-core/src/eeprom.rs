use std::collections::HashMap;

use crate::X806Error;
use crate::defaults;
use crate::model::{BuiltinShortcut, DeviceConfig, KeyAction, Macro, MacroEventKind, RepeatMode};

/// EEPROM base address for the 560-byte action/macro descriptor table (`70 × 8` bytes).
pub const DESCRIPTOR_BASE: u16 = 0x00A0;
/// EEPROM base address for 8-byte-aligned compiled macro bytecode.
pub const MACRO_BASE: u16 = 0x0400;
/// Default extended 4 KB EEPROM macro address ceiling (`0x0FF8`; 2 KB units report `0x07F8`).
pub const DEFAULT_MACRO_LIMIT: usize = 0x0FF8;

const HEADER_SIZE: usize = 0x80;
const DESCRIPTOR_TABLE_SIZE: usize = 4 * 14 * 10; // 560 bytes (70 × 8-byte blocks)
const MAX_DELAY_MS: u16 = 10_000;
const MAX_STEP_COUNT: u32 = 0x1FFF; // 13-bit field across descriptor +1[6:0] and +7[5:0]
const ACTIVE_PROFILES: [usize; 3] = [0, 2, 3];

/// `DPI_SET` index (`0..=16`) -> `0x600068` sensor register byte (`protocol-spec.md` §4.3).
const DPI_REG_TABLE: [u8; 17] = [
    0x01, // 0: 200 DPI
    0x02, // 1: 400 DPI
    0x03, // 2: 600 DPI
    0x04, // 3: 800 DPI
    0x05, // 4: 1000 DPI
    0x06, // 5: 1200 DPI
    0x07, // 6: 1400 DPI
    0x08, // 7: 1600 DPI
    0x09, // 8: 1800 DPI
    0x0A, // 9: 2000 DPI
    0x0C, // 10: 2400 DPI
    0x0D, // 11: 3200 DPI
    0x0E, // 12: 4000 DPI
    0x0F, // 13: 4800 DPI
    0x10, // 14: 5600 DPI
    0x11, // 15: 6400 DPI
    0x12, // 16: 7200 DPI
];

/// Single atomic EEPROM write block (`1..=8` bytes) targeting `addr`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EepromWrite {
    pub addr: u16,
    pub data: Vec<u8>,
}

/// Complete, safety-verified EEPROM write sequence ready for flashing or dry-run inspection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EepromPlan {
    pub writes: Vec<EepromWrite>,
    /// Number of compiled macro bytecode bytes allocated starting at `0x0400`.
    pub macro_bytes_used: usize,
    /// Maximum usable macro bytecode capacity in bytes (`ctx.macro_limit - 0x0400`).
    pub macro_limit: usize,
}

/// Live hardware state read before compiling an EEPROM write plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompileContext {
    /// Low nibble of `GetFeature(0x07)` byte `buf[7]`, preserved in `0x003A[3:0]`.
    pub hw_flags: u8,
    /// Existing EEPROM byte at `0x0076`; bits `7:2` are preserved via read-modify-write.
    pub existing_0x76: u8,
    /// Hardware EEPROM macro end address (`0x07F8` or `0x0FF8`), from `GetFeature(0x07)`.
    pub macro_limit: usize,
}

impl Default for CompileContext {
    fn default() -> Self {
        Self {
            hw_flags: 0x0F,
            existing_0x76: 0x00,
            macro_limit: DEFAULT_MACRO_LIMIT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum MacroDedupKey {
    Shortcut(BuiltinShortcut),
    Pubg(u8),
    User(u8, u8),
}

/// Compiles a `DeviceConfig` and hardware `CompileContext` into a deterministic, safety-checked `EepromPlan`.
pub fn compile_plan(cfg: &DeviceConfig, ctx: CompileContext) -> Result<EepromPlan, X806Error> {
    if !ACTIVE_PROFILES.contains(&usize::from(cfg.active_profile)) {
        return Err(X806Error::SafetyViolation(format!(
            "invalid active_profile {} (must be 0, 2, or 3; slot 1 is unused in EEPROM)",
            cfg.active_profile
        )));
    }

    let mut seen_slots = [false; 12];
    for (ui_btn, &hw_slot) in cfg.keymap.iter().enumerate() {
        if !(1..=11).contains(&hw_slot) {
            return Err(X806Error::SafetyViolation(format!(
                "invalid keymap hardware slot {hw_slot} for button {ui_btn} (must be 1..=11)"
            )));
        }
        if seen_slots[usize::from(hw_slot)] {
            return Err(X806Error::SafetyViolation(format!(
                "duplicate keymap hardware slot {hw_slot} at button {ui_btn}"
            )));
        }
        seen_slots[usize::from(hw_slot)] = true;
    }

    let macro_capacity = ctx.macro_limit.saturating_sub(usize::from(MACRO_BASE));
    let polling_div = cfg.polling_rate.eeprom_divider();
    let polling_ms = cfg.polling_rate.tick_ms();

    let mut header = [0u8; HEADER_SIZE];
    let mut descriptors = [0u8; DESCRIPTOR_TABLE_SIZE];
    let mut macro_bytes: Vec<u8> = Vec::new();
    let mut macro_cache: HashMap<MacroDedupKey, (u16, u16)> = HashMap::new();

    // 0x0001: Active profile & layout mode (only byte written in 0x0000..=0x0007).
    header[0x0001] = ((cfg.active_profile & 0x03) << 5) | 0x03;

    // Compile active profiles 0 (`Game I`), 2 (`Game II`), 3 (`Game III`); Profile 1 stays zeroed.
    for profile_idx in ACTIVE_PROFILES {
        let prof = &cfg.profiles[profile_idx];
        for ui_btn in 0..8 {
            let hw_slot = cfg.keymap[ui_btn];
            let slot_idx = usize::from(hw_slot - 1);
            let desc_offset = (profile_idx * 14 + slot_idx) * 10;
            let action = prof.keys[ui_btn];
            let left_mac = prof.left_mac[ui_btn];

            let hw_key_code = compile_button_action(
                cfg,
                action,
                left_mac,
                polling_div,
                polling_ms,
                macro_capacity,
                &mut descriptors[desc_offset..desc_offset + 10],
                &mut macro_bytes,
                &mut macro_cache,
            )?;

            let btn_byte = ((prof.led_react_type[ui_btn] as u8 & 0x03) << 6)
                | (u8::from(prof.led_react_en[ui_btn]) << 5)
                | (hw_key_code & 0x1F);

            let hdr_off = profile_button_header_offset(profile_idx, hw_slot);
            header[hdr_off] = btn_byte;
        }
    }

    if macro_bytes.len() > macro_capacity {
        return Err(X806Error::MacroOverflow {
            used: macro_bytes.len(),
            limit: macro_capacity,
        });
    }

    // 0x0013..=0x0019: DPI register table & stage enable mask
    let dpi_regs: [u8; 6] = std::array::from_fn(|i| {
        DPI_REG_TABLE
            .get(usize::from(cfg.dpi_stages[i]))
            .copied()
            .unwrap_or(0x12)
    });
    header[0x0013] = (0..6).fold(0u8, |acc, i| acc | (((dpi_regs[i] >> 4) & 1) << i));
    header[0x0014] = 0x10;
    header[0x0015] = ((dpi_regs[0] & 0x0F) << 4) | (dpi_regs[1] & 0x0F);
    header[0x0016] = ((dpi_regs[2] & 0x0F) << 4) | (dpi_regs[3] & 0x0F);
    header[0x0017] = ((dpi_regs[4] & 0x0F) << 4) | (dpi_regs[5] & 0x0F);
    header[0x0018] = 0x18 | (cfg.dpi_level & 0x07);
    header[0x0019] = (0..4).fold(0u8, |acc, i| acc | (u8::from(cfg.dpi_enable[i]) << i));

    // 0x001A..=0x001B: Fire key interval (FIRE_TIME / 8, uint16_le)
    let fire_div = cfg.fire_time_ms / 8;
    header[0x001A..0x001C].copy_from_slice(&fire_div.to_le_bytes());

    // 0x001C: LED mode, reactive bit, and speed (0..=6 active, 7 reserved for Mute)
    let (led_hw_mode, led_react_bit, led_speed) = cfg.led.mode.eeprom_fields(cfg.led.speed);
    header[0x001C] = 0x80
        | ((led_hw_mode & 0x07) << 4)
        | (u8::from(led_react_bit) << 3)
        | (led_speed & 0x07);

    // 0x001D: LED cycle palette mask (bits 0..3 -> bits 0,2,4,6) & user color enable (bit 7)
    let c0 = cfg.led.cycle_mask & 1;
    let c1 = (cfg.led.cycle_mask >> 1) & 1;
    let c2 = (cfg.led.cycle_mask >> 2) & 1;
    let c3 = (cfg.led.cycle_mask >> 3) & 1;
    header[0x001D] =
        (u8::from(cfg.led.user_color_en) << 7) | (c3 << 6) | (c2 << 4) | (c1 << 2) | c0;

    // 0x001E: USB polling rate divider
    header[0x001E] = polling_div;

    // 0x0020..=0x0031: 6-stage DPI LED RGB colors (active-low 4-bit PWM, 0..=15)
    for (i, c) in cfg.led.dpi_colors.iter().enumerate() {
        let base = 0x0020 + i * 3;
        header[base] = c.r.min(15);
        header[base + 1] = c.g.min(15);
        header[base + 2] = c.b.min(15);
    }

    // 0x0032..=0x0033: Extended layout marker
    header[0x0032] = 0x07;
    header[0x0033] = 0x30;

    // 0x003A: LED symmetry/direction + preserved hardware flags
    header[0x003A] = (u8::from(cfg.led.symmetry) << 6)
        | ((cfg.led.direction & 0x03) << 4)
        | (ctx.hw_flags & 0x0F);

    // 0x003F: LED brightness
    header[0x003F] = (cfg.led.brightness & 0x07) << 4;

    // 0x0043: Constant 0xFC (DAT_0061e244 is never written in X806.exe)
    header[0x0043] = 0xFC;

    // 0x0076: Active profile mirror (read-modify-write preserving bits 7:2)
    header[0x0076] = (ctx.existing_0x76 & 0xFC) | (cfg.active_profile & 0x03);

    // Assemble deterministic write plan
    let mut writes = Vec::with_capacity(1 + 15 + 70 + macro_bytes.len() / 8);

    // 1. Single-byte write at 0x0001 (never touching 0x0000 or 0x0002..=0x0007)
    writes.push(EepromWrite {
        addr: 0x0001,
        data: vec![header[0x0001]],
    });

    // 2. Header 8-byte blocks: 0x0008..=0x007F (15 blocks)
    for addr in (0x0008usize..HEADER_SIZE).step_by(8) {
        writes.push(EepromWrite {
            addr: addr as u16,
            data: header[addr..addr + 8].to_vec(),
        });
    }

    // 3. Descriptor table 8-byte blocks: 0x00A0..=0x02CF (70 blocks)
    for (i, chunk) in descriptors.as_chunks::<8>().0.iter().enumerate() {
        writes.push(EepromWrite {
            addr: DESCRIPTOR_BASE + (i * 8) as u16,
            data: chunk.to_vec(),
        });
    }

    // 4. Compiled macro bytecode 8-byte blocks: 0x0400+
    for (i, chunk) in macro_bytes.as_chunks::<8>().0.iter().enumerate() {
        writes.push(EepromWrite {
            addr: MACRO_BASE + (i * 8) as u16,
            data: chunk.to_vec(),
        });
    }

    validate_plan_writes(&writes, ctx.macro_limit)?;

    Ok(EepromPlan {
        writes,
        macro_bytes_used: macro_bytes.len(),
        macro_limit: macro_capacity,
    })
}

/// Compiles a single `Macro` into 8-byte-aligned EEPROM bytecode placed at `base_addr`.
///
/// Returns `(padded_8byte_aligned_bytes, emitted_step_count)` where `emitted_step_count`
/// includes both source events (after any recoil expansion) and trailing `[0x7F, 0x00]`
/// split delay packets.
pub fn compile_macro_bytecode(
    mac: &Macro,
    polling_ms: u16,
    base_addr: u16,
) -> Result<(Vec<u8>, u16), X806Error> {
    if polling_ms == 0 {
        return Err(X806Error::SafetyViolation(
            "polling_ms must be >= 1".to_string(),
        ));
    }

    let (flags, repeat_count_raw) = mac.repeat_mode.to_dat_flags_and_count(mac.finish_cycle);

    // Recoil loop expansion (FUN_004130f0): if flags == 0x02 and floor(40 / total_ms) >= 2,
    // repeat the source event list floor(40 / total_ms) times before bytecode emission.
    let total_delay_ms: u32 = mac.events.iter().map(|e| u32::from(e.delay_ms)).sum();
    let expand_factor: usize = if flags == 0x02 && total_delay_ms > 0 {
        let f = (40 / total_delay_ms) as usize;
        if f >= 2 { f } else { 1 }
    } else {
        1
    };

    let mut stream: Vec<u8> = Vec::with_capacity(2 + mac.events.len() * expand_factor * 4 + 8);

    // Bytes 0..1: uint16_le repeat_count (1 for UntilRelease/UntilClick, else repeat_count)
    let mode = flags & 0x03;
    let hdr_repeat: u16 = if mode == 1 || mode == 2 {
        1
    } else {
        repeat_count_raw
    };
    stream.extend_from_slice(&hdr_repeat.to_le_bytes());

    let mut step_count: u32 = 0;
    let mut led_patches: Vec<(usize, u8, u16)> = Vec::new();

    for _ in 0..expand_factor {
        for ev in &mac.events {
            let clamped_ms = ev.delay_ms.min(MAX_DELAY_MS);
            let ticks = (u32::from(clamped_ms) / u32::from(polling_ms)).max(1);
            let extra_packets = (ticks - 1) / 127;
            let rem = (ticks - extra_packets * 127) as u8; // strictly in 1..=127

            emit_event_packet(&ev.kind, rem, &mut stream, &mut led_patches);

            for _ in 0..extra_packets {
                stream.extend_from_slice(&[0x7F, 0x00]);
            }

            step_count = step_count
                .checked_add(1 + extra_packets)
                .ok_or_else(|| X806Error::SafetyViolation("macro step count overflow".into()))?;
        }
    }

    if step_count > MAX_STEP_COUNT {
        return Err(X806Error::SafetyViolation(format!(
            "emitted macro step count {step_count} exceeds 13-bit hardware limit {MAX_STEP_COUNT}"
        )));
    }

    // Pad event stream to 8-byte boundary before appending any 0xF9 LED blocks.
    pad_to_8(&mut stream);

    // Append referenced LED animation blocks at 8-byte-aligned absolute EEPROM addresses.
    if !led_patches.is_empty() {
        let mut block_addrs: HashMap<u16, u16> = HashMap::new();
        for (patch_offset, channel, index) in led_patches {
            let data_addr = if let Some(&addr) = block_addrs.get(&index) {
                addr
            } else {
                let block = mac
                    .led_blocks
                    .get(usize::from(index))
                    .ok_or_else(|| {
                        X806Error::SafetyViolation(format!(
                            "LED animation event references missing led_blocks[{index}]"
                        ))
                    })?;
                if block.is_empty() || block[0] == 0 {
                    return Err(X806Error::SafetyViolation(format!(
                        "LED animation block[{index}] has invalid frame count 0"
                    )));
                }
                let addr = base_addr
                    .checked_add(u16::try_from(stream.len()).map_err(|_| {
                        X806Error::SafetyViolation("macro bytecode offset exceeds u16".into())
                    })?)
                    .ok_or_else(|| {
                        X806Error::SafetyViolation("LED block EEPROM address overflow".into())
                    })?;
                if addr > 0x3FFF {
                    return Err(X806Error::SafetyViolation(format!(
                        "LED animation block EEPROM address {addr:#06x} exceeds 14-bit limit 0x3FFF"
                    )));
                }

                let raw_len = usize::from(block[0]) * 4 + 4;
                let padded_len = raw_len.div_ceil(8) * 8;
                let mut out_block = vec![0u8; padded_len];
                let copy_len = block.len().min(raw_len);
                out_block[..copy_len].copy_from_slice(&block[..copy_len]);
                out_block[0] = block[0] - 1;
                stream.extend_from_slice(&out_block);

                block_addrs.insert(index, addr);
                addr
            };

            stream[patch_offset + 2] = (data_addr & 0xFF) as u8;
            stream[patch_offset + 3] =
                (((data_addr >> 8) as u8) & 0x3F) | ((channel & 0x03) << 6);
        }
    }

    pad_to_8(&mut stream);
    Ok((stream, step_count as u16))
}

#[allow(clippy::too_many_arguments)]
fn compile_button_action(
    cfg: &DeviceConfig,
    action: KeyAction,
    left_mac: bool,
    polling_div: u8,
    polling_ms: u16,
    macro_capacity: usize,
    desc_out: &mut [u8],
    macro_bytes: &mut Vec<u8>,
    macro_cache: &mut HashMap<MacroDedupKey, (u16, u16)>,
) -> Result<u8, X806Error> {
    match action {
        KeyAction::Disabled => Ok(0x00),
        KeyAction::LeftButton => Ok(0x01),
        KeyAction::MidButton => Ok(0x02),
        KeyAction::RightButton => Ok(0x03),
        KeyAction::Backward => Ok(0x04),
        KeyAction::Forward => Ok(0x05),
        KeyAction::DpiLoop => Ok(0x06),
        KeyAction::ShowDesktop => Ok(0x07),
        KeyAction::DoubleClick => Ok(0x08),
        KeyAction::Fire => Ok(0x09),
        KeyAction::LedLoop => Ok(0x15),
        KeyAction::DpiUp => Ok(0x0B),
        KeyAction::DpiDown => Ok(0x0C),
        KeyAction::FnKey => Ok(0x14),
        KeyAction::ProfileGame1 => Ok(0x17),
        KeyAction::ProfileGame2 => Ok(0x19),
        KeyAction::ProfileGame3 => Ok(0x1A),
        KeyAction::RawLegacy(v) => match v {
            7..=11 => Ok((v - 5) as u8),
            _ => Ok(0x00),
        },
        KeyAction::Media(m) => {
            desc_out.fill(0);
            desc_out[0] = polling_div;
            desc_out[3..5].copy_from_slice(&m.consumer_usage().to_le_bytes());
            Ok(0x0F)
        }
        KeyAction::SingleKey { hid } => {
            // Single-key remap (K=42) is NEVER deduplicated and always allocates 8 bytes.
            let next_addr_usize = usize::from(MACRO_BASE) + macro_bytes.len();
            if macro_bytes.len() + 8 > macro_capacity || next_addr_usize > usize::from(u16::MAX) {
                return Err(X806Error::MacroOverflow {
                    used: macro_bytes.len() + 8,
                    limit: macro_capacity,
                });
            }
            let macro_addr = next_addr_usize as u16;
            macro_bytes.extend_from_slice(&[0x01, 0x00, 0x80, hid, 0x00, 0x00, 0x00, 0x00]);

            desc_out.fill(0);
            desc_out[0] = polling_div;
            desc_out[1] = 0x01; // step_count = 1, repeat_until_click = 0
            desc_out[2] = hid;
            desc_out[5] = (macro_addr & 0xFF) as u8;
            desc_out[6] = ((macro_addr >> 8) & 0x3F) as u8;
            Ok(0x0E)
        }
        KeyAction::Shortcut(s) => {
            let fallback;
            let mac = if let Some(m) = cfg.macros.builtin_shortcuts.get(s as usize) {
                m
            } else {
                fallback = defaults::builtin_shortcuts();
                &fallback[s as usize]
            };
            let is_toggle = matches!(mac.repeat_mode, RepeatMode::UntilClick);
            let hw_key_code = macro_hw_key_code(left_mac, is_toggle);
            compile_dedup_macro(
                MacroDedupKey::Shortcut(s),
                mac,
                polling_div,
                polling_ms,
                macro_capacity,
                desc_out,
                macro_bytes,
                macro_cache,
            )?;
            Ok(hw_key_code)
        }
        KeyAction::PubgMacro { index } => {
            let mac = cfg
                .macros
                .pubg_macros
                .get(usize::from(index))
                .ok_or(X806Error::InvalidMacroRef { group: 0xFF, index })?;
            let is_toggle = matches!(mac.repeat_mode, RepeatMode::UntilClick);
            let hw_key_code = if is_toggle { 0x1F } else { 0x1E };
            compile_dedup_macro(
                MacroDedupKey::Pubg(index),
                mac,
                polling_div,
                polling_ms,
                macro_capacity,
                desc_out,
                macro_bytes,
                macro_cache,
            )?;
            Ok(hw_key_code)
        }
        KeyAction::UserMacro { group, index } => {
            let mac = cfg
                .macros
                .groups
                .get(usize::from(group))
                .and_then(|g| g.macros.get(usize::from(index)))
                .ok_or(X806Error::InvalidMacroRef { group, index })?;
            let is_toggle = matches!(mac.repeat_mode, RepeatMode::UntilClick);
            let hw_key_code = macro_hw_key_code(left_mac, is_toggle);
            compile_dedup_macro(
                MacroDedupKey::User(group, index),
                mac,
                polling_div,
                polling_ms,
                macro_capacity,
                desc_out,
                macro_bytes,
                macro_cache,
            )?;
            Ok(hw_key_code)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn compile_dedup_macro(
    key: MacroDedupKey,
    mac: &Macro,
    polling_div: u8,
    polling_ms: u16,
    macro_capacity: usize,
    desc_out: &mut [u8],
    macro_bytes: &mut Vec<u8>,
    macro_cache: &mut HashMap<MacroDedupKey, (u16, u16)>,
) -> Result<(), X806Error> {
    let (macro_addr, step_count) = if let Some(&cached) = macro_cache.get(&key) {
        cached
    } else {
        let base_usize = usize::from(MACRO_BASE) + macro_bytes.len();
        if base_usize > usize::from(u16::MAX) {
            return Err(X806Error::MacroOverflow {
                used: macro_bytes.len(),
                limit: macro_capacity,
            });
        }
        let base_addr = base_usize as u16;
        let (compiled, steps) = compile_macro_bytecode(mac, polling_ms, base_addr)?;
        let new_used = macro_bytes.len() + compiled.len();
        if new_used > macro_capacity {
            return Err(X806Error::MacroOverflow {
                used: new_used,
                limit: macro_capacity,
            });
        }
        macro_bytes.extend_from_slice(&compiled);
        macro_cache.insert(key, (base_addr, steps));
        (base_addr, steps)
    };

    write_macro_descriptor(desc_out, polling_div, mac, macro_addr, step_count);
    Ok(())
}

fn write_macro_descriptor(
    desc_out: &mut [u8],
    polling_div: u8,
    mac: &Macro,
    macro_addr: u16,
    step_count: u16,
) {
    let (flags, _) = mac.repeat_mode.to_dat_flags_and_count(mac.finish_cycle);
    let mode = flags & 0x03;

    let repeat_until_click = u8::from(mode == 2);
    let repeat_until_release = u8::from(mode == 1);
    let repeat_fixed_count = u8::from(mode == 0 && (flags & 0x04) != 0);
    let finish_cycle_flag = u8::from((mode == 1 || mode == 2) && (flags & 0x08) != 0);

    desc_out.fill(0);
    desc_out[0] = polling_div;
    desc_out[1] = (repeat_until_click << 7) | ((step_count as u8) & 0x7F);
    desc_out[2] = 0x00;
    desc_out[3] = 0x00;
    desc_out[4] = 0x00;
    desc_out[5] = (macro_addr & 0xFF) as u8;
    desc_out[6] = (repeat_until_release << 7)
        | (repeat_fixed_count << 6)
        | (((macro_addr >> 8) as u8) & 0x3F);
    desc_out[7] = (finish_cycle_flag << 6) | (((step_count >> 7) as u8) & 0x3F);
    desc_out[8] = ((mac.opt_flag & 0x01) << 7) | (mac.opt_val & 0x7F);
    desc_out[9] = 0x00;
}

fn macro_hw_key_code(left_mac: bool, is_toggle: bool) -> u8 {
    match (left_mac, is_toggle) {
        (false, false) => 0x0D,
        (false, true) => 0x0E,
        (true, false) => 0x1E,
        (true, true) => 0x1F,
    }
}

fn profile_button_header_offset(profile_idx: usize, hw_slot: u8) -> usize {
    let slot = usize::from(hw_slot);
    match profile_idx {
        0 => {
            if slot <= 9 {
                0x0008 + (slot - 1)
            } else {
                0x003B + (slot - 10)
            }
        }
        2 => 0x0050 + (slot - 1),
        3 => 0x0060 + (slot - 1),
        _ => unreachable!("only profiles 0, 2, 3 are compiled"),
    }
}

fn emit_event_packet(
    kind: &MacroEventKind,
    rem: u8,
    stream: &mut Vec<u8>,
    led_patches: &mut Vec<(usize, u8, u16)>,
) {
    let r = rem & 0x7F;
    match *kind {
        MacroEventKind::KeyDown { hid } => {
            stream.extend_from_slice(&[r | 0x80, hid]);
        }
        MacroEventKind::KeyUp { hid } => {
            stream.extend_from_slice(&[r, hid]);
        }
        MacroEventKind::Delay => {
            stream.extend_from_slice(&[r, 0x00]);
        }
        MacroEventKind::MoveRelative { dx, dy } => {
            let [dx_lo, dx_hi] = dx.to_le_bytes();
            let [dy_lo, dy_hi] = dy.to_le_bytes();
            stream.extend_from_slice(&[r, 0xF6, dx_lo, dx_hi, dy_lo, dy_hi]);
        }
        MacroEventKind::DpiSet { dpi_code } => {
            stream.extend_from_slice(&[r | 0x80, 0xF8, dpi_code, 0x00]);
        }
        MacroEventKind::DpiRestore { dpi_code } => {
            stream.extend_from_slice(&[r, 0xF8, dpi_code, 0x00]);
        }
        MacroEventKind::LedAnim { channel, index } => {
            let patch_offset = stream.len();
            led_patches.push((patch_offset, channel.min(2), index));
            stream.extend_from_slice(&[r, 0xF9, 0x00, 0x00]);
        }
        MacroEventKind::Raw {
            event_type,
            hid,
            arg1,
            arg2,
        } => match event_type {
            1 => stream.extend_from_slice(&[r | 0x80, hid]),
            2 => stream.extend_from_slice(&[r, hid]),
            3 => {
                let [dx_lo, dx_hi] = arg1.wrapping_neg().to_le_bytes();
                let [dy_lo, dy_hi] = arg2.wrapping_neg().to_le_bytes();
                stream.extend_from_slice(&[r, 0xF6, dx_lo, dx_hi, dy_lo, dy_hi]);
            }
            4 => {
                let [dx_lo, dx_hi] = arg1.wrapping_neg().to_le_bytes();
                let [dy_lo, dy_hi] = arg2.to_le_bytes();
                stream.extend_from_slice(&[r, 0xF6, dx_lo, dx_hi, dy_lo, dy_hi]);
            }
            5 => {
                let [dx_lo, dx_hi] = arg1.to_le_bytes();
                let [dy_lo, dy_hi] = arg2.wrapping_neg().to_le_bytes();
                stream.extend_from_slice(&[r, 0xF6, dx_lo, dx_hi, dy_lo, dy_hi]);
            }
            6 => {
                let [dx_lo, dx_hi] = arg1.to_le_bytes();
                let [dy_lo, dy_hi] = arg2.to_le_bytes();
                stream.extend_from_slice(&[r, 0xF6, dx_lo, dx_hi, dy_lo, dy_hi]);
            }
            9 => stream.extend_from_slice(&[r | 0x80, 0xF8, arg1 as u8, 0x00]),
            10 => stream.extend_from_slice(&[r, 0xF8, arg1 as u8, 0x00]),
            13..=15 => {
                let patch_offset = stream.len();
                led_patches.push((patch_offset, event_type - 13, arg1 as u16));
                stream.extend_from_slice(&[r, 0xF9, 0x00, 0x00]);
            }
            _ => stream.extend_from_slice(&[r, hid]),
        },
    }
}

fn pad_to_8(buf: &mut Vec<u8>) {
    let rem = buf.len() % 8;
    if rem != 0 {
        buf.resize(buf.len() + (8 - rem), 0x00);
    }
}

/// Validates every write in the compiled EEPROM plan against protected factory addresses
/// (`0x0000` and `0x0002..=0x0007`), block length (`1..=8`), and `macro_limit`.
fn validate_plan_writes(writes: &[EepromWrite], macro_limit: usize) -> Result<(), X806Error> {
    for w in writes {
        if w.data.is_empty() || w.data.len() > 8 {
            return Err(X806Error::SafetyViolation(format!(
                "write at {:#06x} has invalid length {} (must be 1..=8)",
                w.addr,
                w.data.len()
            )));
        }
        let end = w
            .addr
            .checked_add(w.data.len() as u16)
            .ok_or_else(|| X806Error::SafetyViolation("EEPROM write address overflow".into()))?;
        for addr in w.addr..end {
            if addr == 0x0000 || (0x0002..=0x0007).contains(&addr) {
                return Err(X806Error::SafetyViolation(format!(
                    "refusing write to protected factory EEPROM byte {addr:#06x}"
                )));
            }
            if addr > 0x3FFF {
                return Err(X806Error::SafetyViolation(format!(
                    "write byte {addr:#06x} exceeds 14-bit EEPROM address range"
                )));
            }
            if usize::from(addr) >= macro_limit {
                return Err(X806Error::SafetyViolation(format!(
                    "write byte {addr:#06x} exceeds hardware macro limit {macro_limit:#06x}"
                )));
            }
        }
    }
    Ok(())
}
