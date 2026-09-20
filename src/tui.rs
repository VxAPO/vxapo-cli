//! tui.rs — 交互式菜单（语言选择 / 端点浏览 / 驱动面板 / 安装引导）

use super::*;

/// 选择界面语言，然后进入模式选择。
pub(super) fn choose_language() {
    println!("VxAPO CLI");
    println!("================================\n");
    println!("Select language / 选择语言:");
    println!("  [1] English");
    println!("  [2] 中文");
    print!("> ");
    match read_line().as_str() {
        "1" => set_lang(Lang::En),
        _ => set_lang(Lang::Zh),
    }
    println!();
}

/// 模式选择层：1 查看模式 / 2 Driver 模式 / q 退出。
pub(super) fn interactive() {
    choose_language();

    println!("{}", tr!("VxAPO CLI — 音频处理调试工具", "VxAPO CLI - audio processing debug tool"));
    println!("================================\n");

    loop {
        println!("{}", tr!("--- 选择模式 ---", "--- Select mode ---"));
        println!("  [1] {}", tr!("查看模式：自研枚举 + 槽位/格式/增强诊断（probe 自身能力）", "Viewer mode: built-in enumeration + slot/format/enhancement diagnostics"));
        println!("  [2] {}", tr!("Driver 模式：安装/卸载/配置/快照（install 层）", "Driver mode: install/uninstall/config/snapshot"));
        println!("  [q] {}", tr!("退出", "Quit"));
        print!("> ");
        match read_line().as_str() {
            "1" => viewer_mode(),
            "2" => driver_mode(),
            "q" | "quit" | "exit" => break,
            _ => println!("{}", tr!("未知输入（1 / 2 / q）", "Unknown input (1 / 2 / q)")),
        }
    }
    println!("{}", tr!("再见。", "Goodbye."));
}

/// 查看模式：probe 自身全量枚举（active、去重、格式）+ 设备详情诊断。
/// `exit` 返回模式选择。
pub(super) fn viewer_mode() {
    let mut app = App::new();
    loop {
        app.refresh();
        let title = if lang() == Lang::En {
            format!("--- Viewer mode: built-in enumeration ({} endpoints) ---", app.endpoints.len())
        } else {
            format!("--- 查看模式：自研枚举（{} 端点）---", app.endpoints.len())
        };
        println!("\n{title}");
        if app.endpoints.is_empty() {
            println!("{}", tr!("（无音频端点）", "(no audio endpoints)"));
        } else {
            display::print_endpoints(&app.endpoints);
        }
        println!("  [{}] {}  [r] {}  [q] {}", tr!("序号", "index"), tr!("查看详情", "view details"), tr!("刷新", "refresh"), tr!("回模式选择", "back to mode selection"));
        print!("> ");
        match read_line().as_str() {
            "q" | "quit" | "exit" => break,
            "r" | "refresh" => continue,
            input => {
                if let Ok(idx) = input.parse::<usize>() {
                    if let Some(ep) = app.endpoints.get(idx) {
                        if viewer_detail(ep) {
                            break;
                        }
                        continue;
                    }
                }
                println!("{}", tr!("未知输入：", "Unknown input: "));
            }
        }
    }
}

/// 设备详情页（查看模式）。返回 true=回模式选择。
pub(super) fn viewer_detail(ep: &endpoint::Endpoint) -> bool {
    display::print_detail_header(ep);
    println!("  [b] {}  [q] {}", tr!("返回列表", "back to list"), tr!("回模式选择", "back to mode selection"));
    print!("> ");
    match read_line().as_str() {
        "q" | "quit" | "exit" => true,
        _ => false,
    }
}

/// Driver 模式：install 层设备列表 + 设备操作菜单。
pub(super) fn driver_mode() {
    loop {
        match commands::list_devices(false) {
            Ok(()) => {}
            Err(e) => println!("✗ {e}"),
        }
        println!("\n{}", tr!("--- Driver 模式 ---", "--- Driver mode ---"));
        println!("  [{}] {}（install/uninstall/config/snapshot/转储）", tr!("序号", "index"), tr!("选择设备", "select device"));
        println!("  [q] {}", tr!("回模式选择", "back to mode selection"));
        print!("> ");
        match read_line().as_str() {
            "q" | "quit" | "exit" => break,
            input => {
                if let Ok(idx) = input.parse::<usize>() {
                    if driver_device_menu(&idx) {
                        break;
                    }
                    continue;
                }
                println!("{}", tr!("未知输入：", "Unknown input: "));
            }
        }
    }
}

/// 设备子菜单（Driver 模式）：install/uninstall/config/snapshot/转储。
///
/// 返回：`true` = 用户按 `q` 回模式选择；`false` = 返回设备列表（b）。
pub(super) fn driver_device_menu(dev_idx: &usize) -> bool {
    let dev = match resolve_device_ref(*dev_idx) {
        Ok(d) => d,
        Err(e) => {
            println!("✗ {e}");
            return false;
        }
    };
    // 每次进入重新枚举 probe，供注册表转储按 GUID 匹配端点。
    let mut app = App::new();
    app.refresh();
    loop {
        println!("\n--- {} {dev_idx}：{} ---", tr!("设备", "Device"), dev.name);
        println!("  [i] {}  [u] {}  [c] {}  [h] {}", tr!("安装", "Install"), tr!("卸载", "Uninstall"), tr!("配置管理", "Config"), tr!("帮助", "Help"));
        println!("  [p] {}  [r] {}  [x] {}", tr!("快照 diff", "Snapshot diff"), tr!("快照恢复", "Snapshot restore"), tr!("转储注册表", "Dump registry"));
        println!("  [s] {}  [b] {}  [q] {}", tr!("状态详情", "Status"), tr!("返回列表", "Back to list"), tr!("回模式选择", "Back to mode selection"));
        print!("> ");
        match read_line().as_str() {
            "b" | "" => return false,
            "q" | "quit" | "exit" => return true,
            "h" | "help" | "--help" => {
                print_help();
                println!();
            }
            "i" => install_and_guide(&dev, *dev_idx),
            "u" => {
                match commands::uninstall(&dev.guid, false) {
                    Ok(()) => println!("{}", tr!("✓ 卸载完成", "✓ Uninstall complete")),
                    Err(e) => {
                        println!("✗ {e}");
                        println!("   {}: vxapo-cli uninstall -d {}", tr!("子命令对照", "Subcommand"), dev.guid);
                    }
                }
            }
            "c" => config_menu(&dev.guid),
            "p" => {
                match commands::snapshot_diff(&dev.guid) {
                    Ok(diff) => println!("{diff}"),
                    Err(e) => println!("✗ {e}"),
                }
            }
            "r" => {
                match commands::snapshot_restore(&dev.guid) {
                    Ok(()) => println!("{}", tr!("✓ 恢复完成", "✓ Restore complete")),
                    Err(e) => println!("✗ {e}"),
                }
            }
            "x" => {
                if let Some(ep) = app.endpoints.iter().find(|e| e.guid.eq_ignore_ascii_case(&dev.guid)) {
                    regdump::dump_endpoint(ep);
                } else {
                    println!("{}", tr!("✗ 该设备不在 probe 枚举中（GUID 不匹配/非 active）", "✗ Device not found in probe enumeration (GUID mismatch or not active)"));
                }
            }
            "s" => {
                match commands::show_device_status(&dev_idx.to_string()) {
                    Ok(()) => {}
                    Err(e) => println!("✗ {e}"),
                }
            }
            input => {
                // 子命令直通：菜单内可直接执行子命令。
                let line = input.trim();
                let line = line
                    .strip_prefix("vxapo-cli")
                    .map(str::trim)
                    .unwrap_or(line);
                let args: Vec<String> = line
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
                if args.is_empty() {
                    let msg = if lang() == Lang::En {
                        format!("Unknown input (i/u/c/p/r/x/s/b/q/h, or type a subcommand like install -d {dev_idx})")
                    } else {
                        format!("未知输入（i/u/c/p/r/x/s/b/q/h，或直接打子命令如 install -d {dev_idx}）")
                    };
                    println!("{msg}");
                } else {
                    let known = matches!(
                        args[0].as_str(),
                        "install" | "uninstall" | "config" | "snapshot" | "list" | "status" | "help"
                    );
                    if known {
                        let code = run_subcommand(&args);
                        if code != 0 {
                            println!("  {} {code}。{}", tr!("子命令执行失败（退出码", "Subcommand failed (exit code"), tr!("可用 help 查看用法。", "Use help for usage."));
                        }
                    } else {
                        let msg = if lang() == Lang::En {
                            format!("Unknown input: {input} (Supported: i/u/c/p/r/x/s/b/q/h, or type a subcommand like install -d {dev_idx})")
                        } else {
                            format!("未知输入：{input}（支持 i/u/c/p/r/x/s/b/q/h，或直接打子命令如 install -d {dev_idx}）")
                        };
                        println!("{msg}");
                    }
                }
            }
        }
    }
}

/// 安装并引导（交互模式）。
pub(super) fn install_and_guide(dev: &commands::DeviceRef, dev_idx: usize) {
    println!("  {} [{dev_idx}] {}…", tr!("正在准备安装到", "Preparing to install to"), dev.name);

    match commands::preview_install(&dev_idx.to_string()) {
        Ok(preview) => println!("{preview}"),
        Err(e) => println!("⚠ {}：{e}", tr!("预览失败（继续安装）", "Preview failed (continuing)")),
    }
    println!("  {}", tr!("安装将把 VxAPO 写入 PreMix+PostMix 槽位（默认 SfxEfx 模式）。", "Install will write VxAPO to PreMix+PostMix slots (default SfxEfx mode)."));

    print!("  {}（y={} / n={} / {}=）> ", tr!("保留现有 APO 为子 APO？", "Keep existing APO as child APO?"), tr!("保留", "keep"), tr!("不保留", "discard"), tr!("回车=保留", "Enter=keep"));
    std::io::stdout().flush().unwrap();
    let keep = read_line();
    let keep_child = keep.trim().to_ascii_lowercase() != "n";

    print!("  {}？{} / q {} > ", tr!("开始安装（模式 SfxEfx）", "Start install (SfxEfx)"), tr!("按回车确认", "Press Enter to confirm"), tr!("取消", "cancel"));
    std::io::stdout().flush().unwrap();
    let confirm = read_line();
    if confirm.trim().to_ascii_lowercase() == "q" {
        println!("{}", tr!("已取消。", "Cancelled."));
        return;
    }

    let no_child = !keep_child;
    match commands::install(&dev.guid, None, no_child, false, false, 180, None) {
        Ok(()) => {
            println!("✓ {}（SfxEfx，{}={keep_child}）。", tr!("安装完成", "Install complete"), tr!("子 APO 保留", "child APO keep"));
            println!("  {}：", tr!("调音", "Tuning"));
            println!("    config set -d {dev_idx} -f .\\config.toml");
            println!("    config show -d {dev_idx}");
        }
        Err(e) => {
            println!("✗ {}：{e}", tr!("安装失败", "Install failed"));
            println!(
                "  {}：install -d {dev_idx} --mode LfxGfx|SfxMfx|SfxEfx [--no-child]",
                tr!("可在本菜单直接输入带参数重试", "Retry in this menu with parameters")
            );
        }
    }
}

/// 配置子菜单：config show / config set。
pub(super) fn config_menu(guid: &str) {
    loop {
        println!("\n--- {} ---", tr!("配置管理", "Config management"));
        println!("  [s] {}      [e] {}", tr!("显示 config.toml", "Show config.toml"), tr!("编辑（写文件）", "Edit (write file)"));
        println!("  [b] {}", tr!("返回设备菜单", "Back to device menu"));
        print!("> ");
        match read_line().as_str() {
            "b" | "" => break,
            "s" => {
                match commands::config_show(guid) {
                    Ok(()) => {}
                    Err(e) => println!("✗ {e}"),
                }
            }
            "e" => {
                print!("{}> ", tr!("输入源文件路径（内容将原样写入 config.toml）", "Source file path (content will be written to config.toml)"));
                std::io::stdout().flush().unwrap();
                let file = read_line();
                if file.trim().is_empty() {
                    println!("{}", tr!("已取消", "Cancelled"));
                } else {
                    match commands::config_set(guid, file.trim()) {
                        Ok(()) => println!("{}", tr!("✓ 配置已写入", "✓ Config written")),
                        Err(e) => println!("✗ {e}"),
                    }
                }
            }
            _ => println!("{}（s/e/b）", tr!("未知输入", "Unknown input")),
        }
    }
}

/// 读一行输入（trim 后返回）。
pub(super) fn read_line() -> String {
    std::io::stdout().flush().unwrap();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

// ══════════════════════════════════════════════════════════════════════════════
// 子命令辅助：参数解析 / 设备解析（复用 commands::resolve_device）
// ══════════════════════════════════════════════════════════════════════════════

/// 按枚举序号解析设备三元组（交互菜单用，避免每次 resolve 都枚举）。
pub(super) fn resolve_device_ref(idx: usize) -> Result<commands::DeviceRef, String> {
    commands::resolve_device(&idx.to_string())
}

