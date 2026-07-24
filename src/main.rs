use std::io::{self, Write};
use winreg::enums::*;
use winreg::RegKey;

// ── ANSI 颜色 ──
const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const MAGENTA: &str = "\x1b[35m";
const BLUE: &str = "\x1b[34m";
const RED: &str = "\x1b[31m";
const GRAY: &str = "\x1b[90m";

// ── 已知的 Windows 系统默认 APO CLSID ──
const SYSTEM_APO_CLSIDS: &[&str] = &[
    // SFX / PreMix
    "{da2c9ece-7418-4906-b4fa-0a00b3eb88aa}",
    "{c9453e73-8c5c-4463-9984-af8bab2f5447}",
    "{7ab03736-d528-4e73-905a-7e5e7f3b0b5c}",
    // MFX / PostMix
    "{a29eb043-6ce2-4ee2-b38c-f58719e0d88f}",
    "{ab3b404a-b18f-4b4f-b91f-77f2de95eb18}",
    // EFX / Endpoint
    "{5860e1c5-f95c-4a7a-8ec8-8aef24f379a1}",
    // offload
    "{a296d363-ee83-4af9-9be7-729c1296150a}",
    "{a69c91dc-11c4-414f-a919-4da8ea3f3ca6}",
    "{13ab3ebd-137e-4903-9d89-60be8277fd17}",
    // 路由
    "{6861cfdc-0461-49d5-a8df-be5acd02692f}",
    "{5860e1c5-f95c-4a7a-8ec8-8aef24f379a1}",
    // null GUID（占位）
    "{00000000-0000-0000-0000-000000000000}",
];

fn is_system_apo(clsid: &str) -> bool {
    let lower = clsid.to_lowercase();
    SYSTEM_APO_CLSIDS.iter().any(|s| *s == lower)
}

// ── 注册表路径 ──
const PATH_RENDER: &str =
    "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Render";
const PATH_CAPTURE: &str =
    "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Capture";

// ── 有意义的属性值名前缀 ──
const KNOWN_PREFIXES: &[&str] = &[
    "{a45c254e",  // 设备接口名
    "{b3f8fa53",  // 设备属性
    "{d04e05a6",  // APO FX 绑定
    "{80f111c3",  // 硬件 ID
    "{9c119480",  // 设备路径
    "{1da5d803",  // 设备类别
    "{233164c8",  // 设备拓扑
];

// ── 属性值名 ──
const VAL_NAME_INTERFACE: &str = "{a45c254e-df1c-4efd-8020-67d146a850e0},2";
const VAL_NAME_PRODUCT: &str = "{b3f8fa53-0004-438e-9003-51a46e139bfc},6";
const VAL_SFX: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},5";
const VAL_MFX: &str = "{d04e05a6-594b-4fb6-a80d-01af5eed7d1d},7";
const VAL_FLAGS: &str = "{b3f8fa53-0004-438e-9003-51a46e139bfc},9";

#[derive(Debug, Clone)]
struct Endpoint {
    index: usize,
    guid: String,
    name: String,
    kind: String,
    sfx: Option<String>,
    mfx: Option<String>,
    flags: Option<u32>,
    sfx_is_system: bool,
    mfx_is_system: bool,
}

fn main() {
    println!("VxAPO Control v0.1.0");
    println!("====================\n");

    let mut endpoints = enumerate_all();

    if endpoints.is_empty() {
        println!("No audio endpoints found.");
        return;
    }

    print_endpoints(&endpoints);

    loop {
        println!("\n--- Commands ---");
        println!("[0-{}]  Select endpoint", endpoints.len() - 1);
        println!("[r]    Refresh");
        println!("[q]    Quit");
        print!("> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        let input = input.trim();

        match input {
            "q" | "quit" | "exit" => break,
            "r" | "refresh" => {
                endpoints = enumerate_all();
                print_endpoints(&endpoints);
            }
            n => {
                if let Ok(idx) = n.parse::<usize>() {
                    if idx < endpoints.len() {
                        show_detail(&endpoints[idx]);
                        // 返回后重印设备列表
                        print_endpoints(&endpoints);
                    } else {
                        println!("Index out of range.");
                    }
                } else {
                    println!("Unknown command.");
                }
            }
        }
    }
}

fn print_endpoints(endpoints: &[Endpoint]) {
    println!("\n{BOLD}Found {} endpoints:{RESET}\n", endpoints.len());
    for ep in endpoints {
        let kind_color = if ep.kind == "Playback" { CYAN } else { GREEN };
        let kind_display = format!("{GRAY}({kind_color}{}{GRAY}){RESET}", ep.kind);
        let (apo_text, apo_color) = classify_apo(ep);

        println!(
            "  {BOLD}[{}]{RESET} {} {kind_display} {apo_color}{}{RESET}",
            ep.index, ep.name, apo_text,
        );
    }
}

fn classify_apo(ep: &Endpoint) -> (String, &'static str) {
    match (&ep.sfx, &ep.mfx, ep.sfx_is_system, ep.mfx_is_system) {
        (Some(_), Some(_), true, true)   => ("[Windows default]".into(), GRAY),
        (Some(_), None, true, _)         => ("[Windows default]".into(), GRAY),
        (None, Some(_), _, true)         => ("[Windows default]".into(), GRAY),
        (Some(_), Some(_), false, false) => ("[SFX + MFX]".into(), YELLOW),
        (Some(_), Some(_), true, false)  => ("[MFX]".into(), YELLOW),
        (Some(_), Some(_), false, true)  => ("[SFX]".into(), YELLOW),
        (Some(_), None, false, _)        => ("[SFX only]".into(), MAGENTA),
        (None, Some(_), _, false)        => ("[MFX only]".into(), BLUE),
        (None, None, _, _)               => ("[none]".into(), GRAY),
    }
}

// ============================================================
// 详情
// ============================================================

fn show_detail(ep: &Endpoint) {
    let kind_color = if ep.kind == "Playback" { CYAN } else { GREEN };
    println!("\n{BOLD}========================================{RESET}");
    println!(" {BOLD}{}{RESET}", ep.name);
    println!("{BOLD}========================================{RESET}");
    println!("  Index:    {BOLD}{}{RESET}", ep.index);
    println!("  GUID:     {GRAY}{}{RESET}", ep.guid);
    println!("  Type:     {kind_color}{}{RESET}", ep.kind);

    fn apo_display(val: &Option<String>, is_system: bool) -> String {
        match val {
            Some(clsid) if is_system => format!("{GRAY}{clsid} (system){RESET}"),
            Some(clsid) => format!("{YELLOW}{clsid}{RESET}"),
            None => format!("{GRAY}(none){RESET}"),
        }
    }

    println!("  SFX APO:  {}", apo_display(&ep.sfx, ep.sfx_is_system));
    println!("  MFX APO:  {}", apo_display(&ep.mfx, ep.mfx_is_system));
    println!(
        "  Flags:    {}",
        match ep.flags {
            Some(0) => format!("{RED}0x00{RESET} {GRAY}(system effects DISABLED){RESET}"),
            Some(f) => format!("{YELLOW}0x{f:02x}{RESET} {GRAY}(system effects ACTIVE){RESET}"),
            None => format!("{GRAY}(not set){RESET}"),
        }
    );
    println!("{BOLD}========================================{RESET}");

    loop {
        println!("\n  {CYAN}[x]{RESET} View registry  {GRAY}[b]{RESET} Back");
        print!("  > ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();

        match input.trim() {
            "x" => dump_registry(&ep.guid, &ep.kind),
            "b" | "q" | "" => break,
            _ => {}
        }
    }
}

// ============================================================
// 注册表 dump（只显示有意义的值）
// ============================================================

fn dump_registry(guid: &str, kind: &str) {
    let base = if kind == "Playback" { PATH_RENDER } else { PATH_CAPTURE };
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    // 端点主键
    println!("\n  [{kind}] {guid}");
    dump_key_filtered(&hklm, &format!("{base}\\{guid}"), "  ");

    // Properties 子键
    let props_path = format!("{base}\\{guid}\\Properties");
    if let Ok(props) = hklm.open_subkey_with_flags(&props_path, KEY_READ) {
        println!("  ── Properties ──");
        dump_key_inner(&props, "  ", true);
    }

    // FxProperties 子键
    let fx_path = format!("{base}\\{guid}\\FxProperties");
    if let Ok(fx) = hklm.open_subkey_with_flags(&fx_path, KEY_READ) {
        println!("  ── FxProperties ──");
        dump_key_inner(&fx, "  ", true);
    }
}

fn dump_key_filtered(hklm: &RegKey, path: &str, indent: &str) {
    let key = match hklm.open_subkey_with_flags(path, KEY_READ) {
        Ok(k) => k,
        Err(_) => {
            println!("{indent}(not found)");
            return;
        }
    };
    dump_key_inner(&key, indent, false);
}

fn dump_key_inner(key: &RegKey, indent: &str, filter: bool) {
    for result in key.enum_values() {
        let (name, val) = match result {
            Ok(v) => v,
            Err(_) => continue,
        };

        // 过滤：只显示已知属性组
        if filter && !is_known_property(&name) {
            continue;
        }

        match val.vtype {
            REG_SZ | REG_EXPAND_SZ => {
                if let Some(s) = parse_utf16_bytes(&val.bytes) {
                    println!("{indent}[SZ]  {name} = {s}");
                }
            }
            REG_DWORD => {
                if val.bytes.len() >= 4 {
                    let d = u32::from_le_bytes([val.bytes[0], val.bytes[1], val.bytes[2], val.bytes[3]]);
                    // 对标志位特别标记
                    if name == VAL_FLAGS {
                        let label = if d == 0 { "DISABLED" } else { "ACTIVE" };
                        println!("{indent}[DW]  {name} = 0x{d:08x} ({label})");
                    } else {
                        println!("{indent}[DW]  {name} = 0x{d:08x}");
                    }
                }
            }
            REG_BINARY => {
                // 只显示 APO 相关的，尝试解析 PROPVARIANT
                if name.starts_with("{d04e05a6") {
                    if let Some(s) = parse_prop_string(&val.bytes) {
                        println!("{indent}[BIN] {name} = {s}");
                    } else {
                        println!("{indent}[BIN] {name} = {} bytes", val.bytes.len());
                    }
                } else if filter {
                    // 过滤模式下跳过非 APO 的二进制
                    continue;
                } else {
                    let preview: Vec<String> = val.bytes.iter().take(8).map(|b| format!("{b:02x}")).collect();
                    println!("{indent}[BIN] {name} = [{}...] ({} bytes)", preview.join(" "), val.bytes.len());
                }
            }
            REG_MULTI_SZ => {
                if let Some(s) = parse_multi_utf16(&val.bytes) {
                    println!("{indent}[MSZ] {name} = {s}");
                }
            }
            REG_QWORD => {
                if val.bytes.len() >= 8 {
                    let q = u64::from_le_bytes([
                        val.bytes[0], val.bytes[1], val.bytes[2], val.bytes[3],
                        val.bytes[4], val.bytes[5], val.bytes[6], val.bytes[7],
                    ]);
                    println!("{indent}[QW]  {name} = 0x{q:016x} ({q})");
                }
            }
            other => {
                if !filter {
                    println!("{indent}[??]  {name} = ({other:?}, {} bytes)", val.bytes.len());
                }
            }
        }
    }
}

fn is_known_property(name: &str) -> bool {
    KNOWN_PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

// ============================================================
// 枚举
// ============================================================

fn enumerate_all() -> Vec<Endpoint> {
    let mut endpoints = Vec::new();
    let mut idx = 0;
    enumerate_path(PATH_RENDER, "Playback", &mut endpoints, &mut idx);
    enumerate_path(PATH_CAPTURE, "Capture", &mut endpoints, &mut idx);
    endpoints
}

fn enumerate_path(base: &str, kind: &str, out: &mut Vec<Endpoint>, idx: &mut usize) {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = match hklm.open_subkey_with_flags(base, KEY_READ) {
        Ok(k) => k,
        Err(_) => return,
    };

    let mut seen = std::collections::HashSet::new();

    for guid_result in key.enum_keys() {
        let guid = match guid_result {
            Ok(g) => g,
            Err(_) => continue,
        };

        let state = key
            .open_subkey_with_flags(&guid, KEY_READ)
            .ok()
            .and_then(|ep| ep.get_value::<u32, _>("DeviceState").ok())
            .unwrap_or(0);

        if state != 1 {
            continue;
        }

        let props = key.open_subkey_with_flags(format!("{guid}\\Properties"), KEY_READ);

        let name_interface = props.as_ref().ok().and_then(|p| read_reg_sz(p, VAL_NAME_INTERFACE));
        let name_product = props.as_ref().ok().and_then(|p| read_reg_sz(p, VAL_NAME_PRODUCT));

        let name = match (&name_interface, &name_product) {
            (Some(a), Some(b)) => format!("{a} ({b})"),
            (Some(a), None) => a.clone(),
            (None, Some(b)) => b.clone(),
            (None, None) => {
                let short = &guid[..std::cmp::min(16, guid.len())];
                format!("Unknown ({short})")
            }
        };

        let hw_id = props.as_ref().ok().and_then(|p| {
            read_reg_sz(p, "{80f111c3-b103-42e1-afb6-db7a6fa8be1f},0")
        });

        let dedup_key = hw_id.clone().unwrap_or_else(|| name.clone());
        if !seen.insert(dedup_key) {
            continue;
        }

        // 读 APO 绑定
        let (sfx, mfx, flags) = match key.open_subkey_with_flags(&guid, KEY_READ) {
            Ok(ep) => {
                let mut s = read_reg_sz(&ep, VAL_SFX);
                let mut m = read_reg_sz(&ep, VAL_MFX);
                let mut f: Option<u32> = ep.get_value(VAL_FLAGS).ok();

                if let Ok(fx) = ep.open_subkey_with_flags("FxProperties", KEY_READ) {
                    if s.is_none() {
                        s = read_reg_sz(&fx, VAL_SFX);
                    }
                    if m.is_none() {
                        m = read_reg_sz(&fx, VAL_MFX);
                    }
                    if f.is_none() {
                        f = fx.get_value(VAL_FLAGS).ok();
                    }
                }

                if f.is_none() {
                    if let Ok(props) = ep.open_subkey_with_flags("Properties", KEY_READ) {
                        f = props.get_value(VAL_FLAGS).ok();
                    }
                }

                (s, m, f)
            }
            Err(_) => (None, None, None),
        };

        let sfx_is_system = sfx.as_ref().map(|s| is_system_apo(s)).unwrap_or(false);
        let mfx_is_system = mfx.as_ref().map(|s| is_system_apo(s)).unwrap_or(false);

        out.push(Endpoint {
            index: *idx,
            guid,
            name,
            kind: kind.to_string(),
            sfx,
            mfx,
            flags,
            sfx_is_system,
            mfx_is_system,
        });

        *idx += 1;
    }
}

// ============================================================
// 工具函数
// ============================================================

fn read_reg_sz(key: &RegKey, value_name: &str) -> Option<String> {
    let rv = key.get_raw_value(value_name).ok()?;
    match rv.vtype {
        REG_SZ | REG_EXPAND_SZ => parse_utf16_bytes(&rv.bytes),
        REG_BINARY => parse_prop_string(&rv.bytes),
        _ => None,
    }
}

fn parse_utf16_bytes(data: &[u8]) -> Option<String> {
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

fn parse_multi_utf16(data: &[u8]) -> Option<String> {
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

fn parse_prop_string(data: &[u8]) -> Option<String> {
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