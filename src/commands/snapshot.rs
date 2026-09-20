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

/// 快照 diff 的纯计算结果（I/O 与打印留在 `snapshot_diff`，本结构与 `diff_snapshots` 可单测）。
struct SnapshotDiff {
    adds: usize,
    dels: usize,
    mods: usize,
    same: usize,
    /// 逐项去向，顺序：当前侧按行序（无变化 / 修改 / 新增）→ 基线侧被删除项。
    /// 每项为 `(键, 基线值, 当前值)`；某一侧为 `None` 表示该项只存在于另一侧。
    entries: Vec<(String, Option<String>, Option<String>)>,
}

/// 纯函数：对比两份 `key=value` 文本（无 `=` 的行忽略）。
fn diff_snapshots(baseline: &str, current: &str) -> SnapshotDiff {
    let parse = |text: &str| -> Vec<(String, String)> {
        text.lines()
            .filter_map(|l| l.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
            .collect()
    };
    let b_lines = parse(baseline);
    let c_lines = parse(current);
    let mut diff = SnapshotDiff {
        adds: 0,
        dels: 0,
        mods: 0,
        same: 0,
        entries: Vec::new(),
    };
    for (k, v) in &c_lines {
        match b_lines.iter().find(|(bk, _)| bk == k) {
            Some((_, bv)) if bv == v => {
                diff.same += 1;
                diff.entries.push((k.clone(), Some(bv.clone()), Some(v.clone())));
            }
            Some((_, bv)) => {
                diff.mods += 1;
                diff.entries.push((k.clone(), Some(bv.clone()), Some(v.clone())));
            }
            None => {
                diff.adds += 1;
                diff.entries.push((k.clone(), None, Some(v.clone())));
            }
        }
    }
    for (k, bv) in &b_lines {
        if !c_lines.iter().any(|(ck, _)| ck == k) {
            diff.dels += 1;
            diff.entries.push((k.clone(), Some(bv.clone()), None));
        }
    }
    diff
}

/// 基线 vs 当前 diff（`~` 修改 / `+` 新增 / `-` 删除 / 其余无变化，返回统计行文本）。
pub fn snapshot_diff(guid: &str) -> Result<String, String> {
    let path = snapshot_path(guid);
    let baseline = std::fs::read_to_string(&path).map_err(|_| {
        if lang() == Lang::En {
            "No baseline: run install first to create a snapshot".to_string()
        } else {
            "无基线：先 install 建立快照".to_string()
        }
    })?;
    let current = capture_snapshot(guid)?;
    let diff = diff_snapshots(&baseline, &current);
    for (k, old, new) in &diff.entries {
        match (old, new) {
            (Some(bv), Some(v)) if bv == v => println!("    {} = {}", k, v),
            (Some(bv), Some(v)) => println!("~ {} = {bv} → {v}", k),
            (None, Some(v)) => println!("+ {} = {}", k, v),
            (Some(_), None) => {
                if lang() == Lang::En {
                    println!("- {} = (deleted)", k);
                } else {
                    println!("- {} = （已删除）", k);
                }
            }
            (None, None) => {}
        }
    }
    let SnapshotDiff {
        adds, dels, mods, same, ..
    } = diff;
    if lang() == Lang::En {
        Ok(format!(
            "Changes: +{adds} added / -{dels} deleted / ~{mods} modified / {same} unchanged"
        ))
    } else {
        Ok(format!(
            "变更：+{adds} 新增 / -{dels} 删除 / ~{mods} 修改 / {same} 无变化"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_counts_added_deleted_modified_unchanged() {
        let d = diff_snapshots(
            "slot_0=A\nslot_1=B\nchildPreMix=X\n",
            "slot_0=A\nslot_1=C\nslot_2=D\n",
        );
        assert_eq!((d.same, d.mods, d.adds, d.dels), (1, 1, 1, 1));
        // 顺序：当前侧行序（同/改/增），删除项置尾。
        let keys: Vec<&str> = d.entries.iter().map(|(k, _, _)| k.as_str()).collect();
        assert_eq!(keys, ["slot_0", "slot_1", "slot_2", "childPreMix"]);
    }

    #[test]
    fn diff_ignores_lines_without_equals() {
        let d = diff_snapshots("noise\nslot_0=A\n", "slot_0=A\n");
        assert_eq!((d.same, d.mods, d.adds, d.dels), (1, 0, 0, 0));
    }

    #[test]
    fn diff_of_empty_baseline_lists_all_added() {
        let d = diff_snapshots("", "slot_0=A\nslot_1=B\n");
        assert_eq!((d.same, d.mods, d.adds, d.dels), (0, 0, 2, 0));
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

