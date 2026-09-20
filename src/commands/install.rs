//! commands/install.rs — 安装编排（含旧 GUID 残留处理）

use super::*;
use super::register::*;
use super::snapshot::*;
use super::status::*;

/// 安装前预览：设备 + 5 槽位占用摘要（交互菜单安装前展示）。
///
/// 展示当前谁占着 PreMix/PostMix 槽位（EAPO 等用友好名），
/// 供用户决定是否保留为子 APO。
pub fn preview_install(device_ref: &str) -> Result<String, String> {
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
    let slot_names = ["LFX", "GFX", "SFX", "MFX", "EFX"];
    let mut lines = vec![if lang() == Lang::En {
        format!("  Device: {}", dev.name)
    } else {
        format!("  设备：{}", dev.name)
    }];
    for (i, val) in d.slots.iter().enumerate() {
        let label = match val {
            vxapo_driver::SlotValue::Guid(g) => {
                let gs = format!("{g:?}");
                slot_friendly(&gs).unwrap_or_else(|| gs.clone())
            }
            _ => "-".to_string(),
        };
        lines.push(format!("  {}[{}]: {label}", slot_names[i], i));
    }
    Ok(lines.join("\n"))
}

/// install 命令（CLI 引用规范 5.1）。
pub fn install(
    device_ref: &str,
    mode: Option<&str>,
    no_child: bool,
    json: bool,
    verify: bool,
    timeout_secs: u64,
    progress_file: Option<&Path>,
) -> Result<(), String> {
    require_admin()?;
    emit_phase(progress_file, "admin");
    emit_phase(progress_file, "resolve");
    let dev = resolve_device(device_ref)?;
    let mut config = InstallConfig::default_config();
    match mode {
        // 显式 --mode：用户覆盖，不探测。
        Some(m) => {
            config.install_mode = match m.to_lowercase().as_str() {
                "lfxgfx" => vxapo_driver::InstallMode::LfxGfx,
                "sfxmfx" => vxapo_driver::InstallMode::SfxMfx,
                "sfxefx" => vxapo_driver::InstallMode::SfxEfx,
                _ => {
                        if lang() == Lang::En {
                            return Err(format!("Invalid mode: {m} (LfxGfx/SfxMfx/SfxEfx)"));
                        } else {
                            return Err(format!("无效模式：{m}（LfxGfx/SfxMfx/SfxEfx）"));
                        }
                    }
            };
        }
        // 缺省：自动探测（EAPO 三档，driver detect_mode_for_guid）。
        None => {
            config.install_mode =
                vxapo_driver::detect_mode_for_guid(&dev.guid);
            if !json {
                if lang() == Lang::En {
                    println!("▶ Auto-detected install mode: {:?}", config.install_mode);
                } else {
                    println!("▶ 自动探测安装模式：{:?}", config.install_mode);
                }
            }
        }
    }
    config.use_original_apo_premix = !no_child;
    config.use_original_apo_postmix = !no_child;

    // 快照基线（安装前建立/替换；只注册表，config 不属 CLI 快照）。
    emit_phase(progress_file, "snapshot");
    if let Err(e) = snapshot_device(&dev.guid, true) {
        if !json {
            if lang() == Lang::En {
                    println!("⚠ Snapshot creation failed (continuing): {e}");
                } else {
                    println!("⚠ 快照建立失败（继续安装）：{e}");
                }
        }
    }

    // 每次安装都刷新全局 APO 注册（幂等）。
    // 旧机器可能已有 CLSID→DLL 绑定，但 AudioEngine\AudioProcessingObjects 键
    // 是早期缺字段/缺 MaxInstances 的旧注册——只按「绑定存在」跳过会继续拒载。
    emit_phase(progress_file, "register");
    auto_register_driver()?;

    if verify {
        // 先确保 config.toml 存在（缺省自动导入），再进入验证流程，
        // 使验证通过时的状态即最终可用状态。
        emit_phase(progress_file, "config");
        ensure_default_config(&dev, json);
        emit_phase(progress_file, "verify");
        crate::verify::install_verify(&dev, &config, timeout_secs, progress_file)
    } else {
        // DisableProtectedAudioDG、槽位/ProcessingModes 写入和安装后重启
        // 均由 driver install_endpoint 全流程处理（CLI 不再重复）。
        install_endpoint(&dev.guid, &dev.name, &dev.connection, &config, true)
            .map_err(|e| {
                if lang() == Lang::En {
                    format!("install_endpoint failed: {e} (use vxapo-cli snapshot diff -d {} to view changes)", dev.guid)
                } else {
                    format!("install_endpoint 失败：{e}（可用 vxapo-cli snapshot diff -d {} 查看变更）", dev.guid)
                }
            })?;
        if json {
            println!(
                "{{\"ok\":true,\"device\":\"{}\",\"mode\":\"{:?}\",\"message\":\"已安装\"}}",
                json_escape(&dev.guid),
                config.install_mode
            );
        } else {
            if lang() == Lang::En {
                println!("✓ Installed {} (mode {:?}, child APO keep={})", dev.guid, config.install_mode, !no_child);
            } else {
                println!("✓ 已安装 {}（模式 {:?}，子 APO 保留={}）", dev.guid, config.install_mode, !no_child);
            }
        }

        // 重启音频服务（依赖服务感知 + 轮询 RUNNING，best-effort），
        // 确保 audiodg 重新加载新注册的 APO。
        if !json {
            if lang() == Lang::En {
                println!("  Restarting audio service to apply changes...");
            } else {
                println!("  正在重启音频服务以应用变更…");
            }
        }
        let _ = vxapo_driver::restart_audio_service_wait(10, 15);

        ensure_default_config(&dev, json);
        Ok(())
    }
}

/// per-device config.toml 检查：缺失时自动从 exe 同级 `.\config.toml` 导入，
/// 避免「装完发现没配置」。约定：把 config.toml 放在 vxapo-cli.exe 同目录即可，
/// 安装自动复制到 C:\ProgramData\VxAPO\{guid}\config.toml 供 APO 解析。
pub(super) fn ensure_default_config(dev: &DeviceRef, json: bool) {
    match config_show(&dev.guid) {
        Ok(()) => {}
        Err(_) => {
            let path = device_config_path(&dev.guid);
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .unwrap_or_default();
            let default_src = exe_dir.join("config.toml");
            if default_src.exists() {
                match std::fs::read_to_string(&default_src) {
                    Ok(src) => {
                        if let Some(parent) = Path::new(&path).parent() {
                            let _ = std::fs::create_dir_all(parent);
                        }
                        match std::fs::write(&path, &src) {
                            Ok(()) => {
                                if !json {
                                    if lang() == Lang::En {
                                        println!("📄 Auto-imported {} -> {}", default_src.display(), path);
                                    } else {
                                        println!("📄 已自动导入 {} → {}", default_src.display(), path);
                                    }
                                }
                            }
                            Err(e) => {
                                if !json {
                                    if lang() == Lang::En {
                                        println!("⚠ Auto-import failed: {e}");
                                    } else {
                                        println!("⚠ 自动导入失败：{e}");
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if !json {
                            if lang() == Lang::En {
                                println!("⚠ Failed to read {}: {e}", default_src.display());
                            } else {
                                println!("⚠ 读取 {} 失败：{e}", default_src.display());
                            }
                        }
                    }
                }
            } else if !json {
                if lang() == Lang::En {
                    println!("⚠ No config.toml detected ({path}); APO will run without configuration.");
                    println!("   Use: vxapo-cli config set -d <device> -f <your config file>");
                } else {
                    println!("⚠ 未检测到 config.toml（{path}），APO 将按无配置运行。");
                    println!("   请用 config set 写入：vxapo-cli config set -d <device> -f <你的配置文件>");
                }
            }
        }
    }
}

/// 旧 GUID 残留列表：扫描软件信息区 / ProgramData，并匹配当前活跃端点。
pub fn stale_list(json: bool) -> Result<(), String> {
    let list = list_stale_installs().map_err(|e| e.to_string())?;
    if json {
        println!(
            "{}",
            serde_json::to_string(&list).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if list.is_empty() {
        println!("{}", tr("（无旧 GUID 残留）", "(no stale GUID records)"));
        return Ok(());
    }
    for item in list {
        println!("{}  {}", item.guid, item.target_state);
        println!(
            "     device: {}  matched_by: {}",
            if item.device_instance_id.is_empty() {
                "-"
            } else {
                item.device_instance_id.as_str()
            },
            item.matched_by.clone().unwrap_or_else(|| "-".to_string())
        );
        if let Some(target) = item.target_guid {
            println!(
                "     target: {}  {}",
                target,
                item.target_name.unwrap_or_default()
            );
        }
        println!(
            "     mode: {}  config: {}  snapshot: {}",
            item.inferred_mode,
            item.config_path.unwrap_or_else(|| "-".to_string()),
            item.snapshot_path.unwrap_or_else(|| "-".to_string())
        );
    }
    Ok(())
}

/// 迁移旧 GUID 安装到当前端点，并清理旧记录。
pub fn stale_migrate(
    from: &str,
    to: &str,
    config_from: Option<&str>,
    snapshot_from: Option<&str>,
    json: bool,
) -> Result<(), String> {
    require_admin()?;
    // 迁移**不停服**：写/删端点 FxProperties 值只需要句柄具备 KEY_SET_VALUE
    // （`RegKey::open_for_write` 即是），与 audiodg 是否持有点端无关——在活动音频流
    // 上删除槽位值同样成功。因此这里既不 stop AudioSrv 也不 taskkill audiodg；
    // 修复分支若真的改写了槽位，由 driver 在写完后**重启端点**让变更生效
    // （引擎会缓存端点 APO 链，只改注册表不会立刻重载）。
    let report = migrate_install(from, to, config_from, snapshot_from).map_err(|e| e.to_string())?;
    if json {
        println!(
            "{}",
            serde_json::to_string(&report).map_err(|e| e.to_string())?
        );
    } else {
        println!(
            "{} {} -> {}",
            tr("✓ 已迁移", "✓ migrated"),
            from,
            to
        );
        if !report.warnings.is_empty() {
            for w in report.warnings {
                eprintln!("⚠ {w}");
            }
        }
    }
    // 兜底：确保音频服务处于运行状态（已运行则幂等返回，不重启）。
    if let Err(e) = vxapo_driver::ensure_audio_service_running() {
        eprintln!("⚠ 确保音频服务运行失败：{e}");
    }
    Ok(())
}

/// 清理无法匹配活跃端点的旧 GUID 记录。
pub fn stale_cleanup(guid: &str, json: bool) -> Result<(), String> {
    require_admin()?;
    cleanup_orphan(guid).map_err(|e| e.to_string())?;
    if json {
        println!(
            "{{\"ok\":true,\"device\":\"{}\",\"message\":\"cleaned\"}}",
            json_escape(guid)
        );
    } else {
        println!("✓ {} {guid}", tr("已清理", "cleaned"));
    }
    Ok(())
}

/// 修复迁移后 config/snapshot 的用户 ACL。
pub fn stale_fix_acl(guid: &str, json: bool) -> Result<(), String> {
    require_admin()?;
    fix_config_acl(guid).map_err(|e| e.to_string())?;
    if json {
        println!(
            "{{\"ok\":true,\"device\":\"{}\",\"message\":\"acl-fixed\"}}",
            json_escape(guid)
        );
    } else {
        println!("✓ {guid} ACL fixed");
    }
    Ok(())
}

