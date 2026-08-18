use std::collections::HashSet;
use winreg::enums::*;
use winreg::RegKey;

use crate::endpoint::{Endpoint, EndpointKind};
use crate::knowledge;
use crate::reg;

const PATH_RENDER: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Render";
const PATH_CAPTURE: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Capture";

pub fn dump_endpoint(ep: &Endpoint) {
    let base = match ep.kind {
        EndpointKind::Playback => PATH_RENDER,
        EndpointKind::Capture => PATH_CAPTURE,
    };
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);

    println!("\n  [{}] {}", ep.kind, ep.guid);

    // 打开设备子键
    let device_path = format!("{base}\\{}", ep.guid);
    if let Ok(device_key) = hklm.open_subkey_with_flags(&device_path, KEY_READ) {
        dump_key_inner(&device_key, "  ", false, &mut HashSet::new());
    }

    // Properties
    if let Ok(props) = hklm.open_subkey_with_flags(&format!("{device_path}\\Properties"), KEY_READ) {
        println!("  ── Properties ──");
        let mut seen = HashSet::new();
        dump_key_inner(&props, "  ", true, &mut seen);
    }

    // FxProperties
    if let Ok(fx) = hklm.open_subkey_with_flags(&format!("{device_path}\\FxProperties"), KEY_READ) {
        println!("  ── FxProperties ──");
        let mut seen = HashSet::new();
        dump_key_inner(&fx, "  ", true, &mut seen);
    }
}

fn dump_key_inner(key: &RegKey, indent: &str, filter: bool, seen: &mut HashSet<String>) {
    for result in key.enum_values() {
        let (name, val) = match result {
            Ok(v) => v,
            Err(_) => continue,
        };

        if filter && !knowledge::is_known_property(&name) {
            continue;
        }

        let display_name = knowledge::friendly_name(&name);

        if !seen.insert(name.clone()) {
            continue;
        }

        match val.vtype {
            REG_SZ | REG_EXPAND_SZ => {
                if let Some(s) = reg::parse_utf16_bytes(&val.bytes) {
                    let suffix = knowledge::apo_slot_annotation(&name);
                    println!("{indent}[SZ]  {display_name:<40} = {s}{suffix}");
                }
            }
            REG_DWORD => {
                if val.bytes.len() >= 4 {
                    let d = u32::from_le_bytes([val.bytes[0], val.bytes[1], val.bytes[2], val.bytes[3]]);
                    let extra = knowledge::decode_dword_label(&name, d);
                    println!("{indent}[DW]  {display_name:<40} = 0x{d:08x}{extra}");
                }
            }
            REG_BINARY => {
                if name.starts_with("{d04e05a6") {
                    if let Some(s) = reg::parse_prop_string(&val.bytes) {
                        println!("{indent}[BIN] {display_name:<40} = {s}");
                    } else {
                        println!("{indent}[BIN] {display_name:<40} = {} bytes", val.bytes.len());
                    }
                } else if filter {
                    continue;
                } else {
                    let preview: Vec<String> = val.bytes.iter().take(8).map(|b| format!("{b:02x}")).collect();
                    println!("{indent}[BIN] {display_name:<40} = [{}...] ({} bytes)", preview.join(" "), val.bytes.len());
                }
            }
            REG_MULTI_SZ => {
                if let Some(s) = reg::parse_multi_utf16(&val.bytes) {
                    println!("{indent}[MSZ] {display_name:<40} = {s}");
                }
            }
            REG_QWORD => {
                if val.bytes.len() >= 8 {
                    let q = u64::from_le_bytes([
                        val.bytes[0], val.bytes[1], val.bytes[2], val.bytes[3],
                        val.bytes[4], val.bytes[5], val.bytes[6], val.bytes[7],
                    ]);
                    println!("{indent}[QW]  {display_name:<40} = 0x{q:016x} ({q})");
                }
            }
            other => {
                if !filter {
                    println!("{indent}[??]  {display_name:<40} = ({other:?}, {} bytes)", val.bytes.len());
                }
            }
        }
    }
}
