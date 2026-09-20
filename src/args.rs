//! args.rs — 子命令参数解析（install / stale / config / snapshot）

use super::*;

/// 极简选项解析器：把 `-x` / `--long <value>` / `--long=<value>` / 布尔开关统一成查表。
///
/// 不引入 clap：交互式 TUI 与本层手写解析耦合浅，这里只需消除四处重复的 while 循环。
/// 未知选项按既有行为忽略（保持宽容），非 `-` 开头者按顺序收进位置参数。
pub(super) struct ArgParser {
    /// `(主名, 值)`；布尔开关的值为空串。同名多次出现时保留最后一次。
    opts: Vec<(String, String)>,
    positional: Vec<String>,
}

impl ArgParser {
    /// `valued`：需要吞掉下一个参数的选项，`(主名, 别名)`；`flags`：布尔开关。
    pub(super) fn new(
        argv: &[String],
        valued: &[(&str, &[&str])],
        flags: &[(&str, &[&str])],
    ) -> Self {
        let mut opts = Vec::new();
        let mut positional = Vec::new();
        let mut i = 0;
        while i < argv.len() {
            let arg = argv[i].as_str();
            // `--name=value` 拆成 `--name` + `value`。
            let (head, inline) = match arg.split_once('=') {
                Some((h, v)) => (h, Some(v.to_string())),
                None => (arg, None),
            };
            let valued_hit = valued
                .iter()
                .find(|(name, aliases)| head == *name || aliases.contains(&head))
                .map(|(name, _)| *name);
            let flag_hit = flags
                .iter()
                .find(|(name, aliases)| head == *name || aliases.contains(&head))
                .map(|(name, _)| *name);
            match (valued_hit, flag_hit) {
                (Some(name), _) => {
                    let value = match inline {
                        Some(v) => v,
                        // 缺值（选项是最后一个参数）→ 空串，不 panic、也不吞掉别的选项。
                        None => match argv.get(i + 1) {
                            Some(v) => {
                                i += 1;
                                v.clone()
                            }
                            None => String::new(),
                        },
                    };
                    opts.push((name.to_string(), value));
                }
                (None, Some(name)) => opts.push((name.to_string(), String::new())),
                (None, None) => {
                    if !arg.starts_with('-') {
                        positional.push(arg.to_string());
                    }
                }
            }
            i += 1;
        }
        ArgParser { opts, positional }
    }

    /// 取选项值（未出现或值为空 → `None`）。
    pub(super) fn get(&self, name: &str) -> Option<&str> {
        self.opts
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .filter(|v| !v.is_empty())
    }

    /// 取选项值（缺省空串）。
    pub(super) fn value(&self, name: &str) -> String {
        self.get(name).unwrap_or_default().to_string()
    }

    /// 布尔开关是否出现。
    pub(super) fn flag(&self, name: &str) -> bool {
        self.opts.iter().any(|(k, _)| k == name)
    }

    /// 首个位置参数（裸设备引用等）。
    pub(super) fn positional(&self) -> Option<&str> {
        self.positional.first().map(String::as_str)
    }
}

/// `stale list` / `stale migrate` / `stale cleanup` 参数解析。
pub(super) fn run_stale(args: &[String], json: bool) -> Result<(), String> {
    let sub = args.first().map(String::as_str).unwrap_or("list");
    match sub {
        "list" => commands::stale_list(json),
        "migrate" => {
            let p = ArgParser::new(
                &args[1..],
                &[
                    ("--from", &["-f"][..]),
                    ("--to", &["-t"][..]),
                    ("--config-from", &[][..]),
                    ("--snapshot-from", &[][..]),
                ],
                &[],
            );
            let (from, to) = (p.value("--from"), p.value("--to"));
            if from.is_empty() || to.is_empty() {
                return Err(
                    "stale migrate 用法：vxapo-cli stale migrate --from <oldGuid> --to <newGuid> [--config-from <guid>] [--snapshot-from <guid>]"
                        .to_string(),
                );
            }
            commands::stale_migrate(
                &from,
                &to,
                p.get("--config-from"),
                p.get("--snapshot-from"),
                json,
            )
        }
        // cleanup / fix-acl 参数形状相同（`-d <guid>` 或裸 `<guid>`），合并解析。
        sub @ ("cleanup" | "fix-acl") => {
            let p = ArgParser::new(&args[1..], &[("-d", &["--device"][..])], &[]);
            let guid = p
                .get("-d")
                .or_else(|| p.positional())
                .unwrap_or_default()
                .to_string();
            if guid.is_empty() {
                return Err(format!("stale {sub} 用法：vxapo-cli stale {sub} -d <guid>"));
            }
            if sub == "cleanup" {
                commands::stale_cleanup(&guid, json)
            } else {
                commands::stale_fix_acl(&guid, json)
            }
        }
        _ => Err(format!(
            "未知 stale 子命令：{sub}（可用 list / migrate / cleanup / fix-acl）"
        )),
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// 交互式终端菜单（无参数启动 / 双击 release exe）
// ══════════════════════════════════════════════════════════════════════════════

/// 解析 install 参数：`-d <device> [--mode X] [--no-child] [--verify] [--timeout=<sec>]`。
pub(super) fn parse_install(
    args: &[String],
) -> (String, Option<String>, bool, bool, u64, Option<String>) {
    let p = ArgParser::new(
        args,
        &[
            ("-d", &["--device"][..]),
            ("--mode", &[][..]),
            ("--timeout", &[][..]),
            ("--progress-file", &[][..]),
        ],
        &[("--no-child", &[][..]), ("--verify", &[][..])],
    );
    (
        p.value("-d"),
        p.get("--mode").map(str::to_string),
        p.flag("--no-child"),
        p.flag("--verify"),
        p.get("--timeout")
            .and_then(|v| v.parse().ok())
            .unwrap_or(180),
        p.get("--progress-file").map(str::to_string),
    )
}

/// config 子命令：`config set -d <device> -f <file>` / `config show -d <device>`。
pub(super) fn run_config(args: &[String]) -> Result<(), String> {
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
pub(super) fn run_snapshot(args: &[String]) -> Result<(), String> {
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
pub(super) fn parse_device_file(args: &[String]) -> (String, String) {
    let p = ArgParser::new(
        args,
        &[("-d", &["--device"][..]), ("-f", &["--file"][..])],
        &[],
    );
    (p.value("-d"), p.value("-f"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn valued_option_accepts_short_long_and_inline_forms() {
        for input in [
            vec!["-d", "DEV"],
            vec!["--device", "DEV"],
            vec!["--device=DEV"],
        ] {
            let p = ArgParser::new(&argv(&input), &[("-d", &["--device"][..])], &[]);
            assert_eq!(p.get("-d"), Some("DEV"), "{input:?}");
        }
    }

    #[test]
    fn last_occurrence_wins() {
        let p = ArgParser::new(&argv(&["-d", "A", "-d", "B"]), &[("-d", &[][..])], &[]);
        assert_eq!(p.get("-d"), Some("B"));
    }

    #[test]
    fn flags_do_not_consume_values() {
        let p = ArgParser::new(
            &argv(&["-d", "DEV", "--verify", "--no-child"]),
            &[("-d", &[][..])],
            &[("--verify", &[][..]), ("--no-child", &[][..])],
        );
        assert_eq!(p.get("-d"), Some("DEV"));
        assert!(p.flag("--verify") && p.flag("--no-child"));
        assert!(!p.flag("--mode"));
    }

    #[test]
    fn positional_keeps_first_bare_argument_and_unknown_options_ignored() {
        let p = ArgParser::new(&argv(&["GUID", "--bogus", "extra"]), &[], &[]);
        assert_eq!(p.positional(), Some("GUID"));
    }

    #[test]
    fn missing_value_yields_none_not_panic() {
        let p = ArgParser::new(&argv(&["-d"]), &[("-d", &[][..])], &[]);
        assert_eq!(p.get("-d"), None);
    }

    #[test]
    fn install_parse_smoke() {
        let (dev, mode, no_child, verify, timeout, progress) = parse_install(&argv(&[
            "-d",
            "DEV",
            "--mode",
            "SfxMfx",
            "--no-child",
            "--verify",
            "--timeout=300",
            "--progress-file",
            r"C:\tmp\p.json",
        ]));
        assert_eq!(dev, "DEV");
        assert_eq!(mode.as_deref(), Some("SfxMfx"));
        assert!(no_child && verify);
        assert_eq!(timeout, 300);
        assert_eq!(progress.as_deref(), Some(r"C:\tmp\p.json"));
    }

    #[test]
    fn install_timeout_defaults_and_invalid_value_falls_back() {
        let (_, _, _, _, timeout, _) = parse_install(&argv(&["-d", "DEV"]));
        assert_eq!(timeout, 180);
        let (_, _, _, _, timeout, _) = parse_install(&argv(&["-d", "DEV", "--timeout", "abc"]));
        assert_eq!(timeout, 180);
    }

    #[test]
    fn device_file_parse_smoke() {
        let (dev, file) = parse_device_file(&argv(&["-d", "DEV", "-f", "cfg.toml"]));
        assert_eq!((dev.as_str(), file.as_str()), ("DEV", "cfg.toml"));
    }
}
/// `effects` 子命令：`effects schema [--json]`（参数表透传，见 `commands::effects_schema`）。
pub(super) fn run_effects(args: &[String], json: bool) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("schema") => commands::effects_schema(json),
        other => Err(format!(
            "未知 effects 子命令：{}（可用 schema）",
            other.unwrap_or("")
        )),
    }
}
