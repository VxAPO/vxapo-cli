use winreg::enums::*;
use winreg::RegKey;

pub fn read_reg_sz(key: &RegKey, value_name: &str) -> Option<String> {
    let rv = key.get_raw_value(value_name).ok()?;
    match rv.vtype {
        REG_SZ | REG_EXPAND_SZ => parse_utf16_bytes(&rv.bytes),
        REG_BINARY => parse_prop_string(&rv.bytes),
        _ => None,
    }
}

pub fn parse_utf16_bytes(data: &[u8]) -> Option<String> {
    if data.len() < 4 {
        return None;
    }
    let mut chars = Vec::new();
    let mut i = 0;
    while i + 1 < data.len() {
        let c = u16::from_le_bytes([data[i], data[i + 1]]);
        if c == 0 {
            break;
        }
        chars.push(c);
        i += 2;
    }
    if chars.is_empty() {
        return None;
    }
    String::from_utf16(&chars).ok()
}

pub fn parse_multi_utf16(data: &[u8]) -> Option<String> {
    let mut strings = Vec::new();
    let mut chars = Vec::new();
    let mut i = 0;
    while i + 1 < data.len() {
        let c = u16::from_le_bytes([data[i], data[i + 1]]);
        i += 2;
        if c == 0 {
            if !chars.is_empty() {
                if let Ok(s) = String::from_utf16(&chars) {
                    strings.push(s);
                }
                chars.clear();
            }
            if i + 1 < data.len() {
                let next = u16::from_le_bytes([data[i], data[i + 1]]);
                if next == 0 {
                    break;
                }
            }
        } else {
            chars.push(c);
        }
    }
    if strings.is_empty() {
        None
    } else {
        Some(strings.join(" | "))
    }
}

pub fn parse_prop_string(data: &[u8]) -> Option<String> {
    if data.len() < 8 {
        return None;
    }
    let vt = u16::from_le_bytes([data[0], data[1]]);
    match vt {
        31 => {
            let byte_count = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
            if byte_count < 2 || data.len() < 8 + byte_count {
                return None;
            }
            let num_chars = byte_count / 2 - 1;
            let chars: Vec<u16> = (0..num_chars)
                .map(|i| {
                    let off = 8 + i * 2;
                    u16::from_le_bytes([data[off], data[off + 1]])
                })
                .collect();
            String::from_utf16(&chars).ok()
        }
        30 => {
            let byte_count = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
            if byte_count < 1 || data.len() < 8 + byte_count {
                return None;
            }
            let end = if data[8 + byte_count - 1] == 0 {
                byte_count - 1
            } else {
                byte_count
            };
            String::from_utf8(data[8..8 + end].to_vec()).ok()
        }
        _ => None,
    }
}

pub fn parse_waveformatex(data: &[u8]) -> Option<(Option<u32>, Option<u16>, Option<u16>, Option<u32>)> {
    if let Some(result) = try_parse_wfx(data, 0) {
        return Some(result);
    }
    if let Some(result) = try_parse_wfx(data, 8) {
        return Some(result);
    }
    None
}

fn try_parse_wfx(data: &[u8], offset: usize) -> Option<(Option<u32>, Option<u16>, Option<u16>, Option<u32>)> {
    if data.len() < offset + 16 {
        return None;
    }
    let d = &data[offset..];
    let format_tag = u16::from_le_bytes([d[0], d[1]]);
    let channels = u16::from_le_bytes([d[2], d[3]]);
    let sample_rate = u32::from_le_bytes([d[4], d[5], d[6], d[7]]);
    let bits_per_sample = u16::from_le_bytes([d[14], d[15]]);

    if channels == 0 || channels > 256 {
        return None;
    }
    if sample_rate == 0 || sample_rate > 1_000_000 {
        return None;
    }
    if bits_per_sample == 0 || bits_per_sample > 64 {
        return None;
    }

    let mut channel_mask: Option<u32> = None;
    if format_tag == 0xFFFE && d.len() >= 24 {
        channel_mask = Some(u32::from_le_bytes([d[20], d[21], d[22], d[23]]));
    }

    Some((Some(sample_rate), Some(channels), Some(bits_per_sample), channel_mask))
}

pub const VAL_NAME_INTERFACE: &str = "{a45c254e-df1c-4efd-8020-67d146a850e0},2";
pub const VAL_NAME_PRODUCT: &str = "{b3f8fa53-0004-438e-9003-51a46e139bfc},6";
pub const VAL_SFX: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},5";
pub const VAL_MFX: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},6";
pub const VAL_EFX: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},7";
pub const VAL_FLAGS: &str = "{b3f8fa53-0004-438e-9003-51a46e139bfc},9";
