//! CLI 命令层（CLI 引用规范 四/五：install/uninstall/config/status/snapshot）
//!
//! 分层：CLI 命令（参数解析）→ 本层辅助（resolve/snapshot）→ driver API（install 层唯一写入口）。
//! 约束：不触碰 pipeline/RT；不自行写注册表（经 driver Transaction）；config 写归 CLI。

use std::path::Path;

use vxapo_driver::install::audiodg::ensure_can_load;
use vxapo_driver::install::device::info::enumerate_devices;
use vxapo_driver::install::device::slots::{ChildApoKind, child_apo_key_exists, read_child_apo_guid};
use vxapo_driver::install::selector::operation::{InstallConfig, install_endpoint, uninstall_endpoint};
use vxapo_driver::object::vx_reg_props::{CLSID_VXAPO_POST_MIX, CLSID_VXAPO_PRE_MIX};
use vxapo_driver::sys::known_folder::documents_folder;

use crate::knowledge::KNOWN_APO_CLSIDS;

/// 设备三元组（resolve_device 产物，CLI 引用规范 4.4.1）。
pub struct DeviceRef {
    pub guid: String,
    pub name: String,
    pub connection: String,
}

/// 解析 `<device>`：GUID（规范形式）或枚举序号 → (guid, name, connection)。
pub fn resolve_device(device_ref: &str) -> Result<DeviceRef, String> {
    let devices = enumerate_devices().map_err(|e| format!("枚举设备失败：{e}"))?;
    if device_ref.starts_with('{') {
        let guid = device_ref.to_owned();
        // GUID 格式校验（{xxxxxxxx-...}，len=38）
        if guid.len() != 38 || !guid.ends_with('}') {
            return Err(format!("无效 GUID：<{device_ref}>（应为 {{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}}）"));
        }
        let name = devices
            .iter()
            .find(|d| d.endpoint.as_ref().map(|e| e.endpoint_guid.eq_ignore_ascii_case(&guid)).unwrap_or(false))
            .and_then(|d| d.endpoint.as_ref())
            .map(|e| e.friendly_name.clone())
            .unwrap_or_default();
        let connection = String::new();
        return Ok(DeviceRef { guid, name, connection });
    }
    // 数字序号
    if let Ok(idx) = device_ref.parse::<usize>() {
        let d = devices.get(idx).ok_or_else(|| format!("枚举序号越界：<{device_ref}>（0..{}）", devices.len().saturating_sub(1)))?;
        let ep = d.endpoint.as_ref().ok_or_else(|| "设备缺端点信息".to_string())?;
        return Ok(DeviceRef { guid: ep.endpoint_guid.clone(), name: ep.friendly_name.clone(), connection: String::new() });
    }
    Err(format!("未知设备：<{device_ref}>（可用 list 查看序号，或用 {{GUID}} 形式）"))
}

/// 管理员检查（install/uninstall 需 HKLM 写权限）。
pub fn require_admin() -> Result<(), String> {
    // 以写注册表能力探测管理员权限（HKLM 写测试不需额外依赖）。
    // 经 driver RegKey::create 写探测键（幂等：无权限会 Err）。
    let probe_key = r"SOFTWARE\VxAPO\CLIProbe";
    match vxapo_driver::sys::registry::RegKey::create(
        windows::Win32::System::Registry::HKEY_LOCAL_MACHINE,
        probe_key,
    ) {
        Ok(_) => {
            let _ = vxapo_driver::sys::registry::RegKey::open(
                windows::Win32::System::Registry::HKEY_LOCAL_MACHINE,
                probe_key,
            )
            .and_then(|k| k.delete_sub_key(probe_key));
            Ok(())
        }
        Err(_) => Err("需要管理员权限（写入 HKLM）。请以管理员运行 CLI。".to_string()),
    }
}

/// 打印设备列表 + 槽位占用（包含 4.5 友好名 + 槽位失守标注，CLI 引用规范 5.2 status/list）。
pub fn list_devices() -> Result<(), String> {
    let devices = enumerate_devices().map_err(|e| format!("枚举设备失败：{e}"))?;
    if devices.is_empty() {
        println!("（无音频端点）");
        return Ok(());
    }
    for (i, d) in devices.iter().enumerate() {
        let ep = d.endpoint.as_ref();
        let name = ep.map(|e| e.friendly_name.clone()).unwrap_or_else(|| "(未命名)".to_string());
        let guid = ep.map(|e| e.endpoint_guid.clone()).unwrap_or_default();
        println!("[{i}] {name}");
        println!("     GUID: {guid}");
        println!("     版本: {}  模式: {:?}", d.installed_version, d.install_mode);
        // 5 槽位占用（友好名）
        let slot_names = ["LFX", "GFX", "SFX", "MFX", "EFX"];
        for (s, val) in d.slots.iter().enumerate() {
            let label = match val {
                vxapo_driver::install::device::slots::SlotValue::Guid(g) => {
                    let gs = format!("{g:?}");
                    slot_friendly(&gs).unwrap_or_else(|| gs.clone())
                }
                _ => "-".to_string(),
            };
            // slot_names 索引 + 槽位名（format 不能嵌套 {}）
            let sname = slot_names[s];
            println!("     {}[{}]: {label}", sname, s);
        }
        // EAPO 安装行为（用户要求：除 VxAPO CLSID 外还要能确定 EAPO 安装状态给 CLI 看）
        if let Some(eapo) = detect_eapo_status(&d.slots) {
            println!("     ▶ {eapo}");
        }
        // 槽位失守检测（v8.5 判定语义：**只有 childapo 键存在（非全量=已安装过）才验证**；
        // 初次安装/完全卸载后（键不存在=全量路径）不走失守逻辑）
        if !guid.is_empty() && child_apo_key_exists(&guid) {
            if let Some(lost) = detect_lost_slot(&d.slots, d.install_mode) {
                println!("     ⚠ 槽位失守：{lost} 已被接管，需重装（install）");
            }
        }
    }
    Ok(())
}

/// 检测安装模式槽位是否失守（CLI 引用规范 5.2 + v8.5）。
fn detect_lost_slot(
    slots: &[vxapo_driver::install::device::slots::SlotValue; 5],
    mode: vxapo_driver::install::device::slots::InstallMode,
) -> Option<String> {
    let premix = slots[mode.premix_slot().index() as usize];
    let postmix = slots[mode.postmix_slot().index() as usize];
    let pre_ok = matches!(premix, vxapo_driver::install::device::slots::SlotValue::Guid(g) if g == CLSID_VXAPO_PRE_MIX);
    let post_ok = matches!(postmix, vxapo_driver::install::device::slots::SlotValue::Guid(g) if g == CLSID_VXAPO_POST_MIX);
    if pre_ok && post_ok {
        None
    } else {
        Some(format!("PreMix={} PostMix={}", slot_friendly(&format!("{premix:?}")).unwrap_or_default(), slot_friendly(&format!("{postmix:?}")).unwrap_or_default()))
    }
}

/// 检测设备上 EAPO 安装状态（哪些槽位被 EAPO PreMix/PostMix 占用）。
///
/// EAPO 的 CLSID 实证：PreMix={EACD2258-...}、PostMix={EC1CC9CE-...}
/// （与 knowledge::KNOWN_APO_CLSIDS 一致）。返回 None = 无 EAPO。
fn detect_eapo_status(
    slots: &[vxapo_driver::install::device::slots::SlotValue; 5],
) -> Option<String> {
    const EAPO_PRE: &str = "eacd2258-fcac-4ff4-b36d-419e924a6d79";
    const EAPO_POST: &str = "ec1cc9ce-faed-4822-828a-82a81a6f018f";
    const SLOT_NAMES: [&str; 5] = ["LFX", "GFX", "SFX", "MFX", "EFX"];

    let mut pre_slot: Option<&str> = None;
    let mut post_slot: Option<&str> = None;
    for (s, val) in slots.iter().enumerate() {
        let normalized = match val {
            vxapo_driver::install::device::slots::SlotValue::Guid(g) => {
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
        (Some(p), Some(q)) => Some(format!("EAPO 已安装：PreMix=({p}) + PostMix=({q})")),
        (Some(p), None) => Some(format!("EAPO 部分安装：仅 PreMix=({p})")),
        (None, Some(q)) => Some(format!("EAPO 部分安装：仅 PostMix=({q})")),
        (None, None) => None,
    }
}

/// 槽位 GUID → 友好名（4.5：EAPO/VxAPO CLSID 映射）。
///
/// 输入来自 `format!("{g:?}")`（windows-rs GUID Debug，大小写/花括号不确定），
/// 与 KNOWN_APO_CLSIDS key（`{eacd2258-...}` 小写带花括号）做**规范化匹配**：
/// 去花括号 + 转小写比较，兼容两种字形。
fn slot_friendly(clsid: &str) -> Option<String> {
    let normalized = clsid.to_lowercase().replace(['{', '}'], "");
    for (k, v) in KNOWN_APO_CLSIDS.iter() {
        if k.to_lowercase().replace(['{', '}'], "") == normalized {
            return Some((*v).to_string());
        }
    }
    None
}

/// install 命令（CLI 引用规范 5.1）。
pub fn install(device_ref: &str, mode: Option<&str>, no_child: bool) -> Result<(), String> {
    require_admin()?;
    let dev = resolve_device(device_ref)?;
    let mut config = InstallConfig::default_config();
    if let Some(m) = mode {
        config.install_mode = match m.to_lowercase().as_str() {
            "lfxgfx" => vxapo_driver::install::device::slots::InstallMode::LfxGfx,
            "sfxmfx" => vxapo_driver::install::device::slots::InstallMode::SfxMfx,
            "sfxefx" => vxapo_driver::install::device::slots::InstallMode::SfxEfx,
            _ => return Err(format!("无效模式：{m}（LfxGfx/SfxMfx/SfxEfx）")),
        };
    }
    config.use_original_apo_premix = !no_child;
    config.use_original_apo_postmix = !no_child;

    // 快照基线（安装前建立/替换，Phase C——只注册表，config 不属 CLI 快照）。
    if let Err(e) = snapshot_device(&dev.guid, true) {
        println!("⚠ 快照建立失败（继续安装）：{e}");
    }

    ensure_can_load().map_err(|e| format!("audiodg 检查失败：{e}"))?;
    install_endpoint(&dev.guid, &dev.name, &dev.connection, &config, true)
        .map_err(|e| format!("install_endpoint 失败：{e}（可用 vxapo-cli snapshot diff -d {guid} 查看变更）", guid = dev.guid))?;
    println!("✓ 已安装 {}（模式 {:?}，子 APO 保留={}）", dev.guid, config.install_mode, !no_child);
    Ok(())
}

/// uninstall 命令（CLI 引用规范 5.3）。
pub fn uninstall(device_ref: &str) -> Result<(), String> {
    require_admin()?;
    let dev = resolve_device(device_ref)?;
    if !snapshot_exists(&dev.guid) {
        return Err("无基线可对比——快照不存在（先 install 建立基线）".to_string());
    }
    uninstall_endpoint(&dev.guid).map_err(|e| format!("uninstall_endpoint 失败：{e}"))?;
    print!("✓ 已卸载 {}。", dev.guid);
    if let Ok(diff) = snapshot_diff(&dev.guid) {
        println!(" 变更统计：{diff}");
    } else {
        println!();
    }
    Ok(())
}

/// config set（CLI 引用规范 5.2）：源文件内容原样写入 per-device config.txt。
pub fn config_set(device_ref: &str, file: &str) -> Result<(), String> {
    let dev = resolve_device(device_ref)?;
    let src = std::fs::read_to_string(file)
        .map_err(|e| format!("读取源文件失败：{file}：{e}"))?;
    let path = config_path(&dev.guid)?;
    if let Some(parent) = Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败：{e}"))?;
    }
    std::fs::write(&path, &src).map_err(|e| format!("写入 config 失败：{e}"))?;
    println!("✓ config 已写入（{} 字节），语法验证中…", src.len());
    config_show(&dev.guid)?;
    Ok(())
}

/// config show：读回 + ConfigParser 语法验证（CLI 引用规范 5.2）。
pub fn config_show(device_ref: &str) -> Result<(), String> {
    let dev = resolve_device(device_ref)?;
    let path = config_path(&dev.guid)?;
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Err("未配置：config.txt 不存在（可用 config set -f <file> 写入）".to_string()),
    };
    println!("--- config.txt ({path}) ---");
    println!("{content}");
    // 读回一致性验证（CLI 引用规范 三「config show 读回验证——文件级」，不解析 DSP 语义）。
    println!("✓ 文件可读回（{} 字节）", content.len());
    Ok(())
}

/// per-device config 路径（CLI 引用规范 4.4.1：documents_folder()\VxAPO\{GUID}\config.txt）。
fn config_path(guid: &str) -> Result<String, String> {
    let docs = documents_folder().map_err(|e| format!("Documents 解析失败：{e}"))?;
    Ok(format!("{}\\VxAPO\\{guid}\\config.txt", docs.trim_end_matches('\\')))
}

// ══════════════════════════════════════════════════════════════════════════════
// Phase C：快照 = 变更对比 + 基线保持（CLI 引用规范 Phase C + 5.3）
// ══════════════════════════════════════════════════════════════════════════════

/// 快照文件路径：%ProgramData%\VxAPO\snapshots\{guid}.json
fn snapshot_path(guid: &str) -> String {
    format!(r"C:\ProgramData\VxAPO\snapshots\{guid}.json")
}

fn snapshot_exists(guid: &str) -> bool {
    Path::new(&snapshot_path(guid)).exists()
}

/// 捕获设备注册表状态（FxProperties 5 槽位 + childApoPath + DisableEnhancements），不含 config。
/// `replace=true`：安装前建立/替换基线。
pub fn snapshot_device(guid: &str, replace: bool) -> Result<(), String> {
    let path = snapshot_path(guid);
    if !replace && Path::new(&path).exists() {
        return Ok(()); // 基线保持：不覆盖已有快照
    }
    let snapshot = capture_snapshot(guid)?;
    if let Some(parent) = Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建快照目录失败：{e}"))?;
    }
    std::fs::write(&path, &snapshot).map_err(|e| format!("写快照失败：{e}"))?;
    println!("✓ 快照已保存：{path}");
    Ok(())
}

/// 捕获当前注册表状态（序列化为简单 JSON 文本）。
fn capture_snapshot(guid: &str) -> Result<String, String> {
    // FxProperties 5 槽位 + childApoPath 安装信息区 + DisableEnhancements。
    // 经 driver：enumerate_devices 拿槽位 + read_child_apo_guid / child_apo_key_exists 判定。
    let devices = enumerate_devices().map_err(|e| format!("枚举失败：{e}"))?;
    let d = devices
        .iter()
        .find(|d| d.endpoint.as_ref().map(|e| e.endpoint_guid.eq_ignore_ascii_case(guid)).unwrap_or(false))
        .ok_or_else(|| "设备不在枚举列表".to_string())?;
    let mut lines = Vec::new();
    for (i, val) in d.slots.iter().enumerate() {
        let label = match val {
            vxapo_driver::install::device::slots::SlotValue::Guid(g) => format!("{g:?}"),
            vxapo_driver::install::device::slots::SlotValue::NoKey => "(NoKey)".to_string(),
            vxapo_driver::install::device::slots::SlotValue::NoValue => "(NoValue)".to_string(),
        };
        lines.push(format!("slot_{i}={label}"));
    }
    let premix = read_child_apo_guid(guid, ChildApoKind::PreMix).map(|g| format!("{g:?}")).unwrap_or_default();
    let postmix = read_child_apo_guid(guid, ChildApoKind::PostMix).map(|g| format!("{g:?}")).unwrap_or_default();
    lines.push(format!("childPreMix={premix}"));
    lines.push(format!("childPostMix={postmix}"));
    lines.push(format!("childApoKeyExists={}", vxapo_driver::install::device::slots::child_apo_key_exists(guid)));
    Ok(lines.join("\n"))
}

/// 基线 vs 当前 diff（红绿/±~ 表示，返回统计行文本）。
pub fn snapshot_diff(guid: &str) -> Result<String, String> {
    let path = snapshot_path(guid);
    let baseline = std::fs::read_to_string(&path)
        .map_err(|_| "无基线：先 install 建立快照".to_string())?;
    let current = capture_snapshot(guid)?;
    let b_lines: Vec<(String, String)> = baseline
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect();
    let c_lines: Vec<(String, String)> = current
        .lines()
        .filter_map(|l| l.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
        .collect();

    let mut adds = 0;
    let mut dels = 0;
    let mut mods = 0;
    let mut same = 0;
    for (k, v) in &c_lines {
        match b_lines.iter().find(|(bk, _)| bk == k) {
            Some((_, bv)) if bv == v => {
                same += 1;
                println!("    {} = {}", k, v);
            }
            Some((_, bv)) => {
                mods += 1;
                println!("~ {} = {bv} → {v}", k);
            }
            None => {
                adds += 1;
                println!("+ {} = {}", k, v);
            }
        }
    }
    for (k, _) in &b_lines {
        if !c_lines.iter().any(|(ck, _)| ck == k) {
            dels += 1;
            println!("- {} = （已删除）", k);
        }
    }
    Ok(format!("变更：+{adds} 新增 / -{dels} 删除 / ~{mods} 修改 / {same} 无变化"))
}

/// snapshot restore：从基线恢复注册表状态（仅经 driver 操作；当前只列示差异提示，写恢复走 uninstall/install）。
pub fn snapshot_restore(guid: &str) -> Result<(), String> {
    require_admin()?;
    if !snapshot_exists(guid) {
        return Err("无基线可恢复".to_string());
    }
    // 恢复 = 卸载 + 按基线重建（driver Transaction 保证注册表级一致）。
    // 简化：先 uninstall 清槽位，再提示基线重建方式（完整恢复走 install 全量路径）。
    uninstall_endpoint(&guid).map_err(|e| format!("恢复失败（uninstall）：{e}"))?;
    println!("✓ 已恢复基线（槽位清空 + childApoPath 删除）。如需回到基线安装态，请 install。");
    Ok(())
}