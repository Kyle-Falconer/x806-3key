pub mod dat;
pub mod ini;
pub mod toml;

/// Decodes raw file bytes as UTF-16LE (if BOM `0xFF 0xFE` is present), UTF-8 (stripping optional
/// UTF-8 BOM), or lossy UTF-8 for legacy ANSI/GBK comments.
pub(crate) fn decode_text_bytes(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(&bytes[3..]).into_owned()
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

/// Encodes text as UTF-16LE with a leading BOM (`0xFF 0xFE`).
pub(crate) fn encode_utf16le_bom(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + text.len() * 2);
    out.extend_from_slice(&[0xFF, 0xFE]);
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}

/// Normalizes CRLF/CR line endings to LF and parses INI `[Section]` + `key=value` entries.
pub(crate) fn parse_ini_sections(
    text: &str,
    strip_double_slash: bool,
) -> Vec<(String, Vec<(String, String)>)> {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut current_idx: Option<usize> = None;

    for raw_line in normalized.split('\n') {
        let without_comment = if strip_double_slash {
            raw_line.split("//").next().unwrap_or("")
        } else {
            raw_line
        };
        let line = without_comment.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
            let name = line[1..line.len() - 1].trim().to_string();
            if let Some(pos) = sections.iter().position(|(s, _)| s == &name) {
                current_idx = Some(pos);
            } else {
                sections.push((name, Vec::new()));
                current_idx = Some(sections.len() - 1);
            }
            continue;
        }
        if let Some((k, v)) = line.split_once('=')
            && let Some(idx) = current_idx
        {
            sections[idx]
                .1
                .push((k.trim().to_string(), v.trim().to_string()));
        }
    }

    sections
}
