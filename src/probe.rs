use std::collections::HashSet;
use winreg::enums::*;
use winreg::RegKey;

use crate::endpoint::{Endpoint, EndpointKind};
use crate::reg;

const PATH_RENDER: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Render";
const PATH_CAPTURE: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\MMDevices\\Audio\\Capture";

pub fn probe_all() -> Vec<Endpoint> {
    let mut endpoints = Vec::new();
    let mut idx = 0;
    probe_path(PATH_RENDER, EndpointKind::Playback, &mut endpoints, &mut idx);
    probe_path(PATH_CAPTURE, EndpointKind::Capture, &mut endpoints, &mut idx);
    endpoints
}

fn probe_path(base: &str, ep_kind: EndpointKind, out: &mut Vec<Endpoint>, idx: &mut usize) {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = match hklm.open_subkey_with_flags(base, KEY_READ) {
        Ok(k) => k,
        Err(_) => return,
    };

    let mut seen = HashSet::new();

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

        let format_raw = props
            .as_ref()
            .ok()
            .and_then(|p| p.get_raw_value("{f19f064d-082c-4e27-bc73-6882a1bb8e4c},0").ok());

        let (sample_rate, channels, bit_depth, channel_mask) = format_raw
            .and_then(|rv| {
                if rv.vtype == REG_BINARY && rv.bytes.len() >= 16 {
                    reg::parse_waveformatex(&rv.bytes)
                } else {
                    None
                }
            })
            .unwrap_or((None, None, None, None));

        let name_interface = props.as_ref().ok().and_then(|p| reg::read_reg_sz(p, reg::VAL_NAME_INTERFACE));
        let name_product = props.as_ref().ok().and_then(|p| reg::read_reg_sz(p, reg::VAL_NAME_PRODUCT));

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
            reg::read_reg_sz(p, "{80f111c3-b103-42e1-afb6-db7a6fa8be1f},0")
        });

        let dedup_key = hw_id.clone().unwrap_or_else(|| name.clone());
        if !seen.insert(dedup_key) {
            continue;
        }

        let (sfx, mfx, efx, endpoint_flags, disable_enhancements) =
            match key.open_subkey_with_flags(&guid, KEY_READ) {
                Ok(ep) => {
                    let mut s = reg::read_reg_sz(&ep, reg::VAL_SFX);
                    let mut m = reg::read_reg_sz(&ep, reg::VAL_MFX);
                    let mut e = reg::read_reg_sz(&ep, reg::VAL_EFX);
                    let mut ef: Option<u32> = ep.get_value(reg::VAL_FLAGS).ok();
                    let mut de: Option<u32> = None;

                    if let Ok(fx) = ep.open_subkey_with_flags("FxProperties", KEY_READ) {
                        if s.is_none() { s = reg::read_reg_sz(&fx, reg::VAL_SFX); }
                        if m.is_none() { m = reg::read_reg_sz(&fx, reg::VAL_MFX); }
                        if e.is_none() { e = reg::read_reg_sz(&fx, reg::VAL_EFX); }
                        de = fx.get_value("{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},5").ok();
                    }

                    if ef.is_none() {
                        if let Ok(props) = ep.open_subkey_with_flags("Properties", KEY_READ) {
                            ef = props.get_value(reg::VAL_FLAGS).ok();
                        }
                    }

                    (s, m, e, ef, de)
                }
                Err(_) => (None, None, None, None, None),
            };

        let sfx_is_system = sfx.as_ref().map(|s| crate::knowledge::is_system_apo(s)).unwrap_or(false);
        let mfx_is_system = mfx.as_ref().map(|s| crate::knowledge::is_system_apo(s)).unwrap_or(false);
        let efx_is_system = efx.as_ref().map(|s| crate::knowledge::is_system_apo(s)).unwrap_or(false);

        out.push(Endpoint {
            index: *idx,
            guid,
            name,
            kind: ep_kind.clone(),
            sfx,
            mfx,
            efx,
            endpoint_flags,
            disable_enhancements,
            sfx_is_system,
            mfx_is_system,
            efx_is_system,
            sample_rate,
            channels,
            bit_depth,
            channel_mask,
        });

        *idx += 1;
    }
}
