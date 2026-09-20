//! commands/effects.rs — 效果器参数表透传（决策 2）
//!
//! `effects schema` 直接输出 driver 的 `effect_param_specs()`：app 侧
//! `npm run sync:driver-schema` 消费该 JSON 生成 `src/lib/effects.generated.ts`，
//! 参数范围/步进与 driver 默认值只在 driver 维护一份。
//! 注意：本表是 driver 的权威数值，不含 UI 显示精度——app 按自身取舍就近取整。

/// `effects schema`：参数表（`--json` 供 app 生成 TS，否则打印可读表）。
pub fn effects_schema(json: bool) -> Result<(), String> {
    let specs = vxapo_driver::effect_param_specs();
    if json {
        println!(
            "{}",
            serde_json::to_string(&specs).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    for effect in &specs {
        println!("{}", effect.effect);
        for p in &effect.params {
            println!(
                "  {:<16} {:<6} [{}, {}] step {} default {}",
                p.key,
                p.unit.unwrap_or("-"),
                p.min,
                p.max,
                p.step,
                p.default
            );
        }
    }
    Ok(())
}