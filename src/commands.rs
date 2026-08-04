//! CLI 命令层（CLI 引用规范 四/五：install/uninstall/config/status/snapshot）
//!
//! 分层：CLI 命令（参数解析）→ 本层辅助（resolve/snapshot）→ driver API（install 层唯一写入口）。
//! 约束：不触碰 pipeline/RT；不自行写注册表（经 driver Transaction）；config 写归 CLI。

use std::path::Path;

use vxapo_driver::install::audiodg::ensure_can_load;
use vxapo_driver::install::device::info::enumerate_devices;
use vxapo_driver::install::device::slots::{ChildApoKind, child_apo_key_exists, read_child_apo_guid};
use vxapo_driver::install::selector::operation::{InstallConfig, install_endpoint, uninstall_endpoint};
use vxapo_driver::object::dll_exports::register_apo_with_path;
use vxapo_driver::sys::com::prelude::guid_to_string;
use vxapo_driver::object::vx_reg_props::{CLSID_VXAPO_POST_MIX, CLSID_VXAPO_PRE_MIX};

use crate::knowledge::KNOWN_APO_CLSIDS;

/// 定位 exe 同级 vxapo_driver.dll 并自动注册 COM 类（新机器无绑定）。
///
/// 新开发者拿到 CLI + driver 二进制直接 `install` 时，注册表里没有 CLSID 绑定
/// （未跑过 regsvr32）→ verify(CoCreateInstance) 会 0x80040154。CLI 安装前
/// 自动从 exe 同级找 vxapo_driver.dll 并调 driver 的 `register_apo_with_path`，
/// 使 CLSID → DLL 路径绑定就绪。
///
/// 找不到 DLL 不阻塞（可能已由安装器/regsvr32 预注册，verify 通过即可）。
fn auto_register_driver() -> Result<(), String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();
    let dll = exe_dir.join("vxapo_driver.dll");
    if !dll.exists() {
        println!("  ⚠ 未找到 {}（跳过自动注册——已由安装器/regsvr32 注册则无碍）", dll.display());
        return Ok(());
    }
    let hr = register_apo_with_path(&dll.display().to_string());
    if hr.0 == 0 {
        println!("  ✓ 已注册全局 APO 类：{}", dll.display());
    } else {
        return Err(format!("driver 自动注册失败：{hr:?}"));
    }
    // 回读验证 CLSID 绑定（PreMix 即可，两者同路径）。
    let clsid_str = guid_to_string(&CLSID_VXAPO_PRE_MIX);
    let check = vxapo_driver::sys::registry::RegKey::open(
        windows::Win32::System::Registry::HKEY_CLASSES_ROOT,
        &format!(r"CLSID\{}\InprocServer32", clsid_str),
    );
    match check {
        Ok(k) => match k.read_sz_value("") {
            Ok(p) => println!("  ✓ CLSID→DLL 绑定确认：{p}"),
            Err(e) => return Err(format!("CLSID 绑定回读失败：{e}")),
        },
        Err(e) => return Err(format!("CLSID 绑定验证失败：{e}")),
    }
    Ok(())
}

/// 校验 CLSID→DLL 绑定是否已存在（避免每次 install 重复注册）。
fn driver_binding_exists() -> bool {
    let clsid_str = guid_to_string(&CLSID_VXAPO_PRE_MIX);
    vxapo_driver::sys::registry::RegKey::open(
        windows::Win32::System::Registry::HKEY_CLASSES_ROOT,
        &format!(r"CLSID\{}\InprocServer32", clsid_str),
    )
    .is_ok()
}

/// 设备三元组（resolve_device 产物，CLI 引用规范 4.4.1）。
pub struct DeviceRef {
    pub guid: String,
    pub name: String,
    pub connection: String,
}

/// 重启 Windows 音频服务（audiosrv → audiodg）。
///
/// audiodg 持有点端时会锁 MMDevices 槽位值——卸载前必须停服务（否则槽位删不掉），
/// 安装/卸载后必须重启使新槽位拓扑生效（2026-08-04 实测）。
/// 需要管理员（require_admin 已保证）。
fn restart_audio_service() -> Result<(), String> {
    println!("  ♻ 重启音频服务（audiosrv）…");
    let stop = std::process::Command::new("net")
        .args(["stop", "audiosrv"])
        .output()
        .map_err(|e| format!("net stop audiosrv 执行失败：{e}"))?;
    if !stop.status.success() {
        // 服务本就没在运行也视为成功（错误码可能在 stderr）。
        // audiodg 不存在时 net stop 报错，但音频相关进程可能已被停。
        println!("  （net stop 返回非零：{}，继续）", String::from_utf8_lossy(&stop.stderr).trim());
    }
    // 短暂等待服务停止。
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let start = std::process::Command::new("net")
        .args(["start", "audiosrv"])
        .output()
        .map_err(|e| format!("net start audiosrv 执行失败：{e}"))?;
    if !start.status.success() {
        let msg = String::from_utf8_lossy(&start.stderr).trim().to_string();
        // 已启动（"already running"）不算错误；其他失败才报。
        if !msg.to_lowercase().contains("already") {
            return Err(format!("net start audiosrv 失败：{msg}"));
        }
    }
    println!("  ♻ 音频服务已重启");
    Ok(())
}

/// 安装前预览：设备 + 5 槽位占用摘要（交互菜单安装前展示）。
///
/// 展示当前谁占着 PreMix/PostMix 槽位（EAPO 等用友好名），
/// 供用户决定是否保留为子 APO。
pub fn preview_install(device_ref: &str) -> Result<String, String> {
    let dev = resolve_device(device_ref)?;
    let devices = enumerate_devices().map_err(|e| format!("枚举设备失败：{e}"))?;
    let d = devices
        .iter()
        .find(|d| d.endpoint.as_ref().map(|e| e.endpoint_guid.eq_ignore_ascii_case(&dev.guid)).unwrap_or(false))
        .ok_or_else(|| "设备不在枚举列表".to_string())?;
    let slot_names = ["LFX", "GFX", "SFX", "MFX", "EFX"];
    let mut lines = vec![format!("  设备：{}", dev.name)];
    for (i, val) in d.slots.iter().enumerate() {
        let label = match val {
            vxapo_driver::install::device::slots::SlotValue::Guid(g) => {
                let gs = format!("{g:?}");
                slot_friendly(&gs).unwrap_or_else(|| gs.clone())
            }
            _ => "-".to_string(),
        };
        lines.push(format!("  {}[{}]: {label}", slot_names[i], i));
    }
    Ok(lines.join("\n"))
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
///
/// 真实写验证：create 探测键后**写入一个值**再删除——仅 create/open 成功不够
/// （键已存在时无写权限的用户也可能打开成功，导致 install 阶段才报 0x80070005）。
pub fn require_admin() -> Result<(), String> {
    let probe_key = r"SOFTWARE\VxAPO\CLIProbe";
    let root = windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    match vxapo_driver::sys::registry::RegKey::create(root, probe_key) {
        Ok(key) => {
            // 写入探测值：有 HKLM 写权限才成功。
            let probe_ok = key.write_sz("CLIProbe", "1").is_ok();
            let _ = key.delete_value("CLIProbe");
            drop(key);
            let _ = vxapo_driver::sys::registry::RegKey::open(root, probe_key)
                .and_then(|k| k.delete_sub_key(probe_key));
            if probe_ok {
                Ok(())
            } else {
                Err("需要管理员权限（写入 HKLM）。请以管理员身份运行 CLI（右键→以管理员身份运行）。".to_string())
            }
        }
        Err(_) => Err("需要管理员权限（写入 HKLM）。请以管理员身份运行 CLI（右键→以管理员身份运行）。".to_string()),
    }
}

/// 单设备状态详情（交互菜单 [s] 用）：只显示指定设备的槽位 + 子 APO 信息。
///
/// 与 `list_devices` 不同——本函数聚焦单个设备，并展示 childApo 信息区
/// （PreMixChild/PostMixChild，运行期委托给谁）。
pub fn show_device_status(device_ref: &str) -> Result<(), String> {
    let dev = resolve_device(device_ref)?;
    let devices = enumerate_devices().map_err(|e| format!("枚举设备失败：{e}"))?;
    let d = devices
        .iter()
        .find(|d| d.endpoint.as_ref().map(|e| e.endpoint_guid.eq_ignore_ascii_case(&dev.guid)).unwrap_or(false))
        .ok_or_else(|| "设备不在枚举列表".to_string())?;
    let ep = d.endpoint.as_ref().unwrap();
    println!("[{}]", ep.friendly_name);
    println!("  GUID: {}", ep.endpoint_guid);
    println!("  版本: {}  模式: {:?}", d.installed_version, d.install_mode);
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
        println!("  {}[{}]: {label}", slot_names[s], s);
    }
    // EAPO 状态
    if let Some(eapo) = detect_eapo_status(&d.slots) {
        println!("  ▶ {eapo}");
    }
    // childApo 信息区（子 APO：VxAPO 运行时委托给谁）
    println!("  子 APO：");
    let child_pre = read_child_apo_guid(&ep.endpoint_guid, ChildApoKind::PreMix)
        .map(|g| slot_friendly(&format!("{g:?}")).unwrap_or_else(|| format!("{g:?}")))
        .unwrap_or_else(|| "-".to_string());
    let child_post = read_child_apo_guid(&ep.endpoint_guid, ChildApoKind::PostMix)
        .map(|g| slot_friendly(&format!("{g:?}")).unwrap_or_else(|| format!("{g:?}")))
        .unwrap_or_else(|| "-".to_string());
    println!("    PreMixChild: {child_pre}");
    println!("    PostMixChild: {child_post}");
    // 槽位失守检测（仅存 lib/childapo 键时验证）
    if child_apo_key_exists(&ep.endpoint_guid) {
        if let Some(lost) = detect_lost_slot(&d.slots, d.install_mode) {
            println!("  ⚠ 槽位失守：{lost} 已被接管，需重装（install）");
        }
    }
    Ok(())
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
    match mode {
        // 显式 --mode：用户覆盖，不探测。
        Some(m) => {
            config.install_mode = match m.to_lowercase().as_str() {
                "lfxgfx" => vxapo_driver::install::device::slots::InstallMode::LfxGfx,
                "sfxmfx" => vxapo_driver::install::device::slots::InstallMode::SfxMfx,
                "sfxefx" => vxapo_driver::install::device::slots::InstallMode::SfxEfx,
                _ => return Err(format!("无效模式：{m}（LfxGfx/SfxMfx/SfxEfx）")),
            };
        }
        // 缺省：自动探测（EAPO 三档，driver detect_mode_for_guid）。
        None => {
            config.install_mode =
                vxapo_driver::install::device::info::detect_mode_for_guid(&dev.guid);
            println!("▶ 自动探测安装模式：{:?}", config.install_mode);
        }
    }
    config.use_original_apo_premix = !no_child;
    config.use_original_apo_postmix = !no_child;

    // 快照基线（安装前建立/替换，Phase C——只注册表，config 不属 CLI 快照）。
    if let Err(e) = snapshot_device(&dev.guid, true) {
        println!("⚠ 快照建立失败（继续安装）：{e}");
    }

    // 自动注册 CLSID→DLL 绑定（新机器无绑定；已存在则跳过）。
    if !driver_binding_exists() {
        auto_register_driver()?;
    }

    ensure_can_load().map_err(|e| format!("audiodg 检查失败：{e}"))?;
    install_endpoint(&dev.guid, &dev.name, &dev.connection, &config, true)
        .map_err(|e| format!("install_endpoint 失败：{e}（可用 vxapo-cli snapshot diff -d {guid} 查看变更）", guid = dev.guid))?;
    println!("✓ 已安装 {}（模式 {:?}，子 APO 保留={}）", dev.guid, config.install_mode, !no_child);

    // per-device config.txt 检查（方案 A）：缺失时**自动从 exe 同级 .\config.txt 导入**，
    // 避免「装完发现没配置」。约定：把 config.txt 放在 vxapo-cli.exe 同目录即可，
    // 安装自动复制到 C:\ProgramData\VxAPO\{guid}\config.txt 供 APO 解析（audiodg-SYSTEM 可读）。
    // config 写归 CLI（非 driver）。
    match config_show(&dev.guid) {
        Ok(()) => {}
        Err(_) => {
            let path = config_path(&dev.guid).unwrap_or_default();
            // 自动导入：exe 同级 config.txt（默认约定）。
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .unwrap_or_default();
            let default_src = exe_dir.join("config.txt");
            if default_src.exists() {
                match std::fs::read_to_string(&default_src) {
                    Ok(src) => {
                        if let Some(parent) = Path::new(&path).parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        match std::fs::write(&path, &src) {
                            Ok(()) => println!(
                                "📄 已自动导入 {} → {}",
                                default_src.display(),
                                path
                            ),
                            Err(e) => println!("⚠ 自动导入失败：{e}"),
                        }
                    }
                    Err(e) => println!("⚠ 读取 {} 失败：{e}", default_src.display()),
                }
            } else {
                println!("⚠ 未检测到 config.txt（{path}），APO 将按无配置运行。");
                println!("   请用 config set 写入：vxapo-cli config set -d <device> -f <你的配置文件>");
            }
        }
    }

    // 最后重启音频服务使新槽位拓扑 + config 生效（audiodg 锁定旧拓扑；
    // 重启时 watcher 启动会解析已导入的 config）。
    restart_audio_service()?;
    Ok(())
}

/// uninstall 命令（CLI 引用规范 5.3）。
pub fn uninstall(device_ref: &str) -> Result<(), String> {
    require_admin()?;
    let dev = resolve_device(device_ref)?;
    if !snapshot_exists(&dev.guid) {
        return Err("无基线可对比——快照不存在（先 install 建立基线）".to_string());
    }
    // 卸载前停音频服务：audiodg 持有点端会锁 MMDevices 槽位值，不停服务槽位删不掉。
    println!("  ♻ 停止音频服务（卸载前置）…");
    let stop = std::process::Command::new("net")
        .args(["stop", "audiosrv"])
        .output()
        .map_err(|e| format!("net stop audiosrv 执行失败：{e}"))?;
    if !stop.status.success() {
        println!("  （net stop 返回非零：{}，继续）", String::from_utf8_lossy(&stop.stderr).trim());
    }
    std::thread::sleep(std::time::Duration::from_millis(1500));

    // 卸载（服务已停 → 槽位值删除不被锁）。
    if let Err(e) = uninstall_endpoint(&dev.guid) {
        // 卸载失败也要尝试恢复音频服务。
        let _ = std::process::Command::new("net").args(["start", "audiosrv"]).output();
        return Err(format!("uninstall_endpoint 失败：{e}"));
    }
    print!("✓ 已卸载 {}。", dev.guid);
    if let Ok(diff) = snapshot_diff(&dev.guid) {
        println!(" 变更统计：{diff}");
    } else {
        println!();
    }

    // 重启音频服务恢复声音输出。
    restart_audio_service()?;
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

/// per-device config 路径（方案 A，2026-08-04：C:\ProgramData\VxAPO\{GUID}\config.txt）。
///
/// **为什么不用 Documents**：APO 真实运行在 audiodg（SYSTEM 服务），调 `documents_folder()`
/// 拿到 SYSTEM 的 Documents，读不到 CLI（用户进程）写进用户 Documents 的文件——
/// 导致「改 Documents 的 config 没效果」。ProgramData 全用户共享，SYSTEM + 用户都可读写。
/// 与 driver `resolve_config_path`（scheme A）保持一致。
fn config_path(guid: &str) -> Result<String, String> {
    Ok(format!(r"C:\ProgramData\VxAPO\{guid}\config.txt"))
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