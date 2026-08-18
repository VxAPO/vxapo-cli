//! VxAPO CLI 入口
//!
//! 双模式：
//! - **无参数启动**（双击 release exe / 直接运行）→ 进入**交互式终端菜单**（模式选择）：
//!   1. **查看模式**：自研枚举（probe）+ 槽位/格式/增强详细诊断（保留 CLI 自身能力）
//!   2. **Driver 模式**：install 层操作（list 版本/模式/槽位/失守 + install/uninstall/config/snapshot）
//!   模式内 `exit` 可回退到模式选择。
//! - **带子命令参数** → 命令行自动化（脚本/自动化场景）：
//!   vxapo-cli install -d <device> / config set / snapshot diff 等

mod app;
mod commands;
mod display;
mod endpoint;
mod knowledge;
mod probe;
mod reg;
mod regdump;

use std::io::Write;

use app::App;

fn main() {
    // 子命令模式：带参数 → 直接执行命令；无参数 → 交互菜单（release exe 双击进入）
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        std::process::exit(run_subcommand(&args));
    }
    interactive();
}

/// 子命令分派（返回进程退出码：0 成功 / 1 失败）。
fn run_subcommand(args: &[String]) -> i32 {
    // --json 机器可读输出（WinUI 3 集成用）。从参数中剥离，不影响原有解析。
    let json = args.iter().any(|a| a == "--json");
    let args: Vec<String> = args.iter().filter(|a| *a != "--json").cloned().collect();
    let cmd = args[0].as_str();
    let result = match cmd {
        "list" | "status" => commands::list_devices(json),
        "install" => {
            let (dev, mode, no_child) = parse_install(&args[1..]);
            if dev.is_empty() {
                Err("install 用法：vxapo-cli install -d <device> [--mode LfxGfx|SfxMfx|SfxEfx] [--no-child]".to_string())
            } else {
                commands::install(&dev, mode.as_deref(), no_child, json)
            }
        }
        "uninstall" => {
            // 兼容 `-d <device>` 与裸 `<device>`（install 同样支持 -d）。
            let mut dev = String::new();
            let rest = &args[1..];
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "-d" | "--device" => {
                        i += 1;
                        if let Some(d) = rest.get(i) {
                            dev = d.clone();
                        }
                    }
                    other if dev.is_empty() && !other.starts_with('-') => dev = other.to_string(),
                    _ => {}
                }
                i += 1;
            }
            if dev.is_empty() {
                Err("uninstall 用法：vxapo-cli uninstall -d <device>".to_string())
            } else {
                commands::uninstall(&dev, json)
            }
        }
        "config" => {
            if args.len() < 3 {
                Err("config 用法：vxapo-cli config set -d <device> -f <file> 或 config show -d <device>".to_string())
            } else {
                run_config(&args[1..])
            }
        }
        "snapshot" => {
            if args.len() < 3 {
                Err("snapshot 用法：vxapo-cli snapshot diff -d <device> 或 snapshot restore -d <device> 或 snapshot create -d <device>".to_string())
            } else {
                run_snapshot(&args[1..])
            }
        }
        "help" | "--help" | "-h" => {
            print_help();
            return 0;
        }
        _ => Err(format!("未知命令：{cmd}（vxapo-cli help 查看全部）")),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            if json {
                println!("{{\"ok\":false,\"error\":\"{}\"}}", commands::json_escape(&e));
            } else {
                eprintln!("✗ {e}");
            }
            1
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// 交互式终端菜单（无参数启动 / 双击 release exe）
// ══════════════════════════════════════════════════════════════════════════════

/// 模式选择层：1 查看模式（自研枚举+槽位诊断） / 2 Driver 模式（install 层操作）/ q 退出。
fn interactive() {
    println!("VxAPO CLI — 音频处理调试工具");
    println!("================================\n");

    loop {
        println!("--- 选择模式 ---");
        println!("  [1] 查看模式：自研枚举 + 槽位/格式/增强诊断（probe 自身能力）");
        println!("  [2] Driver 模式：安装/卸载/配置/快照（install 层）");
        println!("  [q] 退出");
        print!("> ");
        match read_line().as_str() {
            "1" => viewer_mode(),
            "2" => driver_mode(),
            "q" | "quit" | "exit" => break,
            _ => println!("未知输入（1 / 2 / q）"),
        }
    }
    println!("再见。");
}

/// 查看模式：probe 自身全量枚举（active、去重、格式）+ 设备详情诊断。
/// `exit` 返回模式选择。
fn viewer_mode() {
    let mut app = App::new();
    loop {
        app.refresh();
        println!("\n--- 查看模式：自研枚举（{} 端点）---", app.endpoints.len());
        if app.endpoints.is_empty() {
            println!("（无音频端点）");
        } else {
            display::print_endpoints(&app.endpoints);
        }
        println!("  [序号] 查看详情  [r] 刷新  [q] 回模式选择");
        print!("> ");
        match read_line().as_str() {
            "q" | "quit" | "exit" => break,
            "r" | "refresh" => continue,
            input => {
                if let Ok(idx) = input.parse::<usize>() {
                    if let Some(ep) = app.endpoints.get(idx) {
                        if viewer_detail(ep) {
                            break; // exit → 回模式选择
                        }
                        continue;
                    }
                }
                println!("未知输入：{input}");
            }
        }
    }
}

/// 设备详情页（查看模式）：槽位/格式/增强等全量诊断。返回 true=回模式选择。
fn viewer_detail(ep: &endpoint::Endpoint) -> bool {
    display::print_detail_header(ep);
    println!("  [b] 返回列表  [q] 回模式选择");
    print!("> ");
    match read_line().as_str() {
        "q" | "quit" | "exit" => true,
        _ => false,
    }
}

/// Driver 模式：install 层设备列表（版本/模式/5 槽位/失守）+ 设备操作菜单。
/// `q` 返回模式选择。
fn driver_mode() {
    loop {
        match commands::list_devices(false) {
            Ok(()) => {}
            Err(e) => println!("✗ {e}"),
        }
        println!("\n--- Driver 模式 ---");
        println!("  [序号] 选择设备（install/uninstall/config/snapshot/转储）");
        println!("  [q] 回模式选择");
        print!("> ");
        match read_line().as_str() {
            "q" | "quit" | "exit" => break,
            input => {
                if let Ok(idx) = input.parse::<usize>() {
                    // 返回 true=设备子菜单里 q 了 → 退出 Driver 模式回模式选择。
                    if driver_device_menu(&idx) {
                        break;
                    }
                    continue;
                }
                println!("未知输入：{input}");
            }
        }
    }
}

/// 设备子菜单（Driver 模式）：install/uninstall/config/snapshot/转储。
///
/// 返回：`true` = 用户按 `q` 回模式选择；`false` = 返回设备列表（b）。
/// `b` 返回设备列表；`q` 回模式选择；`h` 呼出帮助。
fn driver_device_menu(dev_idx: &usize) -> bool {
    let dev = match resolve_device_ref(*dev_idx) {
        Ok(d) => d,
        Err(e) => {
            println!("✗ {e}");
            return false;
        }
    };
    // 每次进入重新枚举 probe，供注册表转储按 GUID 匹配端点（driver 序号 ≠ probe 序号）。
    let mut app = App::new();
    app.refresh();
    loop {
        println!("\n--- 设备 {dev_idx}：{} ---", dev.name);
        println!("  [i] 安装       [u] 卸载        [c] 配置管理     [h] 帮助");
        println!("  [p] 快照 diff  [r] 快照恢复    [x] 转储注册表");
        println!("  [s] 状态详情   [b] 返回列表    [q] 回模式选择");
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
                    Ok(()) => println!("✓ 卸载完成"),
                    Err(e) => {
                        println!("✗ {e}");
                        println!("   子命令对照：vxapo-cli uninstall -d {}", dev.guid);
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
                    Ok(()) => println!("✓ 恢复完成"),
                    Err(e) => println!("✗ {e}"),
                }
            }
            "x" => {
                if let Some(ep) = app.endpoints.iter().find(|e| e.guid.eq_ignore_ascii_case(&dev.guid)) {
                    regdump::dump_endpoint(ep);
                } else {
                    println!("✗ 该设备不在 probe 枚举中（GUID 不匹配/非 active）");
                }
            }
            "s" => {
                // 单设备状态：槽位 + childApo 信息区（要求——只显示当前设备）。
                match commands::show_device_status(&dev_idx.to_string()) {
                    Ok(()) => {}
                    Err(e) => println!("✗ {e}"),
                }
            }
            input => {
                // 子命令直通：菜单内可直接执行子命令（交互与子命令结合）。
                // 兼容两种输入：直接打 `install -d 0`，或粘贴 `vxapo-cli install -d 0`（剥前缀）。
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
                    println!("未知输入（i/u/c/p/r/x/s/b/q/h，或直接打子命令如 install -d {dev_idx}）");
                } else {
                    let known = matches!(
                        args[0].as_str(),
                        "install" | "uninstall" | "config" | "snapshot" | "list" | "status" | "help"
                    );
                    if known {
                        let code = run_subcommand(&args);
                        if code != 0 {
                            println!("  子命令执行失败（退出码 {code}）。可用 help 查看用法。");
                        }
                    } else {
                        println!("未知输入：{input}（支持 i/u/c/p/r/x/s/b/q/h，或直接打子命令如 install -d {dev_idx}）");
                    }
                }
            }
        }
    }
}

/// 安装并引导（交互模式）：先展示槽位占用预览，询问是否保留现有 APO 为子 APO，
/// 再确认执行安装。完成后提示导入 config.txt 调音配置。
fn install_and_guide(dev: &commands::DeviceRef, dev_idx: usize) {
    println!("  正在准备安装到 [{dev_idx}] {}…", dev.name);

    // 槽位占用预览（当前谁占着 PreMix/PostMix）。
    match commands::preview_install(&dev_idx.to_string()) {
        Ok(preview) => println!("{preview}"),
        Err(e) => println!("⚠ 预览失败（继续安装）：{e}"),
    }
    println!("  安装将把 VxAPO 写入 PreMix+PostMix 槽位（默认 SfxEfx 模式）。");

    // 询问是否保留现有 APO 为子 APO（默认保留，回车进入确认）。
    print!("  保留现有 APO 为子 APO？（y=保留 / n=不保留 / 回车=保留）> ");
    std::io::stdout().flush().unwrap();
    let keep = read_line();
    let keep_child = keep.trim().to_ascii_lowercase() != "n";

    print!("  开始安装（模式 SfxEfx）？按回车确认 / q 取消 > ");
    std::io::stdout().flush().unwrap();
    let confirm = read_line();
    if confirm.trim().to_ascii_lowercase() == "q" {
        println!("已取消。");
        return;
    }

    let no_child = !keep_child;
    match commands::install(&dev.guid, None, no_child, false) {
        Ok(()) => {
            println!("✓ 安装完成（模式 SfxEfx，子 APO 保留={keep_child}）。");
            println!("  调音：导入 config.txt（默认读 exe 同级 .\\config.txt，可用 config set 指定别的路径）：");
            println!("    config set -d {dev_idx} -f .\\config.txt");
            println!("  查看是否已配置：config show -d {dev_idx}");
        }
        Err(e) => {
            println!("✗ 安装失败：{e}");
            println!(
                "  可在本菜单直接输入带参数重试：install -d {dev_idx} --mode LfxGfx|SfxMfx|SfxEfx [--no-child]"
            );
        }
    }
}

/// 配置子菜单：config show / config set。
fn config_menu(guid: &str) {
    loop {
        println!("\n--- 配置管理 ---");
        println!("  [s] 显示 config.txt      [e] 编辑（写文件）");
        println!("  [b] 返回设备菜单");
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
                print!("输入源文件路径（内容将原样写入 config.txt）> ");
                std::io::stdout().flush().unwrap();
                let file = read_line();
                if file.trim().is_empty() {
                    println!("已取消");
                } else {
                    match commands::config_set(guid, file.trim()) {
                        Ok(()) => println!("✓ 配置已写入"),
                        Err(e) => println!("✗ {e}"),
                    }
                }
            }
            _ => println!("未知输入（s/e/b）"),
        }
    }
}

/// 读一行输入（trim 后返回）。
fn read_line() -> String {
    std::io::stdout().flush().unwrap();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

// ══════════════════════════════════════════════════════════════════════════════
// 子命令辅助：参数解析 / 设备解析（复用 commands::resolve_device）
// ══════════════════════════════════════════════════════════════════════════════

/// 按枚举序号解析设备三元组（交互菜单用，避免每次 resolve 都枚举）。
fn resolve_device_ref(idx: usize) -> Result<commands::DeviceRef, String> {
    commands::resolve_device(&idx.to_string())
}

/// 解析 install 参数：`-d <device> [--mode X] [--no-child]`。
fn parse_install(args: &[String]) -> (String, Option<String>, bool) {
    let mut dev = String::new();
    let mut mode = None;
    let mut no_child = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-d" | "--device" => {
                i += 1;
                if let Some(d) = args.get(i) {
                    dev = d.clone();
                }
            }
            "--mode" => {
                i += 1;
                mode = args.get(i).cloned();
            }
            "--no-child" => no_child = true,
            _ => {}
        }
        i += 1;
    }
    (dev, mode, no_child)
}

/// config 子命令：`config set -d <device> -f <file>` / `config show -d <device>`。
fn run_config(args: &[String]) -> Result<(), String> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("");
    if sub == "convert" {
        let src = args.get(1).map(|s| s.as_str()).unwrap_or("");
        if src.is_empty() {
            return Err(
                "config convert 需要源文件：vxapo-cli config convert <old.txt> [out.toml]"
                    .to_string(),
            );
        }
        return commands::config_convert(src, args.get(2).map(|s| s.as_str()));
    }
    let (guid, file) = parse_device_file(&args[1..]);
    match sub {
        "set" => {
            if file.is_empty() {
                Err("config set 需要 -f <file>".to_string())
            } else {
                commands::config_set(&guid, &file)
            }
        }
        "show" => commands::config_show(&guid),
        _ => Err("config 子命令：set / show / convert".to_string()),
    }
}

/// snapshot 子命令：`snapshot diff -d <device>` / `snapshot restore -d <device>`。
///
/// `<device>` 支持 `{GUID}` 或枚举序号（与 install/uninstall 一致——序号先
/// resolve_device 转 GUID；`{GUID}` 原样传给底层）。
fn run_snapshot(args: &[String]) -> Result<(), String> {
    let sub = args.first().map(|s| s.as_str()).unwrap_or("");
    let (dev_ref, _) = parse_device_file(&args[1..]);
    if dev_ref.is_empty() {
        return Err("snapshot 需要 -d <device>（{GUID} 或 list 序号）".to_string());
    }
    // 序号 → GUID（install/uninstall 同款 resolve；{GUID} 直接通过）。
    let dev = commands::resolve_device(&dev_ref)?;
    let guid = dev.guid;
    match sub {
        "diff" | "changes" => {
            let diff = commands::snapshot_diff(&guid)?;
            println!("{diff}");
            Ok(())
        }
        "restore" => commands::snapshot_restore(&guid),
        "create" | "baseline" => commands::snapshot_device(&guid, true),
        _ => Err("snapshot 子命令：diff / restore / create".to_string()),
    }
}

/// 解析 `-d <device> [-f <file>]`。
fn parse_device_file(args: &[String]) -> (String, String) {
    let mut dev = String::new();
    let mut file = String::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-d" | "--device" => {
                i += 1;
                if let Some(d) = args.get(i) {
                    dev = d.clone();
                }
            }
            "-f" | "--file" => {
                i += 1;
                if let Some(f) = args.get(i) {
                    file = f.clone();
                }
            }
            _ => {}
        }
        i += 1;
    }
    (dev, file)
}

/// 命令帮助（子命令模式 / help）。
fn print_help() {
    println!("VxAPO CLI — 音频处理端到端验证工具");
    println!();
    println!("交互模式：直接运行 vxapo-cli（无参数）→ 终端菜单操作");
    println!();
    println!("子命令模式：vxapo-cli <命令> [参数]");
    println!("  list / status                     列出音频端点 + 槽位占用 + 失守标注");
    println!("  install -d <device> [--mode LfxGfx|SfxMfx|SfxEfx] [--no-child]");
    println!("  uninstall -d <device>");
    println!("  config set -d <device> -f <file>   写 per-device config.txt");
    println!("  config show -d <device>            读回 config.txt");
    println!("  snapshot diff -d <device>          基线 vs 当前注册表变更");
    println!("  snapshot restore -d <device>       恢复基线（清槽位）");
    println!("  snapshot create -d <device>        建立/替换基线");
    println!(" <device> = {{GUID}} 或枚举序号（list 查看）");
}
