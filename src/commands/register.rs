//! commands/register.rs — driver DLL 定位与 COM 类注册

use super::*;

/// 定位 exe 同级 vxapo_driver.dll 并自动注册 COM 类（新机器无绑定）。
///
/// 新开发者拿到 CLI + driver 二进制直接 `install` 时，注册表里没有 CLSID 绑定
/// （未跑过 regsvr32）→ verify(CoCreateInstance) 会 0x80040154。CLI 安装前
/// 自动从 exe 同级找 vxapo_driver.dll 并调 driver 的 `register_apo_with_path`，
/// 使 CLSID → DLL 路径绑定就绪。
///
/// 找不到 DLL 不阻塞（可能已由安装器/regsvr32 预注册，verify 通过即可）。
pub(super) fn auto_register_driver() -> Result<(), String> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();
    let dll_candidates = [
        exe_dir.join("vxapo_driver.dll"),
        exe_dir.join("resources").join("vxapo_driver.dll"),
        exe_dir.parent().map(|p| p.join("resources").join("vxapo_driver.dll")).unwrap_or_default(),
    ];
    let dll_path = if let Some(dll) = dll_candidates.iter().find(|p| p.exists()) {
        dll.display().to_string()
    } else if driver_binding_exists() {
        // 已存在 CLSID→DLL 绑定：用注册表里的路径刷新注册。
        // 旧版注册可能缺 AudioEngine\AudioProcessingObjects 键/字段，
        // 只按「绑定存在」跳过会导致父槽位仍不加载。
        let clsid_str = guid_to_string(&CLSID_VXAPO_PRE_MIX);
        vxapo_driver::RegKey::open(
            windows::Win32::System::Registry::HKEY_CLASSES_ROOT,
            &format!(r"CLSID\{}\InprocServer32", clsid_str),
        )
        .ok()
        .and_then(|k| k.read_sz_value("").ok())
        .filter(|p| !p.is_empty())
        .unwrap_or_default()
    } else {
        String::new()
    };
    if dll_path.is_empty() {
        if lang() == Lang::En {
            println!("  ⚠ {} not found (skipping auto-register - no problem if already registered by installer/regsvr32)", dll_candidates[0].display());
        } else {
            println!("  ⚠ 未找到 {}（跳过自动注册——已由安装器/regsvr32 注册则无碍）", dll_candidates[0].display());
        }
        return Ok(());
    }
    let hr = register_apo_with_path(&dll_path);
    if hr.0 == 0 {
        if lang() == Lang::En {
            println!("  ✓ Global APO registration refreshed: {dll_path}");
        } else {
            println!("  ✓ 已刷新全局 APO 注册：{dll_path}");
        }
    } else {
        if lang() == Lang::En {
            return Err(format!("Driver auto-registration failed: {hr:?}"));
        } else {
            return Err(format!("driver 自动注册失败：{hr:?}"));
        }
    }
    // 回读验证 CLSID 绑定（PreMix 即可，两者同路径）。
    let clsid_str = guid_to_string(&CLSID_VXAPO_PRE_MIX);
    let check = vxapo_driver::RegKey::open(
        windows::Win32::System::Registry::HKEY_CLASSES_ROOT,
        &format!(r"CLSID\{}\InprocServer32", clsid_str),
    );
    match check {
        Ok(k) => match k.read_sz_value("") {
            Ok(p) => {
                    if lang() == Lang::En {
                        println!("  ✓ CLSID->DLL binding confirmed: {p}");
                    } else {
                        println!("  ✓ CLSID→DLL 绑定确认：{p}");
                    }
                }
            Err(e) => {
                    if lang() == Lang::En {
                        return Err(format!("CLSID binding read-back failed: {e}"));
                    } else {
                        return Err(format!("CLSID 绑定回读失败：{e}"));
                    }
                }
        },
        Err(e) => {
                    if lang() == Lang::En {
                        return Err(format!("CLSID binding verification failed: {e}"));
                    } else {
                        return Err(format!("CLSID 绑定验证失败：{e}"));
                    }
                }
    }

    // 回读验证 AudioEngine APO 注册键：缺失会导致引擎静默拒载。
    let ae_path = format!(r"AudioEngine\AudioProcessingObjects\{}", clsid_str);
    match vxapo_driver::RegKey::open(
        windows::Win32::System::Registry::HKEY_CLASSES_ROOT,
        &ae_path,
    ) {
        Ok(_) => {
            if lang() == Lang::En {
                println!("  ✓ AudioEngine APO registration confirmed: {ae_path}");
            } else {
                println!("  ✓ AudioEngine APO 注册键确认：{ae_path}");
            }
        }
        Err(e) => {
            if lang() == Lang::En {
                return Err(format!("AudioEngine APO registration missing: {ae_path} ({e})"));
            } else {
                return Err(format!("AudioEngine APO 注册键缺失：{ae_path}（{e}）"));
            }
        }
    }

    Ok(())
}

/// 校验 CLSID→DLL 绑定是否已存在（避免每次 install 重复注册）。
pub(super) fn driver_binding_exists() -> bool {
    let clsid_str = guid_to_string(&CLSID_VXAPO_PRE_MIX);
    vxapo_driver::RegKey::open(
        windows::Win32::System::Registry::HKEY_CLASSES_ROOT,
        &format!(r"CLSID\{}\InprocServer32", clsid_str),
    )
    .is_ok()
}

/// 注册随包 driver DLL 的 CLSID 绑定（NSIS 安装后调用；幂等）。
pub fn register() -> Result<(), String> {
    auto_register_driver()
}

