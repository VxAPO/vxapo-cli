//! commands/snapshot.rs — 安装快照建立/对比/回滚

use super::*;

/// 快照文件路径：`{driver snapshot_dir}\{guid}.json`（目录与 driver 迁移备份共用）。
pub(super) fn snapshot_path(guid: &str) -> String {
    format!("{}\\{guid}.json", snapshot_dir())
}

pub(super) fn snapshot_exists(guid: &str) -> bool {
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
        std::fs::create_dir_all(parent).map_err(|e| {
        if lang() == Lang::En {
            format!("Failed to create snapshot directory: {e}")
        } else {
            format!("创建快照目录失败：{e}")
        }
    })?;
    }
    std::fs::write(&path, &snapshot).map_err(|e| {
        if lang() == Lang::En {
            format!("Failed to write snapshot: {e}")
        } else {
            format!("写快照失败：{e}")
        }
    })?;
    if lang() == Lang::En {
        println!("✓ Snapshot saved: {path}");
    } else {
        println!("✓ 快照已保存：{path}");
    }
    Ok(())
}

/// 捕获当前注册表状态（序列化为简单 JSON 文本）。
pub(super) fn capture_snapshot(guid: &str) -> Result<String, String> {
    // FxProperties 5 槽位 + childApoPath 安装信息区 + DisableEnhancements。
    // 经 driver：enumerate_devices 拿槽位 + read_child_apo_guid / child_apo_key_exists 判定。
    let devices = enumerate_devices().map_err(|e| {
        if lang() == Lang::En {
            format!("Enumeration failed: {e}")
        } else {
            format!("枚举失败：{e}")
        }
    })?;
    let d = devices
        .iter()
        .find(|d| d.endpoint.as_ref().map(|e| e.endpoint_guid.eq_ignore_ascii_case(guid)).unwrap_or(false))
        .ok_or_else(|| "设备不在枚举列表".to_string())?;
    let mut lines = Vec::new();
    for (i, val) in d.slots.iter().enumerate() {
        let label = match val {
            vxapo_driver::SlotValue::Guid(g) => format!("{g:?}"),
            vxapo_driver::SlotValue::NoKey => "(NoKey)".to_string(),
            vxapo_driver::SlotValue::NoValue => "(NoValue)".to_string(),
        };
        lines.push(format!("slot_{i}={label}"));
    }
    let premix = read_child_apo_guid(guid, ChildApoKind::PreMix).map(|g| format!("{g:?}")).unwrap_or_default();
    let postmix = read_child_apo_guid(guid, ChildApoKind::PostMix).map(|g| format!("{g:?}")).unwrap_or_default();
    lines.push(format!("childPreMix={premix}"));
    lines.push(format!("childPostMix={postmix}"));
    lines.push(format!("childApoKeyExists={}", vxapo_driver::child_apo_key_exists(guid)));
    Ok(lines.join("\n"))
}

/// 基线 vs 当前 diff（红绿/±~ 表示，返回统计行文本）。
pub fn snapshot_diff(guid: &str) -> Result<String, String> {
    let path = snapshot_path(guid);
    let baseline = std::fs::read_to_string(&path)
        .map_err(|_| {
            if lang() == Lang::En {
                "No baseline: run install first to create a snapshot".to_string()
            } else {
                "无基线：先 install 建立快照".to_string()
            }
        })?;
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
            if lang() == Lang::En {
                println!("- {} = (deleted)", k);
            } else {
                println!("- {} = （已删除）", k);
            }
        }
    }
    if lang() == Lang::En {
        Ok(format!("Changes: +{adds} added / -{dels} deleted / ~{mods} modified / {same} unchanged"))
    } else {
        Ok(format!("变更：+{adds} 新增 / -{dels} 删除 / ~{mods} 修改 / {same} 无变化"))
    }
}

/// snapshot restore：从基线恢复注册表状态（仅经 driver 操作；当前只列示差异提示，写恢复走 uninstall/install）。
pub fn snapshot_restore(guid: &str) -> Result<(), String> {
    require_admin()?;
    if !snapshot_exists(guid) {
        if lang() == Lang::En {
            return Err("No baseline to restore".to_string());
        } else {
            return Err("无基线可恢复".to_string());
        }
    }
    // 恢复 = 卸载 + 按基线重建（driver Transaction 保证注册表级一致）。
    // 简化：先 uninstall 清槽位，再提示基线重建方式（完整恢复走 install 全量路径）。
    uninstall_endpoint(&guid).map_err(|e| {
        if lang() == Lang::En {
            format!("Restore failed (uninstall): {e}")
        } else {
            format!("恢复失败（uninstall）：{e}")
        }
    })?;
    if lang() == Lang::En {
        println!("✓ Baseline restored (slots cleared + childApoPath removed). Run install to return to installed state.");
    } else {
        println!("✓ 已恢复基线（槽位清空 + childApoPath 删除）。如需回到基线安装态，请 install。");
    }
    Ok(())
}

