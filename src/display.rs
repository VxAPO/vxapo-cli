use crate::endpoint::Endpoint;
use crate::knowledge;

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const CYAN: &str = "\x1b[36m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const MAGENTA: &str = "\x1b[35m";
pub const RED: &str = "\x1b[31m";
pub const GRAY: &str = "\x1b[90m";

pub fn print_endpoints(endpoints: &[Endpoint]) {
    println!("\n{BOLD}Found {} endpoints:{RESET}\n", endpoints.len());
    for ep in endpoints {
        let kind_color = if ep.kind == crate::endpoint::EndpointKind::Playback { CYAN } else { GREEN };
        let kind_display = format!("{GRAY}({kind_color}{}{GRAY}){RESET}", ep.kind);
        let (apo_text, apo_color) = classify_apo(ep);

        println!(
            "  {BOLD}[{}]{RESET} {} {kind_display} {apo_color}{}{RESET}",
            ep.index, ep.name, apo_text,
        );
    }
}

fn classify_apo(ep: &Endpoint) -> (String, &'static str) {
    let sfx = ep.sfx.as_ref().filter(|s| !knowledge::is_system_apo(s));
    let mfx = ep.mfx.as_ref().filter(|s| !knowledge::is_system_apo(s));
    let efx = ep.efx.as_ref().filter(|s| !knowledge::is_system_apo(s));
    let sfx_sys = ep.sfx.is_some() && ep.sfx_is_system;
    let mfx_sys = ep.mfx.is_some() && ep.mfx_is_system;
    let efx_sys = ep.efx.is_some() && ep.efx_is_system;

    let has_third_party = sfx.is_some() || mfx.is_some() || efx.is_some();
    let has_system = sfx_sys || mfx_sys || efx_sys;

    if !has_third_party && !has_system {
        return ("[none]".into(), GRAY);
    }
    if !has_third_party && has_system {
        return ("[Windows default]".into(), GRAY);
    }

    let mut parts = Vec::new();
    if sfx.is_some() { parts.push("SFX"); }
    if mfx.is_some() { parts.push("MFX"); }
    if efx.is_some() { parts.push("EFX"); }

    let label = if parts.is_empty() {
        "[Windows default]".to_string()
    } else {
        format!("[{}]", parts.join(" + "))
    };

    let color = if parts.contains(&"SFX") && parts.contains(&"EFX") {
        YELLOW
    } else if parts.contains(&"SFX") && parts.contains(&"MFX") {
        YELLOW
    } else {
        MAGENTA
    };

    (label, color)
}

pub fn print_detail_header(ep: &Endpoint) {
    let kind_color = if ep.kind == crate::endpoint::EndpointKind::Playback { CYAN } else { GREEN };
    println!("\n{BOLD}========================================{RESET}");
    println!(" {BOLD}{}{RESET}", ep.name);
    println!("{BOLD}========================================{RESET}");
    println!("  Index:    {BOLD}{}{RESET}", ep.index);
    println!("  GUID:     {GRAY}{}{RESET}", ep.guid);
    println!("  Type:     {kind_color}{}{RESET}", ep.kind);
    println!("  Path:     {GRAY}HKLM\\...\\Audio\\{}\\{}{RESET}", ep.kind.reg_path(), ep.guid);

    if let (Some(sr), Some(ch), Some(bd)) = (ep.sample_rate, ep.channels, ep.bit_depth) {
        if sr > 0 && ch > 0 && bd > 0 {
            let mask_str = ep.channel_mask
                .map(|m| format!("0x{m:08x}"))
                .unwrap_or_else(|| "none".to_string());
            println!("  Audio:    {sr} Hz, {ch}ch, {bd}bit, mask={mask_str}");
        }
    }

    if let (Some(ch), Some(mask)) = (ep.channels, ep.channel_mask) {
        let speaker_count = mask.count_ones();
        if speaker_count > 0 && speaker_count != ch as u32 {
            println!("  {YELLOW}WARNING: mask has {speaker_count} speaker(s) but device reports {ch} channel(s){RESET}");
        }
    }

    fn apo_display(val: &Option<String>, is_system: bool) -> String {
        match val {
            Some(clsid) if is_system => format!("{GRAY}{clsid} (system, replaceable){RESET}"),
            Some(clsid) => format!("{YELLOW}{clsid}{RESET}"),
            None => format!("{GRAY}(none){RESET}"),
        }
    }

    println!("  SFX APO:  {}", apo_display(&ep.sfx, ep.sfx_is_system));
    println!("  MFX APO:  {}", apo_display(&ep.mfx, ep.mfx_is_system));
    println!("  EFX APO:  {}", apo_display(&ep.efx, ep.efx_is_system));

    match ep.endpoint_flags {
        Some(flags) => {
            let sysfx = if flags & 0x01 != 0 { format!("{RED}DISABLED{RESET}") }
                        else { format!("{GREEN}ENABLED{RESET}") };
            println!("  SysFX:    {sysfx} {GRAY}(AudioEndpoint_Flags=0x{flags:02x}, bit0={}){RESET}", flags & 1);
        }
        None => println!("  SysFX:    {GRAY}(not set){RESET}"),
    }

    match ep.disable_enhancements {
        Some(0) | None => println!("  Enhancements: {GREEN}ENABLED{RESET} {GRAY}(DisableEnhancements not set){RESET}"),
        Some(1)       => println!("  Enhancements: {RED}DISABLED{RESET} {GRAY}(DisableEnhancements=1){RESET}"),
        Some(v)       => println!("  Enhancements: {YELLOW}UNKNOWN{RESET} {GRAY}(DisableEnhancements={v}){RESET}"),
    }

    println!("{BOLD}========================================{RESET}");
}
