//! commands/uninstall.rs — 卸载编排

use super::*;
use super::snapshot::*;

/// uninstall 命令（CLI 引用规范 5.3）。
pub fn uninstall(device_ref: &str, json: bool) -> Result<(), String> {
    require_admin()?;
    let dev = resolve_device(device_ref)?;
    // 端点键已被 Windows 重新枚举移除，但 VxAPO 自己的旧记录还在：
    // 直接走残留清理，避免 find_endpoint_path 失败后留下 Child APOs / 配置目录。
    if find_endpoint_path(&dev.guid).is_err() {
        let stale = list_stale_installs().map_err(|e| e.to_string())?;
        if stale
            .iter()
            .any(|s| s.guid.eq_ignore_ascii_case(&dev.guid))
        {
            return stale_cleanup(&dev.guid, json);
        }
    }
    if !snapshot_exists(&dev.guid) {
        if lang() == Lang::En {
            return Err("No baseline to compare - snapshot does not exist (run install first to create one)".to_string());
        } else {
            return Err("无基线可对比——快照不存在（先 install 建立基线）".to_string());
        }
    }
    // 卸载前先让 audiodg 退出：**不是为了"能删槽位值"**（写/删 FxProperties 值只需
    // KEY_SET_VALUE 句柄，音频播放中、DLL 已被 audiodg 加载、audiodg 持有点端的
    // 情况下，槽位值照样删成功）。真正的理由是：
    // ① 释放模块映像——taskkill 后 audiodg 才会卸载 vxapo_driver.dll，
    //    否则紧随其后的重装/换 DLL 会因文件被占用而覆盖失败（NSIS 的
    //    installer-hooks.nsh 同样为此在安装/卸载前停服务）；
    // ② 让端点图重建（pnputil /restart-device）立刻生效——引擎会缓存端点 APO 链，
    //    只改注册表的话新起的流仍加载旧 APO。
    // 停服走 driver 的 SCM 封装（30s 超时，不会挂死；服务未跑时也安全），
    // 随后按事件等待 audiodg 退出。
    if !json {
        if lang() == Lang::En {
            println!("  Stopping audio service + terminating audiodg (uninstall prerequisite)...");
        } else {
            println!("  停止音频服务 + 终止 audiodg（卸载前置）…");
        }
    }
    let _ = vxapo_driver::stop_audio_service();
    // 事件驱动等待：对 audiodg 进程句柄 WaitForSingleObject，进程一退出立即返回
    // （上限 5 s，超时也继续——槽位值的写/删不依赖它，只有"换 DLL 前释放模块映像"依赖）。
    let wait_start = std::time::Instant::now();
    let exited = vxapo_driver::wait_for_audiodg_exit(5000);
    if !json {
        if exited {
            if lang() == Lang::En {
                println!("  audiodg exited ({} ms)", wait_start.elapsed().as_millis());
            } else {
                println!("  audiodg 已退出（{} ms）", wait_start.elapsed().as_millis());
            }
        } else if lang() == Lang::En {
            println!("  ⚠ timed out waiting for audiodg exit (5 s); continuing (slot edit unaffected)");
        } else {
            println!("  ⚠ 等待 audiodg 退出超时（5 s），继续执行（槽位写入不受影响）");
        }
    }

    // 卸载（audiodg 已退出；uninstall_endpoint 内部还会再停一次服务并重启端点）。
    if let Err(e) = uninstall_endpoint(&dev.guid) {
        // 卸载失败也要尝试恢复音频服务。
        let _ = vxapo_driver::ensure_audio_service_running();
        if lang() == Lang::En {
            return Err(format!("uninstall_endpoint failed: {e}"));
        } else {
            return Err(format!("uninstall_endpoint 失败：{e}"));
        }
    }

    // 卸载后回读验证：5 槽位中不应残留 VxAPO CLSID。
    let residual = enumerate_devices()
        .map_err(|e| {
        if lang() == Lang::En {
            format!("Failed to re-enumerate devices: {e}")
        } else {
            format!("回读枚举失败：{e}")
        }
    })?
        .iter()
        .find(|d| d.endpoint.as_ref().map(|e| e.endpoint_guid.eq_ignore_ascii_case(&dev.guid)).unwrap_or(false))
        .map(|d| {
            d.slots.iter().filter(|s| {
                matches!(s, vxapo_driver::SlotValue::Guid(g)
                    if *g == CLSID_VXAPO_PRE_MIX || *g == CLSID_VXAPO_POST_MIX)
            }).count()
        })
        .unwrap_or(0);
    if residual > 0 {
        let _ = vxapo_driver::ensure_audio_service_running();
        if lang() == Lang::En {
            return Err(format!("Detected {residual} slot(s) still containing VxAPO CLSID after uninstall - audio process may still be holding them. Please retry."));
        } else {
            return Err(format!("卸载后检测到 {residual} 个槽位残留 VxAPO CLSID——音频进程可能仍占用，请重试。"));
        }
    }

    // 卸载收尾对齐安装：不整服重启（避免二次打断）——
    // uninstall_endpoint 已定向重启该端点设备并 ensure AudioSrv 运行。

    if json {
        if lang() == Lang::En {
            println!("{{\"ok\":true,\"device\":\"{}\",\"message\":\"uninstalled\"}}", json_escape(&dev.guid));
        } else {
            println!("{{\"ok\":true,\"device\":\"{}\",\"message\":\"已卸载\"}}", json_escape(&dev.guid));
        }
    } else {
        print!("✓ 已卸载 {}。", dev.guid);
        if let Ok(diff) = snapshot_diff(&dev.guid) {
            if lang() == Lang::En {
                println!("  Change summary: {diff}");
            } else {
                println!(" 变更统计：{diff}");
            }
        } else {
            println!();
        }
    }

    Ok(())
}

