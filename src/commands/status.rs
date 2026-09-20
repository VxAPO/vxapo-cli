//! commands/status.rs — 设备列表/状态渲染与槽位诊断

use super::*;

/// 单设备状态详情（交互菜单 [s] 用）：只显示指定设备的槽位 + 子 APO 信息。
///
/// 与 `list_devices` 不同——本函数聚焦单个设备，并展示 childApo 信息区
/// （PreMixChild/PostMixChild，运行期委托给谁）。
pub fn show_device_status(device_ref: &str) -> Result<(), String> {
    let dev = resolve_device(device_ref)?;
    let devices = enumerate_devices().map_err(|e| {
        if lang() == Lang::En {
            format!("Failed to enumerate devices: {e}")
        } else {
            format!("枚举设备失败：{e}")
        }
    })?;
    let d = devices
        .iter()
        .find(|d| d.endpoint.as_ref().map(|e| e.endpoint_guid.eq_ignore_ascii_case(&dev.guid)).unwrap_or(false))
        .ok_or_else(|| {
            if lang() == Lang::En {
                "Device not in enumeration list".to_string()
            } else {
                "设备不在枚举列表".to_string()
            }
        })?;
    let ep = d.endpoint.as_ref().unwrap();
    println!("[{}]", ep.friendly_name);
    println!("  GUID: {}", ep.endpoint_guid);
    if lang() == Lang::En {
        println!("  Version: {}  Mode: {:?}", d.installed_version, d.install_mode);
    } else {
        println!("  版本: {}  模式: {:?}", d.installed_version, d.install_mode);
    }
    // 5 槽位占用（友好名）
    let slot_names = ["LFX", "GFX", "SFX", "MFX", "EFX"];
    for (s, val) in d.slots.iter().enumerate() {
        let label = match val {
            vxapo_driver::SlotValue::Guid(g) => {
                let gs = format!("{g:?}");
                slot_friendly(&gs).unwrap_or_else(|| gs.clone())
            }
            _ => "-".to_string(),
        };
        println!("  {}[{}]: {label}", slot_names[s], s);
    }
    // EAPO 状态
    if let Some(eapo) = detect_eapo_status(&d.slots) {
        println!("  ▶ {eapo}");
    }
    // childApo 信息区
    println!("  {}", tr("子 APO：", "Child APO:"));
    let child_pre = read_child_apo_guid(&ep.endpoint_guid, ChildApoKind::PreMix)
        .map(|g| slot_friendly(&format!("{g:?}")).unwrap_or_else(|| format!("{g:?}")))
        .unwrap_or_else(|| "-".to_string());
    let child_post = read_child_apo_guid(&ep.endpoint_guid, ChildApoKind::PostMix)
        .map(|g| slot_friendly(&format!("{g:?}")).unwrap_or_else(|| format!("{g:?}")))
        .unwrap_or_else(|| "-".to_string());
    println!("    PreMixChild: {child_pre}");
    println!("    PostMixChild: {child_post}");
    // 槽位失守检测
    if child_apo_key_exists(&ep.endpoint_guid) {
        if let Some(lost) = detect_lost_slot(&d.slots, d.install_mode) {
            if lang() == Lang::En {
                println!("  ⚠ Slot lost: {lost} has been taken over; reinstall required");
            } else {
                println!("  ⚠ 槽位失守：{lost} 已被接管，需重装（install）");
            }
        }
    }
    Ok(())
}

/// 查询端点主音量（0.0–1.0；失败返回 None）。
pub(super) fn endpoint_volume(guid: &str) -> Option<f32> {
    use windows::Win32::Media::Audio::{
        EDataFlow, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let needle = guid.to_ascii_uppercase();
        for flow in [EDataFlow(0), EDataFlow(1)] {
            let collection = enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE).ok()?;
            let count = collection.GetCount().ok()?;
            for i in 0..count {
                let device = collection.Item(i).ok()?;
                let id = device.GetId().ok()?;
                if id
                    .to_string()
                    .unwrap_or_default()
                    .to_ascii_uppercase()
                    .contains(&needle)
                {
                    let volume: IAudioEndpointVolume =
                        device.Activate(CLSCTX_ALL, Some(std::ptr::null())).ok()?;
                    return volume.GetMasterVolumeLevelScalar().ok();
                }
            }
        }
        None
    }
}

/// 打印设备列表 + 槽位占用（包含 4.5 友好名 + 槽位失守标注，CLI 引用规范 5.2 status/list）。
pub fn list_devices(json: bool) -> Result<(), String> {
    let devices = enumerate_devices().map_err(|e| {
        if lang() == Lang::En {
            format!("Failed to enumerate devices: {e}")
        } else {
            format!("枚举设备失败：{e}")
        }
    })?;
    if json {
        let mut parts = Vec::new();
        let formats: std::collections::HashMap<String, (Option<u32>, Option<u16>, Option<u16>, &'static str)> =
            crate::probe::probe_all()
                .into_iter()
                .map(|e| {
                    let kind = if matches!(e.kind, crate::endpoint::EndpointKind::Capture) {
                        "capture"
                    } else {
                        "playback"
                    };
                    (e.guid.to_uppercase(), (e.sample_rate, e.channels, e.bit_depth, kind))
                })
                .collect();
        for (i, d) in devices.iter().enumerate() {
            let ep = d.endpoint.as_ref();
            let name = ep.map(|e| e.friendly_name.clone()).unwrap_or_else(|| "(未命名)".to_string());
            let guid = ep.map(|e| e.endpoint_guid.clone()).unwrap_or_default();
            let device_id = ep.map(|e| e.device_id.clone()).unwrap_or_default();
            let (sr, ch, bd, _k) = formats
                .get(&guid.to_uppercase())
                .cloned()
                .unwrap_or((None, None, None, "playback"));
            let sr = sr.map(|v| v.to_string()).unwrap_or_else(|| "null".to_string());
            let ch = ch.map(|v| v.to_string()).unwrap_or_else(|| "null".to_string());
            let bd = bd.map(|v| v.to_string()).unwrap_or_else(|| "null".to_string());
            let kind = match ep.map(|e| e.flow as u8) {
                Some(1) => "capture",
                _ => "playback",
            };
            let kind = formats
                .get(&guid.to_uppercase())
                .map(|(_, _, _, k)| *k)
                .unwrap_or(kind);
            let volume = if guid.is_empty() {
                "null".to_string()
            } else {
                endpoint_volume(&guid)
                    .map(|v| format!("{:.3}", v))
                    .unwrap_or_else(|| "null".to_string())
            };
            let slots: Vec<String> = d.slots.iter().map(|v| match v {
                vxapo_driver::SlotValue::Guid(g) => {
                    let gs = format!("{g:?}");
                    let label = slot_friendly(&gs).unwrap_or_else(|| gs.clone());
                    format!("\"{}\"", json_escape(&label))
                }
                _ => "null".to_string(),
            }).collect();
            let mut o = format!(
                "{{\"index\":{i},\"name\":\"{}\",\"guid\":\"{}\",\"device_id\":\"{}\",\"connection\":\"\",\"installed_version\":\"{}\",\"install_mode\":\"{}\",\"slots\":{{\"LFX\":{},\"GFX\":{},\"SFX\":{},\"MFX\":{},\"EFX\":{}}},\"sample_rate\":{sr},\"channels\":{ch},\"bit_depth\":{bd},\"kind\":\"{kind}\",\"volume\":{volume}",
                json_escape(&name),
                json_escape(&guid),
                json_escape(&device_id),
                json_escape(&d.installed_version),
                crate::verify::mode_str(d.install_mode),
                slots[0], slots[1], slots[2], slots[3], slots[4],
            );
            if let Some(eapo) = detect_eapo_status(&d.slots) {
                o.push_str(&format!(",\"eapo\":\"{}\"", json_escape(&eapo)));
            }
            if !guid.is_empty() && child_apo_key_exists(&guid) {
                if let Some(lost) = detect_lost_slot(&d.slots, d.install_mode) {
                    o.push_str(&format!(",\"lost_slot\":\"{}\"", json_escape(&lost)));
                }
            }
            o.push('}');
            parts.push(o);
        }
        println!("[{}]", parts.join(","));
        return Ok(());
    }
    if devices.is_empty() {
        println!("{}", tr("（无音频端点）", "(no audio endpoints)"));
        return Ok(());
    }
    for (i, d) in devices.iter().enumerate() {
        let ep = d.endpoint.as_ref();
        let name = ep.map(|e| e.friendly_name.clone()).unwrap_or_else(|| {
            if lang() == Lang::En { "(unnamed)".to_string() } else { "(未命名)".to_string() }
        });
        let guid = ep.map(|e| e.endpoint_guid.clone()).unwrap_or_default();
        println!("[{i}] {name}");
        println!("     GUID: {guid}");
        if lang() == Lang::En {
            println!("     Version: {}  Mode: {:?}", d.installed_version, d.install_mode);
        } else {
            println!("     版本: {}  模式: {:?}", d.installed_version, d.install_mode);
        }
        // 5 槽位占用（友好名）
        let slot_names = ["LFX", "GFX", "SFX", "MFX", "EFX"];
        for (s, val) in d.slots.iter().enumerate() {
            let label = match val {
                vxapo_driver::SlotValue::Guid(g) => {
                    let gs = format!("{g:?}");
                    slot_friendly(&gs).unwrap_or_else(|| gs.clone())
                }
                _ => "-".to_string(),
            };
            let sname = slot_names[s];
            println!("     {}[{}]: {label}", sname, s);
        }
        // EAPO 安装状态
        if let Some(eapo) = detect_eapo_status(&d.slots) {
            println!("     ▶ {eapo}");
        }
        // 槽位失守检测
        if !guid.is_empty() && child_apo_key_exists(&guid) {
            if let Some(lost) = detect_lost_slot(&d.slots, d.install_mode) {
                if lang() == Lang::En {
                    println!("     ⚠ Slot lost: {lost} has been taken over; reinstall required");
                } else {
                    println!("     ⚠ 槽位失守：{lost} 已被接管，需重装（install）");
                }
            }
        }
    }
    Ok(())
}

/// 检测安装模式槽位是否失守（CLI 引用规范 5.2 +）。
pub(super) fn detect_lost_slot(
    slots: &[vxapo_driver::SlotValue; 5],
    mode: vxapo_driver::InstallMode,
) -> Option<String> {
    let premix = slots[mode.premix_slot().index() as usize];
    let postmix = slots[mode.postmix_slot().index() as usize];
    let pre_ok = matches!(premix, vxapo_driver::SlotValue::Guid(g) if g == CLSID_VXAPO_PRE_MIX);
    let post_ok = matches!(postmix, vxapo_driver::SlotValue::Guid(g) if g == CLSID_VXAPO_POST_MIX);
    if pre_ok && post_ok {
        None
    } else {
        Some(format!("PreMix={} PostMix={}", slot_friendly(&format!("{premix:?}")).unwrap_or_default(), slot_friendly(&format!("{postmix:?}")).unwrap_or_default()))
    }
}

/// 检测设备上 EAPO 安装状态（哪些槽位被 EAPO PreMix/PostMix 占用）。
///
/// EAPO 的 CLSID 见 `knowledge::KNOWN_APO_CLSIDS`（此处只存小写无花括号形式用于比较）。
/// 返回 None = 无 EAPO。
pub(super) fn detect_eapo_status(
    slots: &[vxapo_driver::SlotValue; 5],
) -> Option<String> {
    const EAPO_PRE: &str = "eacd2258-fcac-4ff4-b36d-419e924a6d79";
    const EAPO_POST: &str = "ec1cc9ce-faed-4822-828a-82a81a6f018f";
    const SLOT_NAMES: [&str; 5] = ["LFX", "GFX", "SFX", "MFX", "EFX"];

    let mut pre_slot: Option<&str> = None;
    let mut post_slot: Option<&str> = None;
    for (s, val) in slots.iter().enumerate() {
        let normalized = match val {
            vxapo_driver::SlotValue::Guid(g) => {
                format!("{g:?}").to_lowercase().replace(['{', '}'], "")
            }
            _ => continue,
        };
        if normalized == EAPO_PRE {
            pre_slot = Some(SLOT_NAMES[s]);
        }
        if normalized == EAPO_POST {
            post_slot = Some(SLOT_NAMES[s]);
        }
    }
    match (pre_slot, post_slot) {
        (Some(p), Some(q)) => Some(if lang() == Lang::En {
            format!("EAPO installed: PreMix=({p}) + PostMix=({q})")
        } else {
            format!("EAPO 已安装：PreMix=({p}) + PostMix=({q})")
        }),
        (Some(p), None) => Some(if lang() == Lang::En {
            format!("EAPO partially installed: PreMix only ({p})")
        } else {
            format!("EAPO 部分安装：仅 PreMix=({p})")
        }),
        (None, Some(q)) => Some(if lang() == Lang::En {
            format!("EAPO partially installed: PostMix only ({q})")
        } else {
            format!("EAPO 部分安装：仅 PostMix=({q})")
        }),
        (None, None) => None,
    }
}

/// 槽位 GUID → 友好名（4.5：EAPO/VxAPO CLSID 映射）。
///
/// 输入来自 `format!("{g:?}")`（windows-rs GUID Debug，大小写/花括号不确定），
/// 与 KNOWN_APO_CLSIDS key（`{eacd2258-...}` 小写带花括号）做**规范化匹配**：
/// 去花括号 + 转小写比较，兼容两种字形。
pub(super) fn slot_friendly(clsid: &str) -> Option<String> {
    let normalized = clsid.to_lowercase().replace(['{', '}'], "");
    for (k, v) in KNOWN_APO_CLSIDS.iter() {
        if k.to_lowercase().replace(['{', '}'], "") == normalized {
            return Some((*v).to_string());
        }
    }
    None
}

