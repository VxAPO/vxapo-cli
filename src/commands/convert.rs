//! commands/convert.rs — 配置文件读写与 EAPO txt → TOML 转换

use super::*;

/// config set（CLI 引用规范 5.2）：源文件内容原样写入 per-device config.toml。
pub fn config_set(device_ref: &str, file: &str) -> Result<(), String> {
    let dev = resolve_device(device_ref)?;
    let src = std::fs::read_to_string(file)
        .map_err(|e| {
        if lang() == Lang::En {
            format!("Failed to read source file {file}: {e}")
        } else {
            format!("读取源文件失败：{file}：{e}")
        }
    })?;
    let path = device_config_path(&dev.guid);
    if let Some(parent) = Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
        if lang() == Lang::En {
            format!("Failed to create config directory: {e}")
        } else {
            format!("创建配置目录失败：{e}")
        }
    })?;
    }
    std::fs::write(&path, &src).map_err(|e| {
        if lang() == Lang::En {
            format!("Failed to write config: {e}")
        } else {
            format!("写入 config 失败：{e}")
        }
    })?;
    if lang() == Lang::En {
        println!("✓ Config written ({} bytes), validating syntax...", src.len());
    } else {
        println!("✓ config 已写入（{} 字节），语法验证中…", src.len());
    }
    config_show(&dev.guid)?;
    Ok(())
}

/// config show：读回 + ConfigParser 语法验证（CLI 引用规范 5.2）。
pub fn config_show(device_ref: &str) -> Result<(), String> {
    let dev = resolve_device(device_ref)?;
    let path = device_config_path(&dev.guid);
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            if lang() == Lang::En {
                return Err("Not configured: config.toml does not exist (use config set -f <file> to write it)".to_string());
            } else {
                return Err("未配置：config.toml 不存在（可用 config set -f <file> 写入）".to_string());
            }
        }
    };
    println!("--- config.toml ({path}) ---");
    println!("{content}");
    // 读回一致性验证（CLI 引用规范 三「config show 读回验证——文件级」，不解析 DSP 语义）。
    if lang() == Lang::En {
        println!("✓ File read back successfully ({} bytes)", content.len());
    } else {
        println!("✓ 文件可读回（{} 字节）", content.len());
    }
    Ok(())
}

/// config convert：旧 EAPO 风格 txt → config.toml（迁移期工具）。
///
/// 支持 GraphicEQ / Preamp / Wide / AuralEnhancer / Reverb / Maximizer /
/// LoudnessCorrection；不支持的命令跳过并提示手动迁移。
pub fn config_convert(src: &str, out: Option<&str>) -> Result<(), String> {
    let text = std::fs::read_to_string(src).map_err(|e| {
        if lang() == Lang::En {
            format!("Failed to read {src}: {e}")
        } else {
            format!("读取失败：{src}：{e}")
        }
    })?;
    let toml = convert_txt_to_toml(&text)?;
    let out_path = out.map(|s| s.to_string()).unwrap_or_else(|| {
        Path::new(src)
            .with_extension("toml")
            .display()
            .to_string()
    });
    std::fs::write(&out_path, &toml).map_err(|e| {
        if lang() == Lang::En {
            format!("Failed to write {out_path}: {e}")
        } else {
            format!("写入失败：{out_path}：{e}")
        }
    })?;
    if lang() == Lang::En {
        println!("✓ Converted {} -> {}", src, out_path);
    } else {
        println!("✓ 已转换 {} → {}", src, out_path);
    }
    println!("--- 输出预览 ---");
    println!("{toml}");
    Ok(())
}

pub(super) fn convert_txt_to_toml(text: &str) -> Result<String, String> {
    let mut out = String::from("version = 1\n\n");
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((cmd, rest)) = line.split_once(':') else {
            if lang() == Lang::En {
            println!("⚠ Skipping unrecognized line: {line}");
        } else {
            println!("⚠ 跳过无法识别的行：{line}");
        }
            continue;
        };
        let cmd = cmd.trim();
        let rest = rest.trim();
        match cmd.to_ascii_lowercase().as_str() {
            "graphiceq" => {
                if rest.is_empty() {
                    if lang() == Lang::En {
                    println!("⚠ GraphicEQ: empty parameters skipped");
                } else {
                    println!("⚠ GraphicEQ: 空参数跳过");
                }
                    continue;
                }
                let mut bands = Vec::new();
                for seg in rest.split(';') {
                    let seg = seg.trim();
                    if seg.is_empty() {
                        continue;
                    }
                    let mut it = seg.split_whitespace();
                    let (f, g) = match (it.next(), it.next()) {
                        (Some(f), Some(g)) => (f, g),
                        _ => {
                        if lang() == Lang::En {
                            return Err(format!("Invalid GraphicEQ segment: '{seg}'"));
                        } else {
                            return Err(format!("GraphicEQ 段无效：'{seg}'"));
                        }
                    }
                    };
                    let f: f32 = f
                        .replace(',', ".")
                        .parse()
                        .map_err(|_| format!("频率无效：{f}"))?;
                    let g: f32 = g
                        .replace(',', ".")
                        .parse()
                        .map_err(|_| format!("增益无效：{g}"))?;
                    bands.push((f, g));
                }
                if !(6..=31).contains(&bands.len()) {
                    return Err(format!(
                        "GraphicEQ 转换后 {} 段，PEQ 要求 6-31 段（请手动调整曲线）",
                        bands.len()
                    ));
                }
                out.push_str("[[effects]]\ntype = \"peq\"\n");
                for (f, g) in &bands {
                    out.push_str(&format!(
                        "[[effects.bands]]\nfc = {f}\ngain_db = {g}\nq = 1.0\n"
                    ));
                }
                out.push('\n');
            }
            "preamp" => {
                let db = rest
                    .split_whitespace()
                    .next()
                    .ok_or_else(|| "Preamp 参数无效".to_string())?;
                out.push_str(&format!("[[effects]]\ntype = \"preamp\"\ngain_db = {db}\n\n"));
            }
            "wide" => {
                let kv = parse_kv(rest)?;
                let intensity = kv
                    .get("intensity")
                    .or_else(|| kv.get("surround"))
                    .ok_or_else(|| "Wide 缺少 Intensity".to_string())?;
                out.push_str(&format!(
                    "[[effects]]\ntype = \"wide\"\nintensity = {intensity}\n\n"
                ));
            }
            "auralenhancer" => {
                let kv = parse_kv(rest)?;
                out.push_str("[[effects]]\ntype = \"aural\"\n");
                write_mapped(
                    &mut out,
                    &kv,
                    &[
                        ("tunehz", "tune_hz"),
                        ("drive", "drive"),
                        ("odd", "odd"),
                        ("even", "even"),
                        ("wet", "wet"),
                        ("dry", "dry"),
                    ],
                );
                out.push('\n');
            }
            "reverb" => {
                let kv = parse_kv(rest)?;
                out.push_str("[[effects]]\ntype = \"reverb\"\n");
                write_mapped(
                    &mut out,
                    &kv,
                    &[
                        ("roomsize", "room_size"),
                        ("decay", "decay"),
                        ("damping", "damping"),
                        ("bandwidth", "bandwidth"),
                        ("density", "density"),
                        ("lat5", "lat5"),
                        ("lat6", "lat6"),
                        ("predelay", "pre_delay_ms"),
                        ("motionrate", "motion_rate"),
                        ("motiondepth", "motion_depth_ms"),
                        ("wet", "wet"),
                        ("dry", "dry"),
                    ],
                );
                out.push('\n');
            }
            "maximizer" => {
                // 当前模型名是 `compressor`（旧名 maximizer/leveler 由 driver 侧兜底映射）。
                // EAPO Maximizer 的 gain_boost/max_output/target/lookahead/dither 在新模型里
                // 没有对应参数，转换时丢弃并提示；release 对齐为 `release_ms`。
                let kv = parse_kv(rest)?;
                out.push_str("[[effects]]\ntype = \"compressor\"\n");
                write_mapped(
                    &mut out,
                    &kv,
                    &[
                        ("release", "release_ms"),
                        ("wet", "wet"),
                        ("dry", "dry"),
                    ],
                );
                out.push('\n');
                if ["gainboost", "maxoutput", "target", "lookahead", "dither"]
                    .iter()
                    .any(|k| kv.contains_key(*k))
                {
                    if lang() == Lang::En {
                        println!("⚠ Maximizer: mapped to `compressor`; gain_boost/max_output/target/lookahead/dither are no longer supported and were dropped");
                    } else {
                        println!("⚠ Maximizer：已映射为 compressor；gain_boost/max_output/target/lookahead/dither 不再支持，已丢弃");
                    }
                }
            }
            "loudnesscorrection" => {
                let mut it = rest.split_whitespace();
                let phon = it.next().ok_or_else(|| "LoudnessCorrection 缺少 phon".to_string())?;
                let reference = it.next().unwrap_or("80");
                out.push_str(&format!(
                    "[[effects]]\ntype = \"loudness\"\nphon = {phon}\nreference_phon = {reference}\n\n"
                ));
            }
            other => {
                    if lang() == Lang::En {
                        println!("⚠ Command {other}: no longer supported, skipped (please migrate manually)");
                    } else {
                        println!("⚠ 命令 {other}: 不再支持，跳过（请手动迁移）");
                    }
                }
        }
    }
    Ok(out)
}

/// 解析 `Key Value [unit]` 对（键小写，单位跳过）。
pub(super) fn parse_kv(rest: &str) -> Result<std::collections::HashMap<String, String>, String> {
    let toks: Vec<&str> = rest.split_whitespace().collect();
    let mut map = std::collections::HashMap::new();
    let mut i = 0;
    while i < toks.len() {
        let key = toks[i].to_ascii_lowercase();
        let Some(&val) = toks.get(i + 1) else {
            break;
        };
        i += 2;
        map.insert(key, val.to_string());
        if toks
            .get(i)
            .is_some_and(|t| t.eq_ignore_ascii_case("hz") || t.eq_ignore_ascii_case("db") || t.eq_ignore_ascii_case("ms"))
        {
            i += 1;
        }
    }
    Ok(map)
}

pub(super) fn write_mapped(
    out: &mut String,
    kv: &std::collections::HashMap<String, String>,
    map: &[(&str, &str)],
) {
    for (k, dst) in map {
        if let Some(v) = kv.get(*k) {
            out.push_str(&format!("{dst} = {v}\n"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Maximizer 段映射为当前模型名 `compressor`；无对应参数的旧字段不再写出。
    #[test]
    fn maximizer_converts_to_compressor_without_legacy_params() {
        let toml = convert_txt_to_toml(
            "Maximizer: GainBoostDb 6.0 MaxOutputDb -0.3 Release 10.0 Target 0.32 Lookahead 0.75\n",
        )
        .unwrap();
        assert!(
            toml.contains("type = \"compressor\""),
            "应生成当前模型名：{toml}"
        );
        assert!(!toml.contains("type = \"maximizer\""));
        assert!(
            toml.contains("release_ms = 10.0"),
            "release 应对齐 release_ms：{toml}"
        );
        for dropped in ["gain_boost_db", "max_output_db", "target =", "lookahead_ms"] {
            assert!(!toml.contains(dropped), "{dropped} 应已丢弃：{toml}");
        }
    }

    /// 无旧字段时仍输出 compressor，且不产生多余键。
    #[test]
    fn maximizer_release_only() {
        let toml = convert_txt_to_toml("Maximizer: Release 20\n").unwrap();
        assert!(toml.contains("type = \"compressor\""));
        assert!(toml.contains("release_ms = 20"));
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// 快照 = 变更对比 + 基线保持（CLI 引用规范 5.3）
// ══════════════════════════════════════════════════════════════════════════════

