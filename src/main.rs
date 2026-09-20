//! VxAPO CLI 入口
//!
//! 双模式：
//! - **无参数启动**（双击 release exe / 直接运行）→ 进入**交互式终端菜单**（模式选择）：
//!   1. **查看模式**：自研枚举（probe）+ 槽位/格式/增强详细诊断（保留 CLI 自身能力）
//!   2. **Driver 模式**：install 层操作（list 版本/模式/槽位/失守 + install/uninstall/config/snapshot）
//!   模式内 `exit` 可回退到模式选择。
//! - **带子命令参数** → 命令行自动化（脚本/自动化场景）：
//!   vxapo-cli install -d <device> / config set / snapshot diff 等

/// 双语文本：`tr!(中文, English)`（语言由 `i18n::lang()` 决定）。
///
/// 必须定义在模块声明之前：`macro_rules!` 为文本作用域，`args` / `tui` 子模块要用。
macro_rules! tr {
    ($zh:expr, $en:expr) => { i18n::tr($zh, $en) };
}

mod args;
mod commands;
mod display;
mod endpoint;
mod knowledge;
mod probe;
mod reg;
mod i18n;
mod regdump;
mod tui;
mod verify;

use std::io::Write;
use std::path::Path;

use i18n::{Lang, lang, set_lang};

use probe::App;

fn main() {
    // 子命令模式：带参数 → 直接执行命令；无参数 → 交互菜单（release exe 双击进入）
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        std::process::exit(run_subcommand(&args));
    }
    tui::interactive();
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
            let (dev, mode, no_child, verify, timeout, progress_file) =
                args::parse_install(&args[1..]);
            if dev.is_empty() {
                Err("install 用法：vxapo-cli install -d <device> [--mode LfxGfx|SfxMfx|SfxEfx] [--no-child] [--verify] [--timeout=<sec>] [--progress-file=<path>]".to_string())
            } else {
                let progress = progress_file.as_deref().map(Path::new);
                commands::install(&dev, mode.as_deref(), no_child, json, verify, timeout, progress)
            }
        }
        "uninstall" => {
            // 兼容 `-d <device>` 与裸 `<device>`（与 install 一致）。
            let p = args::ArgParser::new(&args[1..], &[("-d", &["--device"][..])], &[]);
            let dev = p
                .get("-d")
                .or_else(|| p.positional())
                .unwrap_or_default()
                .to_string();
            if dev.is_empty() {
                Err("uninstall 用法：vxapo-cli uninstall -d <device>".to_string())
            } else {
                commands::uninstall(&dev, json)
            }
        }
        "register" => commands::register(),
        "stale" => args::run_stale(&args[1..], json),
        "effects" => args::run_effects(&args[1..], json),
        "config" => {
            if args.len() < 3 {
                Err("config 用法：vxapo-cli config set -d <device> -f <file> 或 config show -d <device>".to_string())
            } else {
                args::run_config(&args[1..])
            }
        }
        "snapshot" => {
            if args.len() < 3 {
                Err("snapshot 用法：vxapo-cli snapshot diff -d <device> 或 snapshot restore -d <device> 或 snapshot create -d <device>".to_string())
            } else {
                args::run_snapshot(&args[1..])
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
                println!("{}", vxapo_protocol::CliError::new(&e).to_json());
            } else {
                eprintln!("✗ {e}");
            }
            1
        }
    }
}

/// 命令帮助（子命令模式 / help）。
fn print_help() {
    if lang() == Lang::En {
        println!("VxAPO CLI - audio processing end-to-end verification tool");
        println!();
        println!("Interactive mode: run vxapo-cli without arguments");
        println!();
        println!("Subcommand mode: vxapo-cli <command> [options]");
        println!("  list / status                     List audio endpoints + slot usage + lost-slot markers");
        println!("  install -d <device> [--mode LfxGfx|SfxMfx|SfxEfx] [--no-child] [--verify] [--timeout=<sec>]");
        println!("  uninstall -d <device>");
        println!("  config set -d <device> -f <file>   Write per-device config.toml");
        println!("  config show -d <device>            Read back config.toml");
        println!("  snapshot diff -d <device>          Show registry changes vs baseline");
        println!("  snapshot restore -d <device>       Restore baseline");
        println!("  snapshot create -d <device>        Create/replace baseline");
        println!(" <device> = {{GUID}} or enumeration index (see list)");
    } else {
        println!("VxAPO CLI — 音频处理端到端验证工具");
        println!();
        println!("交互模式：直接运行 vxapo-cli（无参数）→ 终端菜单操作");
        println!();
        println!("子命令模式：vxapo-cli <命令> [参数]");
        println!("  list / status                     列出音频端点 + 槽位占用 + 失守标注");
        println!("  install -d <device> [--mode LfxGfx|SfxMfx|SfxEfx] [--no-child] [--verify] [--timeout=<sec>]");
        println!("  uninstall -d <device>");
        println!("  config set -d <device> -f <file>   写 per-device config.toml");
        println!("  config show -d <device>            读回 config.toml");
        println!("  snapshot diff -d <device>          基线 vs 当前注册表变更");
        println!("  snapshot restore -d <device>       恢复基线（清槽位）");
        println!("  snapshot create -d <device>        建立/替换基线");
        println!(" <device> = {{GUID}} 或枚举序号（list 查看）");
    }
}
