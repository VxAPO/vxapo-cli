//! commands/status.rs — 设备列表/状态渲染与槽位诊断

use super::*;
use crate::knowledge::{
    normalize_clsid, EAPO_POST_MIX_CLSID, EAPO_PRE_MIX_CLSID, KNOWN_APO_CLSIDS_NORMALIZED,
};

/// 单设备状态详情（交互菜单 [s] 用）：只显示指定设备的槽位 + 子 APO 信息。
///
/// 与 `list_devices` 不同——本函数聚焦单个设备，并展示 childApo 信息区
/// （PreMixChild/PostMixChild，运行期委托给谁）。
pub fn show_device_status(device_ref: &str) -> Result<(), String> {
    // 单次全量枚举：resolve（可能按序号定位）与配对查找共用同一份列表
    // （原实现 resolve_device 内部全量枚举一次、这里再枚举一次）。
    let devices = enumerate_devices().map_err(|e| {
        if lang() == Lang::En {
            format!("Failed to enumerate devices: {e}")
        } else {
            format!("枚举设备失败：{e}")
        }
    })?;
    let dev = resolve_device_in(device_ref, &devices)?;
    let d = find_device_by_guid(&devices, &dev.guid)?;
    // `find_device_by_guid` 谓词已要求 endpoint 存在；这里仍用 `let-else` 兜底而不是 unwrap。
    let Some(ep) = d.endpoint.as_ref() else {
        return Err(if lang() == Lang::En {
            "Device not in enumeration list".to_string()
        } else {
            "设备不在枚举列表".to_string()
        });
    };
    println!("[{}]", ep.friendly_name);
    println!("  GUID: {}", ep.endpoint_guid);
    if lang() == Lang::En {
        println!(
            "  Version: {}  Mode: {:?}",
            d.installed_version, d.install_mode
        );
    } else {
        println!(
            "  版本: {}  模式: {:?}",
            d.installed_version, d.install_mode
        );
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

/// 查询全部活动端点的主音量（端点 GUID 大写无花括号 → 0.0–1.0）。
///
/// **为什么自起 COM 枚举而不走 driver**：driver facade 只暴露设备/槽位/安装状态，
/// 没有端点音量查询接口；这里按端点 GUID 直接问 `IAudioEndpointVolume`，
/// 仅供 `list --json` 的展示字段，不参与安装/卸载写路径。
///
/// **一次建表**：COM 初始化一次、每流枚举一次、每端点 Activate 一次。
/// 原实现对每台设备重复 CoInitializeEx/CoCreateInstance 并全端点线性找 GUID
/// （N 台设备 = N 次 COM 初始化 + N×M 次 GetId 字符串转换），`list --json`
/// 的耗时大头就在这里。首个命中的流优先（Render 先于 Capture），与原逐台
/// 查找语义一致。
pub(super) fn endpoint_volume_map() -> std::collections::HashMap<String, f32> {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        EDataFlow, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    let mut map = std::collections::HashMap::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(e) => e,
                Err(_) => return map,
            };
        for flow in [EDataFlow(0), EDataFlow(1)] {
            let Ok(collection) = enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) else {
                continue;
            };
            let Ok(count) = collection.GetCount() else {
                continue;
            };
            for i in 0..count {
                let Ok(device) = collection.Item(i) else {
                    continue;
                };
                let Ok(id) = device.GetId() else {
                    continue;
                };
                // 端点 ID 形如 `{0.0.0.00000000}.{endpoint-guid}`；末段即端点 GUID。
                let id_str = id.to_string().unwrap_or_default();
                let Some(guid_part) = id_str.rsplit('.').next() else {
                    continue;
                };
                let key = normalize_clsid(guid_part).to_ascii_uppercase();
                if map.contains_key(&key) {
                    continue; // 首个命中的流优先。
                }
                let volume: Option<IAudioEndpointVolume> =
                    device.Activate(CLSCTX_ALL, Some(std::ptr::null())).ok();
                if let Some(vol) = volume.and_then(|v| v.GetMasterVolumeLevelScalar().ok()) {
                    map.insert(key, vol);
                }
            }
        }
    }
    map
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
        // 输出契约见 `vxapo_protocol::Device`（App 侧类型由同一 crate 生成，不再手写拼接）。
        // 格式与流向一律取自 driver 枚举（决策 6）：`DeviceInfo.format` + `EndpointInfo.flow`。
        // 原实现会另扫一遍 MMDevices 注册表（`probe::probe_all`）再按 GUID 拼回，
        // 现已删除；两条路径的口径对齐由 driver `format.rs` 的解析单测锁定。
        let volume_map = endpoint_volume_map();
        let mut out: Vec<Device> = Vec::with_capacity(devices.len());
        for (i, d) in devices.iter().enumerate() {
            let ep = d.endpoint.as_ref();
            let name = ep
                .map(|e| e.friendly_name.clone())
                .unwrap_or_else(|| "(未命名)".to_string());
            let guid = ep.map(|e| e.endpoint_guid.clone()).unwrap_or_default();
            let device_id = ep.map(|e| e.device_id.clone()).unwrap_or_default();
            let format = d.format.as_ref();
            let kind = match ep.map(|e| e.flow) {
                Some(vxapo_driver::Flow::Capture) => DeviceKind::Capture,
                _ => DeviceKind::Playback,
            };
            let mut slot_labels: [Option<String>; 5] = Default::default();
            for (idx, v) in d.slots.iter().enumerate() {
                if let vxapo_driver::SlotValue::Guid(g) = v {
                    let gs = format!("{g:?}");
                    slot_labels[idx] = Some(slot_friendly(&gs).unwrap_or_else(|| gs.clone()));
                }
            }
            out.push(Device {
                index: i as u32,
                name,
                guid: guid.clone(),
                device_id,
                connection: String::new(),
                installed_version: d.installed_version.clone(),
                install_mode: crate::verify::mode_str(d.install_mode).to_string(),
                slots: DeviceSlots::from_slice(slot_labels),
                sample_rate: format.map(|f| f.sample_rate),
                channels: format.map(|f| u32::from(f.channels)),
                bit_depth: format.map(|f| u32::from(f.bits_per_sample)),
                kind,
                volume: if guid.is_empty() {
                    None
                } else {
                    volume_map
                        .get(&normalize_clsid(&guid).to_ascii_uppercase())
                        .copied()
                },
                eapo: detect_eapo_status(&d.slots),
                lost_slot: if !guid.is_empty() && child_apo_key_exists(&guid) {
                    detect_lost_slot(&d.slots, d.install_mode)
                } else {
                    None
                },
            });
        }
        println!(
            "{}",
            serde_json::to_string(&out).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if devices.is_empty() {
        println!("{}", tr("（无音频端点）", "(no audio endpoints)"));
        return Ok(());
    }
    for (i, d) in devices.iter().enumerate() {
        let ep = d.endpoint.as_ref();
        let name = ep.map(|e| e.friendly_name.clone()).unwrap_or_else(|| {
            if lang() == Lang::En {
                "(unnamed)".to_string()
            } else {
                "(未命名)".to_string()
            }
        });
        let guid = ep.map(|e| e.endpoint_guid.clone()).unwrap_or_default();
        println!("[{i}] {name}");
        println!("     GUID: {guid}");
        if lang() == Lang::En {
            println!(
                "     Version: {}  Mode: {:?}",
                d.installed_version, d.install_mode
            );
        } else {
            println!(
                "     版本: {}  模式: {:?}",
                d.installed_version, d.install_mode
            );
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
        Some(format!(
            "PreMix={} PostMix={}",
            slot_friendly(&format!("{premix:?}")).unwrap_or_default(),
            slot_friendly(&format!("{postmix:?}")).unwrap_or_default()
        ))
    }
}

/// 检测设备上 EAPO 安装状态（哪些槽位被 EAPO PreMix/PostMix 占用）。
///
/// 直接比较 GUID 常量（`knowledge::EAPO_PRE_MIX_CLSID` / `EAPO_POST_MIX_CLSID`，
/// 与 KNOWN_APO_CLSIDS 的两个 EAPO key 同源）；原实现对每槽位做
/// `format! + to_lowercase + replace` 字符串归一化再比串，纯属绕路。
/// 返回 None = 无 EAPO。
pub(super) fn detect_eapo_status(slots: &[vxapo_driver::SlotValue; 5]) -> Option<String> {
    const SLOT_NAMES: [&str; 5] = ["LFX", "GFX", "SFX", "MFX", "EFX"];

    let mut pre_slot: Option<&str> = None;
    let mut post_slot: Option<&str> = None;
    for (s, val) in slots.iter().enumerate() {
        if let vxapo_driver::SlotValue::Guid(g) = val {
            if *g == EAPO_PRE_MIX_CLSID {
                pre_slot = Some(SLOT_NAMES[s]);
            }
            if *g == EAPO_POST_MIX_CLSID {
                post_slot = Some(SLOT_NAMES[s]);
            }
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
/// 归一化后查预归一化表（`knowledge::KNOWN_APO_CLSIDS_NORMALIZED`，首次访问
/// 建表一次）；原实现每次调用对全表 key 现做归一化，每设备 5 槽就是
/// 5×N 次字符串分配。
pub(super) fn slot_friendly(clsid: &str) -> Option<String> {
    let normalized = normalize_clsid(clsid);
    KNOWN_APO_CLSIDS_NORMALIZED
        .get(&normalized)
        .map(|v| (*v).to_string())
}
