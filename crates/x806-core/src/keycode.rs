/// Table of `(windows_vk, usb_hid_usage, display_name)` for non-contiguous keys.
/// Entries with `windows_vk == 0` are HID-only display mappings.
const SPECIAL_KEYS: &[(u8, u8, &str)] = &[
    // Digit '0'
    (0x30, 0x27, "0"),
    // Control / Edit
    (0x0D, 0x28, "RETURN"),
    (0x1B, 0x29, "ESCAPE"),
    (0x08, 0x2A, "BACKSPACE"),
    (0x09, 0x2B, "TAB"),
    (0x20, 0x2C, "SPACE"),
    (0x14, 0x39, "CAPITAL"),
    // OEM Punctuation (VK_OEM_3 0xC0 -> HID 0x35)
    (0xBD, 0x2D, "-"),
    (0xBB, 0x2E, "+"),
    (0xDB, 0x2F, "[{"),
    (0xDD, 0x30, "]}"),
    (0xDC, 0x31, "\\|"),
    (0xBA, 0x33, ";:"),
    (0xDE, 0x34, "'"),
    (0xC0, 0x35, "`~"),
    (0xBC, 0x36, ","),
    (0xBE, 0x37, "."),
    (0xBF, 0x38, "/?"),
    // Navigation & Lock (0x46..=0x53)
    (0x2C, 0x46, "SNAPSHOT"),
    (0x91, 0x47, "SCROLL"),
    (0x13, 0x48, "PAUSE"),
    (0x2D, 0x49, "INSERT"),
    (0x24, 0x4A, "HOME"),
    (0x21, 0x4B, "PRIOR"),
    (0x2E, 0x4C, "DELETE"),
    (0x23, 0x4D, "END"),
    (0x22, 0x4E, "NEXT"),
    (0x27, 0x4F, "RIGHT"),
    (0x25, 0x50, "LEFT"),
    (0x28, 0x51, "DOWN"),
    (0x26, 0x52, "UP"),
    (0x90, 0x53, "NUMLOCK"),
    // Numpad
    (0x6F, 0x54, "DIVIDE"),
    (0x6A, 0x55, "MULTIPLY"),
    (0x6D, 0x56, "SUBTRACT"),
    (0x6B, 0x57, "ADD"),
    (0x60, 0x62, "NUMPAD0"),
    (0x6E, 0x63, "DECIMAL"),
    (0x5D, 0x65, "APPS"),
    // Special keys
    (0x2B, 0x74, "EXECUTE"),
    (0x2F, 0x75, "HELP"),
    (0x00, 0x76, "MENU"),
    (0x29, 0x77, "SELECT"),
    (0x0C, 0x9C, "CLEAR"),
    (0x6C, 0x9F, "SEPARATOR"),
    // Modifiers (0xE0..=0xE7)
    (0xA2, 0xE0, "LCONTROL"),
    (0xA0, 0xE1, "LSHIFT"),
    (0xA4, 0xE2, "LMENU"),
    (0x5B, 0xE3, "LWIN"),
    (0xA3, 0xE4, "RCONTROL"),
    (0xA1, 0xE5, "RSHIFT"),
    (0xA5, 0xE6, "RMENU"),
    (0x5C, 0xE7, "RWIN"),
    // Mouse pseudo-VKs (FUN_0042e1c0 / FUN_00410b10)
    (0xF0, 0xF1, "L-KEY"),
    (0xF1, 0xF3, "R-KEY"),
    (0xF2, 0xF2, "M-KEY"),
];

/// Converts a Windows Virtual-Key (VK) code to a USB HID Usage ID (Page 0x07 or mouse pseudo-code).
/// Returns `0` for unmapped VK codes.
pub fn vk_to_hid(vk: u8) -> u8 {
    match vk {
        0 => 0,
        0x41..=0x5A => vk - 0x3D, // 'A'..='Z' -> 0x04..=0x1D
        0x31..=0x39 => vk - 0x13, // '1'..='9' -> 0x1E..=0x26
        0x70..=0x7B => vk - 0x36, // F1..=F12 -> 0x3A..=0x45
        0x7C..=0x87 => vk - 0x14, // F13..=F24 -> 0x68..=0x73
        0x61..=0x69 => vk - 0x08, // Numpad 1..=9 -> 0x59..=0x61
        _ => SPECIAL_KEYS
            .iter()
            .find(|&&(entry_vk, _, _)| entry_vk == vk)
            .map_or(0, |&(_, hid, _)| hid),
    }
}

/// Converts a USB HID Usage ID (or mouse pseudo-code `0xF1..=0xF3`) to a Windows Virtual-Key (VK) code.
/// Returns `0` for unmapped HID codes.
pub fn hid_to_vk(hid: u8) -> u8 {
    match hid {
        0 => 0,
        0x04..=0x1D => hid + 0x3D, // 'A'..='Z'
        0x1E..=0x26 => hid + 0x13, // '1'..='9'
        0x3A..=0x45 => hid + 0x36, // F1..=F12
        0x68..=0x73 => hid + 0x14, // F13..=F24
        0x59..=0x61 => hid + 0x08, // Numpad 1..=9
        _ => SPECIAL_KEYS
            .iter()
            .find(|&&(_, entry_hid, _)| entry_hid == hid)
            .map_or(0, |&(vk, _, _)| vk),
    }
}

/// Returns the human-readable macro display name for a USB HID Usage ID (or mouse pseudo-code).
pub fn hid_display_name(hid: u8) -> String {
    match hid {
        0x04..=0x1D => char::from(b'A' + (hid - 0x04)).to_string(),
        0x1E..=0x26 => char::from(b'1' + (hid - 0x1E)).to_string(),
        0x3A..=0x45 => format!("F{}", hid - 0x3A + 1),
        0x68..=0x73 => format!("F{}", hid - 0x68 + 13),
        0x59..=0x61 => format!("NUMPAD{}", hid - 0x59 + 1),
        _ => SPECIAL_KEYS
            .iter()
            .find(|&&(_, entry_hid, _)| entry_hid == hid)
            .map_or_else(|| format!("0x{hid:02X}"), |&(_, _, name)| name.to_string()),
    }
}
