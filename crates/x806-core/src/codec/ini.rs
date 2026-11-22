use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::defaults;
use crate::model::{
    BuiltinShortcut, DeviceConfig, KeyAction, LedMode, LegacyOsSettings, MacroGroup, MediaKey,
    PollingRate, ReactiveStyle,
};
use crate::X806Error;

use super::dat;

const MACRO_GRP_SLOTS: usize = 20;

/// Parses `macroGrp.ini` (UTF-16LE with BOM or UTF-8) into ordered macro group names.
pub fn parse_macro_groups(bytes: &[u8]) -> Result<Vec<String>, X806Error> {
    let text = super::decode_text_bytes(bytes);
    let sections = super::parse_ini_sections(&text, true);

    let num_kvs = sections
        .iter()
        .find(|(s, _)| s == "MACRO_GROUP_NUM")
        .map(|(_, kvs)| kvs.as_slice())
        .ok_or_else(|| X806Error::IniParse("missing [MACRO_GROUP_NUM] section".to_string()))?;

    let count_str = num_kvs
        .iter()
        .find(|(k, _)| k == "macGrpNum")
        .map(|(_, v)| v.as_str())
        .ok_or_else(|| X806Error::IniParse("missing macGrpNum key".to_string()))?;

    let count = count_str
        .parse::<usize>()
        .map_err(|_| X806Error::IniParse(format!("invalid macGrpNum: {count_str}")))?;

    if count == 0 {
        return Ok(Vec::new());
    }

    let name_kvs = sections
        .iter()
        .find(|(s, _)| s == "MACRO_GROUP_NAME")
        .map(|(_, kvs)| kvs.as_slice())
        .ok_or_else(|| X806Error::IniParse("missing [MACRO_GROUP_NAME] section".to_string()))?;

    if count > name_kvs.len() {
        return Err(X806Error::IniParse(format!(
            "macGrpNum {count} exceeds available entries ({})",
            name_kvs.len()
        )));
    }

    let mut out = Vec::with_capacity(count);
    for idx in 0..count {
        let key = idx.to_string();
        let name = name_kvs
            .iter()
            .rfind(|(k, _)| k == &key)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| {
                X806Error::IniParse(format!("missing macro group entry {idx} in [MACRO_GROUP_NAME]"))
            })?;
        out.push(name);
    }
    Ok(out)
}

/// Encodes macro group names into `macroGrp.ini` bytes (UTF-16LE with BOM `0xFF 0xFE`).
pub fn encode_macro_groups(names: &[String]) -> Vec<u8> {
    let mut text = String::new();
    let _ = write!(
        text,
        "[MACRO_GROUP_NUM]\r\nmacGrpNum={}\r\n[MACRO_GROUP_NAME]\r\n",
        names.len()
    );
    let total_slots = names.len().max(MACRO_GRP_SLOTS);
    for idx in 0..total_slots {
        let val = names.get(idx).map(String::as_str).unwrap_or("");
        let _ = write!(text, "{idx}={val}\r\n");
    }
    super::encode_utf16le_bom(&text)
}

/// Parses `X806.exe`'s `config.ini` (comment-tolerant, mixed line endings, `[ShareData]` precedence).
pub fn parse_config_ini(bytes: &[u8]) -> Result<DeviceConfig, X806Error> {
    let text = super::decode_text_bytes(bytes);
    let sections = super::parse_ini_sections(&text, true);

    let has_known = sections.iter().any(|(name, _)| {
        matches!(
            name.as_str(),
            "Init" | "KEYMAP" | "Config0" | "Config1" | "Config2" | "Config3" | "ShareData"
        )
    });
    if !has_known {
        return Err(X806Error::IniParse(
            "no recognized X806 config.ini sections found".to_string(),
        ));
    }

    let mut cfg = defaults::factory_default_config();
    let mut os = LegacyOsSettings::default();

    if let Some((_, init_kvs)) = sections.iter().find(|(s, _)| s == "Init") {
        let mut names_native: [Option<String>; 4] = [None, None, None, None];
        let mut names_en: [Option<String>; 4] = [None, None, None, None];

        for (k, v) in init_kvs {
            match k.as_str() {
                "VER_SEL" => os.ver_sel = parse_int(k, v)?,
                "CurConfig" => {
                    let raw = parse_int::<u8>(k, v)?;
                    cfg.active_profile = match raw {
                        0 | 2 | 3 => raw,
                        _ => 0,
                    };
                }
                "ConfigName1" | "ConfigName2" | "ConfigName3" | "ConfigName4" => {
                    let idx = (k.as_bytes()[10] - b'1') as usize;
                    if !v.trim().is_empty() {
                        names_native[idx] = Some(v.trim().to_string());
                    }
                }
                "ConfigName1_EN" | "ConfigName2_EN" | "ConfigName3_EN" | "ConfigName4_EN" => {
                    let idx = (k.as_bytes()[10] - b'1') as usize;
                    if !v.trim().is_empty() {
                        names_en[idx] = Some(v.trim().to_string());
                    }
                }
                _ => {}
            }
        }

        for i in 0..4 {
            let chosen = if os.ver_sel != 0 {
                names_en[i].as_ref().or(names_native[i].as_ref())
            } else {
                names_native[i].as_ref().or(names_en[i].as_ref())
            };
            if let Some(name) = chosen {
                cfg.profiles[i].name = name.clone();
            }
        }
    }

    if let Some((_, keymap_kvs)) = sections.iter().find(|(s, _)| s == "KEYMAP") {
        for (k, v) in keymap_kvs {
            if let Some(idx) = parse_indexed_suffix(k, "K", "_MAP", 8) {
                cfg.keymap[idx - 1] = parse_int(k, v)?;
            }
        }
    }

    // Parse per-profile keys for Config0..Config3
    for p_idx in 0..4 {
        let sec_name = format!("Config{p_idx}");
        if let Some((_, kvs)) = sections.iter().find(|(s, _)| s == &sec_name) {
            apply_profile_kvs(&mut cfg.profiles[p_idx], kvs)?;
        }
    }

    // Apply shared fields from [Config0] first, then let [ShareData] override.
    if let Some((_, c0_kvs)) = sections.iter().find(|(s, _)| s == "Config0") {
        apply_shared_kvs(&mut cfg, &mut os, c0_kvs)?;
    }
    if let Some((_, share_kvs)) = sections.iter().find(|(s, _)| s == "ShareData") {
        apply_shared_kvs(&mut cfg, &mut os, share_kvs)?;
    }

    cfg.legacy_os_settings = Some(os);
    Ok(cfg)
}

/// Serializes a `DeviceConfig` to `X806.exe`-compatible `config.ini` text (`\r\n` line endings).
pub fn encode_config_ini(cfg: &DeviceConfig) -> String {
    let os = cfg.legacy_os_settings.unwrap_or_default();
    let mut out = String::new();

    out.push_str("[Init]\r\n");
    let _ = writeln!(out, "VER_SEL={}\r", os.ver_sel);
    let _ = writeln!(out, "CurConfig={}\r", cfg.active_profile.min(3));
    out.push_str("ConfigNum=4\r\n");
    for (i, p) in cfg.profiles.iter().enumerate() {
        let _ = writeln!(out, "ConfigName{}={}\r", i + 1, p.name);
    }
    for (i, p) in cfg.profiles.iter().enumerate() {
        let _ = writeln!(out, "ConfigName{}_EN={}\r", i + 1, p.name);
    }

    out.push_str("\r\n[PRODUCTNAME]\r\nPRODUCTNAME_CN=X806\r\nPRODUCTNAME_EN=X806\r\n");
    out.push_str("\r\n[KEYNUM]\r\nKeyNum=8\r\n");

    out.push_str("\r\n[KEYMAP]\r\n");
    for (i, slot) in cfg.keymap.iter().enumerate() {
        let _ = writeln!(out, "K{}_MAP={slot}\r", i + 1);
    }
    let _ = writeln!(out, "K9_MAP=9\r");

    for (p_idx, prof) in cfg.profiles.iter().enumerate() {
        let _ = writeln!(out, "\r\n[Config{p_idx}]\r");
        for (k, act) in prof.keys.iter().enumerate() {
            let _ = writeln!(out, "K{k}={}\r", format_key_action(*act));
        }
        let _ = writeln!(out, "WHEEL_STATUS={}\r", prof.wheel_status);
        for (k, en) in prof.led_react_en.iter().enumerate() {
            let _ = writeln!(out, "LED_REACT_EN{k}={}\r", u8::from(*en));
        }
        for (k, st) in prof.led_react_type.iter().enumerate() {
            let _ = writeln!(out, "LED_REACT_TYPE{k}={}\r", *st as u8);
        }
        for (k, lm) in prof.left_mac.iter().enumerate() {
            let _ = writeln!(out, "LEFT_MAC{k}={}\r", u8::from(*lm));
        }
    }

    out.push_str("\r\n[ShareData]\r\n");
    let _ = writeln!(out, "DPI_LEVEL={}\r", cfg.dpi_level);
    for (i, st) in cfg.dpi_stages.iter().enumerate() {
        let _ = writeln!(out, "DPI_SET{i}={st}\r");
    }
    for (i, en) in cfg.dpi_enable.iter().enumerate() {
        let _ = writeln!(out, "DPI_SET_EN{i}={}\r", u8::from(*en));
    }
    for (i, c) in cfg.led.dpi_colors.iter().enumerate() {
        let _ = writeln!(out, "DPI{i}_R={}\r", c.r);
        let _ = writeln!(out, "DPI{i}_G={}\r", c.g);
        let _ = writeln!(out, "DPI{i}_B={}\r", c.b);
    }
    for (i, sw) in cfg.led.type_sw_en.iter().enumerate() {
        let _ = writeln!(out, "LED_TYPE_SW_EN{i}={}\r", u8::from(*sw));
    }
    let _ = writeln!(out, "USB_SPEED={}\r", cfg.polling_rate.to_ini_code());
    let _ = writeln!(out, "LED_TYPE={}\r", cfg.led.mode.to_ini_code());
    let _ = writeln!(out, "CYC_7COL_EN={}\r", cfg.led.cycle_mask);
    let _ = writeln!(out, "LED_CYC_TIME={}\r", cfg.led.speed);
    for (i, c) in cfg.led.user_colors.iter().enumerate() {
        let _ = writeln!(out, "LED_USER{i}_R={}\r", c.r);
        let _ = writeln!(out, "LED_USER{i}_G={}\r", c.g);
        let _ = writeln!(out, "LED_USER{i}_B={}\r", c.b);
    }
    let _ = writeln!(
        out,
        "LED_USER_COLER_EN={}\r",
        u8::from(cfg.led.user_color_en)
    );
    let _ = writeln!(
        out,
        "LED_FEATURE_SCOLOR={}\r",
        u8::from(cfg.led.single_color_flag)
    );
    let _ = writeln!(out, "LED_FEATURE_SYMMETRY={}\r", u8::from(cfg.led.symmetry));
    let _ = writeln!(out, "LED_FEATURE_DIREC={}\r", cfg.led.direction);
    let _ = writeln!(out, "LED_BRIGHT={}\r", cfg.led.brightness);
    let _ = writeln!(out, "LED_CYCTIME={}\r", cfg.led.speed);
    let _ = writeln!(out, "FIRE_TIME={}\r", cfg.fire_time_ms);
    let _ = writeln!(out, "WHEEL_SPEED={}\r", os.wheel_speed);
    let _ = writeln!(out, "DPI_SPEED={}\r", os.dpi_speed);

    out
}

/// Imports a full `X806.exe` configuration bundle (`config.ini` + macro group `.dat`s + PUBG + built-ins).
pub fn import_x806_bundle(
    config_ini: &Path,
    macros_dir: Option<&Path>,
    pubg_dat: Option<&Path>,
    const_macro_dat: Option<&Path>,
) -> Result<DeviceConfig, X806Error> {
    let ini_bytes = std::fs::read(config_ini)?;
    let mut cfg = parse_config_ini(&ini_bytes)?;
    let ver_sel = cfg.legacy_os_settings.map(|o| o.ver_sel).unwrap_or(1);
    let base_dir = config_ini
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    // 1. User macro groups (`macroGrp.ini` + `<name>.dat`)
    let resolved_macros_dir: Option<PathBuf> = match macros_dir {
        Some(dir) => Some(dir.to_path_buf()),
        None => discover_macros_dir(base_dir, ver_sel),
    };
    if let Some(dir) = resolved_macros_dir {
        let grp_path = dir.join("macroGrp.ini");
        if grp_path.exists() || macros_dir.is_some() {
            let grp_bytes = std::fs::read(&grp_path)?;
            let group_names = parse_macro_groups(&grp_bytes)?;
            let mut groups = Vec::with_capacity(group_names.len());
            for name in group_names {
                validate_macro_group_name(&name)?;
                let macros = if name.trim().is_empty() {
                    Vec::new()
                } else {
                    let dat_path = dir.join(format!("{name}.dat"));
                    if dat_path.exists() {
                        let dat_bytes = std::fs::read(&dat_path)?;
                        dat::parse_dat(&dat_bytes)?
                    } else {
                        Vec::new()
                    }
                };
                groups.push(MacroGroup { name, macros });
            }
            cfg.macros.groups = groups;
        }
    }

    // 2. PUBG macros
    let resolved_pubg: Option<PathBuf> = match pubg_dat {
        Some(p) => Some(p.to_path_buf()),
        None => discover_pubg_dat(base_dir, ver_sel),
    };
    if let Some(path) = resolved_pubg {
        let bytes = std::fs::read(&path)?;
        cfg.macros.pubg_macros = dat::parse_dat(&bytes)?;
    }

    // 3. Built-in shortcuts (`constMacro.dat`)
    let resolved_const: Option<PathBuf> = match const_macro_dat {
        Some(p) => Some(p.to_path_buf()),
        None => discover_const_macro(base_dir),
    };
    if let Some(path) = resolved_const {
        let bytes = std::fs::read(&path)?;
        cfg.macros.builtin_shortcuts = dat::parse_dat(&bytes)?;
    }

    Ok(cfg)
}

/// Exports a `DeviceConfig` back to `X806.exe` `config.ini`, `macroGrp.ini`, and `<group>.dat` files.
pub fn export_x806_bundle(
    cfg: &DeviceConfig,
    config_ini: &Path,
    macros_dir: &Path,
) -> Result<(), X806Error> {
    for group in &cfg.macros.groups {
        validate_macro_group_name(&group.name)?;
    }

    let parent = config_ini
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if parent != Path::new(".") {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir_all(macros_dir)?;

    std::fs::write(config_ini, encode_config_ini(cfg))?;

    let names: Vec<String> = cfg.macros.groups.iter().map(|g| g.name.clone()).collect();
    std::fs::write(macros_dir.join("macroGrp.ini"), encode_macro_groups(&names))?;

    for group in &cfg.macros.groups {
        if !group.name.trim().is_empty() {
            let dat_text = dat::encode_dat(&group.macros, false);
            std::fs::write(macros_dir.join(format!("{}.dat", group.name)), dat_text)?;
        }
    }

    if !cfg.macros.pubg_macros.is_empty() {
        std::fs::write(
            parent.join("PUBG_EN.dat"),
            dat::encode_dat(&cfg.macros.pubg_macros, false),
        )?;
    }
    if !cfg.macros.builtin_shortcuts.is_empty() {
        std::fs::write(
            parent.join("constMacro.dat"),
            dat::encode_dat(&cfg.macros.builtin_shortcuts, false),
        )?;
    }

    Ok(())
}

fn validate_macro_group_name(name: &str) -> Result<(), X806Error> {
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
        return Err(X806Error::IniParse(format!(
            "invalid macro group name '{name}': path separators and traversal segments are not allowed"
        )));
    }
    let mut comps = Path::new(name).components();
    if !matches!(
        (comps.next(), comps.next()),
        (Some(std::path::Component::Normal(_)), None)
    ) {
        return Err(X806Error::IniParse(format!(
            "invalid macro group name '{name}': must be a single relative filename"
        )));
    }
    Ok(())
}

fn discover_macros_dir(base_dir: &Path, ver_sel: u32) -> Option<PathBuf> {
    let mut candidates = vec![base_dir.to_path_buf(), base_dir.join("macro")];
    if let Some(parent) = base_dir.parent() {
        let primary = if ver_sel == 0 { "INI_CN" } else { "INI_EN" };
        let fallback = if ver_sel == 0 { "INI_EN" } else { "INI_CN" };
        candidates.push(parent.join(primary).join("macro"));
        candidates.push(parent.join(fallback).join("macro"));
    }
    candidates
        .into_iter()
        .find(|dir| dir.join("macroGrp.ini").exists())
}

fn discover_pubg_dat(base_dir: &Path, ver_sel: u32) -> Option<PathBuf> {
    let (primary, fallback) = if ver_sel == 0 {
        ("PUBG_CN.dat", "PUBG_EN.dat")
    } else {
        ("PUBG_EN.dat", "PUBG_CN.dat")
    };
    [base_dir.join(primary), base_dir.join(fallback)]
        .into_iter()
        .find(|p| p.exists())
}

fn discover_const_macro(base_dir: &Path) -> Option<PathBuf> {
    let mut candidates = vec![base_dir.join("constMacro.dat")];
    if let Some(p1) = base_dir.parent() {
        candidates.push(p1.join("constMacro.dat"));
        if let Some(p2) = p1.parent() {
            candidates.push(p2.join("constMacro.dat"));
        }
    }
    candidates.into_iter().find(|p| p.exists())
}

fn apply_profile_kvs(
    prof: &mut crate::model::ProfileConfig,
    kvs: &[(String, String)],
) -> Result<(), X806Error> {
    for (k, v) in kvs {
        if let Some(idx) = parse_indexed(k, "K", 8) {
            prof.keys[idx] = parse_key_action(v)?;
        } else if let Some(idx) = parse_indexed(k, "LEFT_MAC", 8) {
            prof.left_mac[idx] = parse_int::<i32>(k, v)? != 0;
        } else if let Some(idx) = parse_indexed(k, "LED_REACT_EN", 8) {
            prof.led_react_en[idx] = parse_int::<i32>(k, v)? != 0;
        } else if let Some(idx) = parse_indexed(k, "LED_REACT_TYPE", 8) {
            prof.led_react_type[idx] = ReactiveStyle::from_u8(parse_int::<u8>(k, v)?);
        } else if k == "WHEEL_STATUS" {
            prof.wheel_status = parse_int(k, v)?;
        }
    }
    Ok(())
}

fn apply_shared_kvs(
    cfg: &mut DeviceConfig,
    os: &mut LegacyOsSettings,
    kvs: &[(String, String)],
) -> Result<(), X806Error> {
    for (k, v) in kvs {
        match k.as_str() {
            "DPI_LEVEL" => cfg.dpi_level = parse_int::<u8>(k, v)?.min(5),
            "LED_TYPE" => cfg.led.mode = LedMode::from_ini_code(parse_int(k, v)?),
            "LED_CYCTIME" | "LED_CYC_TIME" => cfg.led.speed = parse_int(k, v)?,
            "LED_USER_COLER_EN" => cfg.led.user_color_en = parse_int::<i32>(k, v)? != 0,
            "LED_FEATURE_SCOLOR" => cfg.led.single_color_flag = parse_int::<i32>(k, v)? != 0,
            "LED_FEATURE_SYMMETRY" => cfg.led.symmetry = parse_int::<i32>(k, v)? != 0,
            "LED_FEATURE_DIREC" => cfg.led.direction = parse_int(k, v)?,
            "LED_BRIGHT" => cfg.led.brightness = parse_int(k, v)?,
            "CYC_7COL_EN" => cfg.led.cycle_mask = parse_int(k, v)?,
            "USB_SPEED" => cfg.polling_rate = PollingRate::from_ini_code(parse_int(k, v)?),
            "FIRE_TIME" => cfg.fire_time_ms = parse_int(k, v)?,
            "DPI_SPEED" => os.dpi_speed = parse_int(k, v)?,
            "WHEEL_SPEED" => os.wheel_speed = parse_int(k, v)?,
            _ => {
                if let Some(i) = parse_indexed(k, "DPI_SET", 6) {
                    cfg.dpi_stages[i] = parse_int(k, v)?;
                } else if let Some(i) = parse_indexed(k, "DPI_SET_EN", 6) {
                    cfg.dpi_enable[i] = parse_int::<i32>(k, v)? != 0;
                } else if let Some(i) = parse_indexed(k, "LED_TYPE_SW_EN", 10) {
                    cfg.led.type_sw_en[i] = parse_int::<i32>(k, v)? != 0;
                } else if let Some((i, ch)) = parse_rgb_key(k, "DPI", 6) {
                    let c = parse_int::<u8>(k, v)?;
                    match ch {
                        'R' => cfg.led.dpi_colors[i].r = c,
                        'G' => cfg.led.dpi_colors[i].g = c,
                        'B' => cfg.led.dpi_colors[i].b = c,
                        _ => {}
                    }
                } else if let Some((i, ch)) = parse_rgb_key(k, "LED_USER", 7) {
                    let c = parse_int::<u8>(k, v)?;
                    match ch {
                        'R' => cfg.led.user_colors[i].r = c,
                        'G' => cfg.led.user_colors[i].g = c,
                        'B' => cfg.led.user_colors[i].b = c,
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(())
}

fn parse_key_action(val: &str) -> Result<KeyAction, X806Error> {
    if let Some((lhs_str, rhs_str)) = val.split_once(',') {
        let lhs = parse_int::<u32>("K*", lhs_str.trim())?;
        let rhs = parse_int::<u8>("K*,hid", rhs_str.trim())?;
        if lhs == 42 {
            return Ok(KeyAction::SingleKey { hid: rhs });
        }
        return Ok(KeyAction::RawLegacy(lhs));
    }

    let id = parse_int::<u32>("K*", val.trim())?;
    let action = match id {
        0 => KeyAction::Disabled,
        1 => KeyAction::LeftButton,
        2 => KeyAction::MidButton,
        3 => KeyAction::RightButton,
        4 => KeyAction::Backward,
        5 => KeyAction::Forward,
        6 => KeyAction::DpiLoop,
        12 => KeyAction::ShowDesktop,
        13 => KeyAction::DoubleClick,
        14 => KeyAction::Fire,
        15 => KeyAction::LedLoop,
        16 => KeyAction::DpiUp,
        17 => KeyAction::DpiDown,
        18 => KeyAction::FnKey,
        19 => KeyAction::ProfileGame1,
        20 => KeyAction::ProfileGame2,
        21 => KeyAction::ProfileGame3,
        23..=32 => {
            KeyAction::Media(MediaKey::from_ini_action_id(id).expect("23..=32 is valid MediaKey"))
        }
        33..=40 => KeyAction::Shortcut(
            BuiltinShortcut::from_ini_action_id(id).expect("33..=40 is valid BuiltinShortcut"),
        ),
        50..=255 => KeyAction::PubgMacro {
            index: (id - 50) as u8,
        },
        256..=65535 => KeyAction::UserMacro {
            group: ((id >> 8) - 1) as u8,
            index: (id & 0xFF) as u8,
        },
        other => KeyAction::RawLegacy(other),
    };
    Ok(action)
}

fn format_key_action(action: KeyAction) -> String {
    match action {
        KeyAction::Disabled => "0".to_string(),
        KeyAction::LeftButton => "1".to_string(),
        KeyAction::MidButton => "2".to_string(),
        KeyAction::RightButton => "3".to_string(),
        KeyAction::Backward => "4".to_string(),
        KeyAction::Forward => "5".to_string(),
        KeyAction::DpiLoop => "6".to_string(),
        KeyAction::ShowDesktop => "12".to_string(),
        KeyAction::DoubleClick => "13".to_string(),
        KeyAction::Fire => "14".to_string(),
        KeyAction::LedLoop => "15".to_string(),
        KeyAction::DpiUp => "16".to_string(),
        KeyAction::DpiDown => "17".to_string(),
        KeyAction::FnKey => "18".to_string(),
        KeyAction::ProfileGame1 => "19".to_string(),
        KeyAction::ProfileGame2 => "20".to_string(),
        KeyAction::ProfileGame3 => "21".to_string(),
        KeyAction::Media(m) => m.ini_action_id().to_string(),
        KeyAction::Shortcut(s) => s.ini_action_id().to_string(),
        KeyAction::SingleKey { hid } => format!("42,{hid}"),
        KeyAction::PubgMacro { index } => (50 + u32::from(index)).to_string(),
        KeyAction::UserMacro { group, index } => {
            (((u32::from(group) + 1) << 8) | u32::from(index)).to_string()
        }
        KeyAction::RawLegacy(v) => v.to_string(),
    }
}

fn parse_int<T: std::str::FromStr>(key: &str, val: &str) -> Result<T, X806Error> {
    val.trim()
        .parse::<T>()
        .map_err(|_| X806Error::IniParse(format!("invalid integer for {key}: {val}")))
}

fn parse_indexed(key: &str, prefix: &str, limit: usize) -> Option<usize> {
    let rest = key.strip_prefix(prefix)?;
    let idx = rest.parse::<usize>().ok()?;
    (idx < limit).then_some(idx)
}

fn parse_indexed_suffix(key: &str, prefix: &str, suffix: &str, max_one_based: usize) -> Option<usize> {
    let rest = key.strip_prefix(prefix)?.strip_suffix(suffix)?;
    let idx = rest.parse::<usize>().ok()?;
    (1..=max_one_based).contains(&idx).then_some(idx)
}

fn parse_rgb_key(key: &str, prefix: &str, limit: usize) -> Option<(usize, char)> {
    let rest = key.strip_prefix(prefix)?;
    let (num_str, ch_str) = rest.split_once('_')?;
    let idx = num_str.parse::<usize>().ok()?;
    if idx >= limit || ch_str.len() != 1 {
        return None;
    }
    let ch = ch_str.chars().next()?;
    matches!(ch, 'R' | 'G' | 'B').then_some((idx, ch))
}
