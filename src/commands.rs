//! CLI 命令层（CLI 引用规范 四/五：install/uninstall/config/status/snapshot）
//!
//! 分层：CLI 命令（参数解析）→ 本层辅助（resolve/snapshot）→ driver API（install 层唯一写入口）。
//! 约束：不触碰 pipeline/RT；不自行写注册表（经 driver Transaction）；config 写归 CLI。

use std::path::Path;

use vxapo_driver::{
    child_apo_key_exists, cleanup_orphan, device_config_path, enumerate_devices, find_endpoint_path,
    fix_config_acl, guid_to_string, install_endpoint, list_stale_installs, migrate_install,
    read_child_apo_guid, register_apo_with_path, snapshot_dir, uninstall_endpoint, ChildApoKind,
    InstallConfig, CLSID_VXAPO_POST_MIX, CLSID_VXAPO_PRE_MIX,
};

use crate::i18n::{Lang, lang, tr};
use crate::knowledge::KNOWN_APO_CLSIDS;
use crate::verify::emit_phase;

/// JSON 字符串转义（无依赖手写最小实现）。
pub(crate) fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// 设备三元组（resolve_device 产物，CLI 引用规范 4.4.1）。
pub struct DeviceRef {
    pub guid: String,
    pub name: String,
    pub connection: String,
}

/// 解析 `<device>`：GUID（规范形式）或枚举序号 → (guid, name, connection)。
pub fn resolve_device(device_ref: &str) -> Result<DeviceRef, String> {
    let devices = enumerate_devices().map_err(|e| {
        if lang() == Lang::En {
            format!("Failed to enumerate devices: {e}")
        } else {
            format!("枚举设备失败：{e}")
        }
    })?;
    if device_ref.starts_with('{') {
        let guid = device_ref.to_owned();
        // GUID 格式校验（{xxxxxxxx-...}，len=38）
        if guid.len() != 38 || !guid.ends_with('}') {
            if lang() == Lang::En {
                return Err(format!("Invalid GUID: <{device_ref}> (expected {{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}})"));
            } else {
                return Err(format!("无效 GUID：<{device_ref}>（应为 {{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}}）"));
            }
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
        let d = devices.get(idx).ok_or_else(|| {
            if lang() == Lang::En {
                format!("Enumeration index out of range: <{device_ref}> (0..{})", devices.len().saturating_sub(1))
            } else {
                format!("枚举序号越界：<{device_ref}>（0..{}）", devices.len().saturating_sub(1))
            }
        })?;
        let ep = d.endpoint.as_ref().ok_or_else(|| {
            if lang() == Lang::En {
                "Device missing endpoint info".to_string()
            } else {
                "设备缺端点信息".to_string()
            }
        })?;
        return Ok(DeviceRef { guid: ep.endpoint_guid.clone(), name: ep.friendly_name.clone(), connection: String::new() });
    }
    if lang() == Lang::En {
        Err(format!("Unknown device: <{device_ref}> (use list to see indices, or use {{GUID}})"))
    } else {
        Err(format!("未知设备：<{device_ref}>（可用 list 查看序号，或用 {{GUID}} 形式）"))
    }
}

/// 管理员检查（install/uninstall 需 HKLM 写权限）。
///
/// 真实写验证：create 探测键后**写入一个值**再删除——仅 create/open 成功不够
/// （键已存在时无写权限的用户也可能打开成功，导致 install 阶段才报 0x80070005）。
pub fn require_admin() -> Result<(), String> {
    let probe_key = r"SOFTWARE\VxAPO\CLIProbe";
    let root = windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    match vxapo_driver::RegKey::create(root, probe_key) {
        Ok(key) => {
            // 写入探测值：有 HKLM 写权限才成功。
            let probe_ok = key.write_sz("CLIProbe", "1").is_ok();
            let _ = key.delete_value("CLIProbe");
            drop(key);
            let _ = vxapo_driver::RegKey::open(root, probe_key)
                .and_then(|k| k.delete_sub_key(probe_key));
            if probe_ok {
                Ok(())
            } else {
                if lang() == Lang::En {
                Err("Administrator privileges required (write to HKLM). Please run CLI as Administrator.".to_string())
            } else {
                Err("需要管理员权限（写入 HKLM）。请以管理员身份运行 CLI（右键→以管理员身份运行）。".to_string())
            }
            }
        }
        Err(_) => if lang() == Lang::En {
                Err("Administrator privileges required (write to HKLM). Please run CLI as Administrator.".to_string())
            } else {
                Err("需要管理员权限（写入 HKLM）。请以管理员身份运行 CLI（右键→以管理员身份运行）。".to_string())
            },
    }
}

// per-device config 路径与快照目录由 driver 提供（`device_config_path` / `snapshot_dir`），
// 保证 CLI 与 APO（audiodg，SYSTEM 服务）读同一份 `C:\ProgramData\VxAPO\...` 布局。

// ── 子模块（注册 / 状态 / 安装 / 卸载 / 配置转换 / 快照）──────────────────

mod convert;
mod install;
mod register;
mod snapshot;
mod status;
mod uninstall;

pub use convert::{config_convert, config_set, config_show};
pub use install::{
    install, preview_install, stale_cleanup, stale_fix_acl, stale_list, stale_migrate,
};
pub use register::register;
pub use snapshot::{snapshot_device, snapshot_diff, snapshot_restore};
pub use status::{list_devices, show_device_status};
pub use uninstall::uninstall;
