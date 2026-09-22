# VxAPO CLI

<!-- 徽章区（待补）：CI 状态 · 许可证 · 最近发布 -->

[中文](#vxapo-cli) · [English](#vxapo-cli-english)

VxAPO CLI 是 VxAPO 的命令行工具，负责设备管理、配置读写与诊断。App（Tauri）通过提权子进程
调用它完成安装、卸载与验证。CLI 把注册表写入交给 driver 的事务层，自己不决定槽位策略。

阅读顺序：定位与边界 → 命令一览 → 安装与验证 → 交互模式 → 契约 crate → 与 Driver / App 的
边界 → 上手 → 测试 → 参考。

## 1 · 定位与边界

| CLI 负责 | CLI 不负责 |
|---|---|
| 端点枚举与状态展示（`list` / `status`） | 槽位模式的选择与写入策略（driver `install/selector`） |
| 调用 driver 的安装 / 卸载事务 | 回滚：CLI 没有回滚子命令；App 的 `rollback_install` 失败兜底时改调 `vxapo-cli uninstall` |
| 配置文件读写与旧格式转换（`config`） | 配置的校验规则（边界由 driver 定义，CLI 只按边界限幅） |
| 注册表基线快照（`snapshot`） | 实时热重载（driver 侧事件驱动） |
| 效果器参数表透传（`effects schema`） | 参数范围的维护（来源是 driver 的 `effect_param_specs()`） |
| 旧 GUID 残留的查询 / 迁移 / 清理（`stale`） | 设备端点的身份判定（以设备实例 ID 为稳定身份） |

## 2 · 命令一览

| 命令 | 作用 | 关键选项 / 子命令 | 实现位置 |
|---|---|---|---|
| `list` / `status` | 枚举端点 | `--json` 供 App 与脚本消费 | `src/commands/status.rs` |
| `install -d <device>` | 安装 | `--mode`、`--no-child`、`--verify`、`--progress-file`、`--timeout` | `src/commands/install.rs` |
| `uninstall -d <device>` | 卸载 | `-d` 或裸 `<device>` 均可 | `src/commands/uninstall.rs` |
| `register` | 注册 COM CLSID | 使用 exe 同级的 `vxapo_driver.dll` | `src/commands/register.rs` |
| `config show -d <device>` | 读配置 | 每设备 `C:\ProgramData\VxAPO\{GUID}\config.toml` | `src/commands/convert.rs` |
| `config set -d <device> -f <file>` | 写配置 | 写回前按 driver 边界限幅 | 同上 |
| `config convert <old> <out>` | 旧文本配置转 TOML | 一次性迁移 | 同上 |
| `snapshot create` / `diff` / `restore -d <device>` | 注册表基线 | 只动注册表，不触碰配置内容 | `src/commands/snapshot.rs` |
| `stale list` / `migrate` / `cleanup` | 旧 GUID 残留 | 以设备实例 ID 匹配活跃端点 | `src/commands/install.rs` |
| `effects schema [--json]` | 效果器参数表 | App 据此生成 TS 表 | `src/commands/effects.rs` |
| `help` / 无参数 | 打印帮助 / 进入交互模式 | — | `src/main.rs` |

- **`--verify` 不是默认开启**。`install` 的签名是 `verify: bool`（`src/commands/install.rs`）。
  不传该开关时，`install` 直接调用 driver 的 `install_endpoint`，不跑验证闭环。
- **CLI 没有 `rollback` 子命令**。回滚是 App 侧兜底：`rollback_install`
  （`vxapo-app/src-tauri/src/lib.rs`）改调 `vxapo-cli uninstall`。

## 3 · 安装与验证

`install --verify` 的执行顺序（`src/commands/install.rs` 与 `src/verify.rs`）：

1. `require_admin()`：非管理员直接失败。
2. `auto_register_driver()`：定位 exe 同级的 `vxapo_driver.dll`，注册 COM CLSID 绑定。
3. 确保 `config.toml` 存在。缺失时导入默认配置，使验证通过后的状态就是最终可用状态。
4. 走验证闭环：停服务 → 启服务 → `CoCreateInstance` → `GetMixFormat` → `Initialize` →
   `test_pipe` 回环。

`--progress-file <path>` 把阶段事件写成 JSON 事件流：`register` / `config` / `verify` 等阶段
由 `emit_phase` 发出，App 后端订阅后展示进度。

不带 `--verify` 时，`install` 只调 driver 的 `install_endpoint`。`DisableProtectedAudioDG`、
槽位与 `ProcessingModes` 写入、安装后重启都由 driver 在该调用内完成，CLI 不重复。

## 4 · 交互模式（`src/tui.rs` / `src/i18n.rs`）

无参数运行进入交互模式。程序先询问语言，再询问模式。

| 选择 | 模式 | 内容 |
|---|---|---|
| `1` | 查看模式 | 枚举端点，检查槽位 / 格式 / 增强 |
| `2` | Driver 模式 | 安装 / 卸载 / 配置 / 快照 |
| `q` | — | 退出 |

## 5 · 契约 crate（`protocol/`）

本仓是 cargo workspace，成员 `protocol/` 只依赖 serde 与 ts-rs，**不依赖 driver**：

- 安装 / 卸载进度事件：`InstallProgressEvent`（`#[serde(tag = "event")]`，六类 variant）。
- 残留列表：`StaleInstall` / `StaleTargetState` / `StaleMatchedBy`。迁移报告：`MigrationReport`。
- App 侧类型由 ts-rs 生成，两端因此不各自维护一份。

进度事件以类型化方式发射（`EventSink::emit` 接收 `impl Serialize`），并有逐字节形状回归测试
锁定输出形状。stale 相关 JSON 在 CLI 侧从 driver 类型映射，`protocol` 因此保持无 driver 依赖。

## 6 · 与 Driver / App 的边界

| 事项 | 决定方 | CLI 的角色 |
|---|---|---|
| 安装模式与子 APO 保留策略 | driver `install/selector` | 透传参数 |
| 数值边界（增益 `[-120,+48]`、段数上限、NaN/inf） | driver | 写回前按边界限幅。driver 只在内存 clamp |
| 配置模型（`version=1` / `enabled` / `[meta]` / `[[effects]]`） | driver | `config convert` 按当前模型生成 |
| 旧类型名兼容（`maximizer` / `leveler` / `auralenhancer` / `loudnesscorrection`） | driver 解析时映射 | 不做额外处理 |
| 注册表写入 | driver 事务层 | 只调用，不自己拼槽位 |

App 不直接写注册表，一律经提权 CLI 执行。

## 7 · 快速上手

前提：Windows 8.1+ 与 Rust（MSVC）工具链。对设备做安装 / 卸载需要管理员权限。

```bash
cargo build --release                      # 产物 vxapo-cli.exe

vxapo-cli list                             # 查设备（序号或 {GUID}）
vxapo-cli install -d 0 --mode SfxEfx --verify   # 安装并跑验证闭环
vxapo-cli snapshot diff -d 0               # 核对注册表变更
vxapo-cli uninstall -d 0
```

## 8 · 测试

```bash
cargo test        # 22 个测试
```

## 9 · 设计参考与致谢

安装模型（逐设备注册 APO 槽位、把原 APO 保留为子 APO）与验证流程参考了
[Equalizer APO](https://sourceforge.net/projects/equalizerapo/) 的公开实践。本工具是独立实现，
不含 Equalizer APO 代码。Equalizer APO © Jonas Thedering，GPL-2.0。

## 文档与许可

- 项目文档见 [`../vxapo-docs`](../vxapo-docs)，CLI 规范见 [`../vxapo-docs/cli`](../vxapo-docs/cli)。
- 许可证：GPL-3.0-or-later。

---

<a id="vxapo-cli-english"></a>

# VxAPO CLI

<!-- Badges (TODO): CI status · license · latest release -->

[中文](#vxapo-cli) · [English](#vxapo-cli-english)

VxAPO CLI is the command-line tool for VxAPO. It manages devices, reads and writes config,
and runs diagnostics. The App (Tauri) calls it as an elevated subprocess for install,
uninstall and verification. The CLI hands registry writes to the driver transaction layer
and does not choose the slot strategy itself.

Reading order: scope and boundaries → commands → install and verification → interactive
mode → contract crate → boundaries with the Driver / App → getting started → tests →
references.

## 1 · Scope and boundaries

| The CLI owns | The CLI does not own |
|---|---|
| Endpoint enumeration and status output (`list` / `status`) | Slot mode selection and the write strategy (driver `install/selector`) |
| Calls to the driver install / uninstall transaction | Rollback. The CLI has no rollback command. The App's `rollback_install` fallback calls `vxapo-cli uninstall` instead |
| Config read/write and legacy format conversion (`config`) | Config validation rules. The driver defines the bounds, and the CLI only clamps to them |
| Registry baseline snapshots (`snapshot`) | Hot reload. The driver watches file events |
| Relaying the effect parameter table (`effects schema`) | Parameter range maintenance. The ranges come from the driver's `effect_param_specs()` |
| Query, migration and cleanup of stale GUIDs (`stale`) | Endpoint identity. The driver matches on the device instance ID |

## 2 · Commands

| Command | Purpose | Key options / subcommands | Where implemented |
|---|---|---|---|
| `list` / `status` | Enumerate endpoints | `--json` for the App and scripts | `src/commands/status.rs` |
| `install -d <device>` | Install | `--mode`, `--no-child`, `--verify`, `--progress-file`, `--timeout` | `src/commands/install.rs` |
| `uninstall -d <device>` | Uninstall | `-d` and a bare `<device>` both work | `src/commands/uninstall.rs` |
| `register` | Register the COM CLSID | Uses `vxapo_driver.dll` next to the executable | `src/commands/register.rs` |
| `config show -d <device>` | Read config | Per device `C:\ProgramData\VxAPO\{GUID}\config.toml` | `src/commands/convert.rs` |
| `config set -d <device> -f <file>` | Write config | Clamps to driver bounds before writing | same |
| `config convert <old> <out>` | Legacy text config to TOML | One-time migration | same |
| `snapshot create` / `diff` / `restore -d <device>` | Registry baseline | Touches the registry only, never config content | `src/commands/snapshot.rs` |
| `stale list` / `migrate` / `cleanup` | Stale GUID records | Matches active endpoints by device instance ID | `src/commands/install.rs` |
| `effects schema [--json]` | Effect parameter table | The App generates its TS table from this | `src/commands/effects.rs` |
| `help` / no arguments | Print help / enter interactive mode | — | `src/main.rs` |

- **`--verify` is not the default.** The `install` signature takes `verify: bool`
  (`src/commands/install.rs`). Without the flag, `install` calls the driver's
  `install_endpoint` directly and runs no verification loop.
- **The CLI has no `rollback` subcommand.** Rollback is an App-side fallback:
  `rollback_install` (`vxapo-app/src-tauri/src/lib.rs`) calls `vxapo-cli uninstall`.

## 3 · Install and verification

`install --verify` runs these steps in order (`src/commands/install.rs`, `src/verify.rs`):

1. `require_admin()` fails the run when the process is not elevated.
2. `auto_register_driver()` locates `vxapo_driver.dll` next to the executable and registers
   the COM CLSID binding.
3. The CLI makes sure `config.toml` exists. When it is missing, the CLI imports the default
   config, so a passing verification leaves the device in its final usable state.
4. The verification loop runs: stop service → start service → `CoCreateInstance` →
   `GetMixFormat` → `Initialize` → `test_pipe` round trip.

`--progress-file <path>` writes the phase events as a JSON stream. `emit_phase` emits phases
such as `register`, `config` and `verify`. The App backend subscribes and shows progress.

Without `--verify`, `install` calls only the driver's `install_endpoint`. That call handles
`DisableProtectedAudioDG`, the slot and `ProcessingModes` writes, and the post-install
restart. The CLI does not repeat any of it.

## 4 · Interactive mode (`src/tui.rs` / `src/i18n.rs`)

Run the tool with no arguments to enter interactive mode. It asks for a language first, then
for a mode.

| Choice | Mode | Contents |
|---|---|---|
| `1` | Viewer | Enumerate endpoints and inspect slots, formats and enhancements |
| `2` | Driver | Install, uninstall, config and snapshot operations |
| `q` | — | Quit |

## 5 · Contract crate (`protocol/`)

This repository is a cargo workspace. The `protocol/` member depends on serde and ts-rs only,
and **never on the driver**:

- Install and uninstall progress events: `InstallProgressEvent`, with six variants tagged by
  `event`.
- Stale records: `StaleInstall` / `StaleTargetState` / `StaleMatchedBy`. Migration report:
  `MigrationReport`.
- ts-rs generates the App-side types, so the two sides do not maintain separate copies.

The CLI emits progress events as typed values (`EventSink::emit` takes `impl Serialize`), and
byte-exact shape regression tests pin the output. The CLI maps stale JSON from driver types,
which keeps `protocol` free of driver dependencies.

## 6 · Boundaries with the Driver and App

| Item | Decided by | The CLI's role |
|---|---|---|
| Install mode and child-APO preservation policy | driver `install/selector` | Passes parameters through |
| Numeric bounds (gain `[-120,+48]`, band-count limit, NaN/inf) | driver | Clamps on write. The driver clamps in memory only |
| Config model (`version=1` / `enabled` / `[meta]` / `[[effects]]`) | driver | `config convert` generates the current model |
| Legacy type names (`maximizer` / `leveler` / `auralenhancer` / `loudnesscorrection`) | driver parse time | No extra handling |
| Registry writes | driver transaction layer | Calls into it and never assembles slots itself |

The App never writes the registry directly. It always goes through the elevated CLI.

## 7 · Getting started

Prerequisites: Windows 8.1+ and a Rust (MSVC) toolchain. Device install and uninstall need
administrator rights.

```bash
cargo build --release                      # produces vxapo-cli.exe

vxapo-cli list                             # list devices (index or {GUID})
vxapo-cli install -d 0 --mode SfxEfx --verify   # install and verify
vxapo-cli snapshot diff -d 0               # inspect registry changes
vxapo-cli uninstall -d 0
```

## 8 · Tests

```bash
cargo test        # 22 tests
```

## 9 · Design references and acknowledgments

The install model (per-device APO slot registration, keeping the original APO as a child)
and the verification workflow reference the public practice of
[Equalizer APO](https://sourceforge.net/projects/equalizerapo/). This tool is an independent
implementation and contains no Equalizer APO code. Equalizer APO © Jonas Thedering, GPL-2.0.

## Documentation and license

- Project documentation: [`../vxapo-docs`](../vxapo-docs). CLI reference:
  [`../vxapo-docs/cli`](../vxapo-docs/cli).
- License: GPL-3.0-or-later.
