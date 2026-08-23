# VxAPO CLI

VxAPO CLI 是 VxAPO 的命令行工具，负责设备管理、配置读写与诊断。它是
**安装/卸载/注册表操作的唯一入口层**：App（Tauri）后端通过提权子进程调用它完成
安装、卸载、回滚与验证；CLI 本身把注册表写入收敛到 driver 的统一事务层。

## 功能与实现

### 设备与安装（`src/commands.rs` + driver 的 `install/`）

- 端点枚举与状态：`list` 展示设备序号、名称、GUID、安装模式、槽位占用、EAPO/失守状态、
  采样率/声道/位深/类型/音量；`--json` 输出供 App/脚本消费。
- 安装 / 卸载：`install -d <device> [--mode LfxGfx|SfxMfx|SfxEfx] [--no-child]` /
  `uninstall -d <device>`。安装前自动定位 exe 同级 `vxapo_driver.dll` 并注册 COM
  CLSID 绑定（新机器无需手动 regsvr32）；注册表写入统一经 driver 事务层。
- 验证闭环：`install --verify --progress-file` 逐阶段 emit JSON 事件
  （写配置 / 停服务 / 启服务 / 验证 / 重试 / 完成），App 后端订阅 `install-progress`
  展示进度；失败时 best 配置保留、可回滚（`rollback` 走卸载）。

### 配置（`src/regdump.rs` / `src/verify.rs` 等）

- `config show` / `config set -f <toml>`：读写每设备 `C:\ProgramData\VxAPO\{GUID}\config.toml`。
- `config convert old.txt out.toml`：旧式文本配置转 TOML。
- 配置模型与 driver 完全一致（`[[effects]]`，见下方“行为对齐”）。

### 快照（`src/reg.rs`）

- `snapshot create / diff / restore`：注册表基线快照，用于验证和回滚安装/卸载变更，
  不触碰配置内容。

### 交互模式（`src/app.rs` / `src/i18n.rs`）

- 无参数运行进入交互模式：先选语言（English / 中文），再选查看模式（枚举端点/检查槽位）
  或 Driver 模式（安装/卸载/配置/快照）。

## 与 Driver / App 的行为对齐

- **安装模型**：APO 槽位模式与子 APO 保留策略由 driver `install/selector` 决定，
  CLI 只透传参数；App 不直接写注册表，一律经 CLI 提权执行。
- **配置契约**：`version=1` / `enabled` / `[meta]` / `[[effects]]`；
  旧类型（maximizer/leveler/auralenhancer/loudnesscorrection）在 driver 解析时兼容，
  CLI 的 `config convert` 同样按当前模型生成。
- **限幅**：CLI/App 写回时按 driver 数值边界主动限幅（增益 `[-120,+48]`、31 段上限、
  NaN/inf 拒绝），driver 只在内存 clamp、不回写。
- **诊断**：`verify` 复用 driver 的 `test_pipe` / 格式协商，保证“装了就能出声”。

## 致谢 Equalizer APO

安装模型（逐设备注册 APO 槽位、保留原 APO 为子 APO）与验证流程的设计
参考了 [Equalizer APO](https://sourceforge.net/projects/equalizerapo/) 的实践；
本工具为独立实现，不包含 Equalizer APO 代码。Equalizer APO © Jonas Thedering，GPL-2.0。

## 构建

```bash
cargo build --release
```

## 用法

### 交互模式

```bash
vxapo-cli
```

程序会先询问语言：

```text
Select language / 选择语言:
  [1] English
  [2] 中文
```

然后可以选择：

- `1` 查看模式：枚举端点并检查槽位/格式/增强。
- `2` Driver 模式：安装/卸载/配置/快照操作。
- `q` 退出。

### 子命令模式

```bash
vxapo-cli <command> [options]
```

#### 列出设备

```bash
vxapo-cli list
vxapo-cli list --json
```

显示设备序号、名称、GUID、安装模式、槽位占用、EAPO/失守状态、采样率、声道数、位深、类型和音量。

#### 安装

```bash
vxapo-cli install -d <device>
vxapo-cli install -d <device> --mode SfxEfx
vxapo-cli install -d <device> --mode LfxGfx --no-child
```

`<device>` 可以是 `{GUID}` 或 `list` 显示的序号。

- `--mode` 选择 APO 槽位模式：`LfxGfx`、`SfxMfx` 或 `SfxEfx`。
- `--no-child` 不保留原 APO 作为子 APO。
- `--verify`（默认）与 `--progress-file`：安装后逐阶段验证，进度以 JSON 事件写出。

#### 卸载

```bash
vxapo-cli uninstall -d <device>
```

#### 配置

```bash
vxapo-cli config show -d <device>
vxapo-cli config set -d <device> -f ./config.toml
vxapo-cli config convert old.txt out.toml
```

#### 快照

```bash
vxapo-cli snapshot create -d <device>
vxapo-cli snapshot diff -d <device>
vxapo-cli snapshot restore -d <device>
```

快照只保存注册表基线，用于验证和回滚安装/卸载变更。

## 文档

项目文档见 `../vxapo-docs`，详细规范见 `../vxapo-docs/cli` 与 `../vxapo-docs/driver`。

## 许可证

GPL-3.0-or-later

---

# VxAPO CLI

VxAPO CLI is the command-line tool for managing VxAPO devices and configuration. It is
the **single entry point for install/uninstall/registry operations**: the App (Tauri)
backend invokes it as an elevated subprocess for install, uninstall, rollback, and
verification; registry writes are funneled through the driver's unified transaction layer.

## Features & implementation

### Devices & install (`src/commands.rs` + driver `install/`)

- Endpoint enumeration and status: `list` shows index, name, GUID, install mode, slot
  occupancy, EAPO/lost status, sample rate, channels, bit depth, kind, and volume;
  `--json` output for the App/scripts.
- Install / uninstall: `install -d <device> [--mode LfxGfx|SfxMfx|SfxEfx] [--no-child]` /
  `uninstall -d <device>`. Before installing, the CLI auto-locates `vxapo_driver.dll`
  next to the executable and registers the COM CLSID binding (no manual regsvr32 on
  fresh machines); registry writes go through the driver transaction layer.
- Verification loop: `install --verify --progress-file` emits JSON events per phase
  (write config / stop service / start service / verify / retry / complete); the App
  backend subscribes to `install-progress`; on failure the best config is kept and a
  rollback can run.

### Config (`src/regdump.rs` / `src/verify.rs`)

- `config show` / `config set -f <toml>`: read/write per-device
  `C:\ProgramData\VxAPO\{GUID}\config.toml`.
- `config convert old.txt out.toml`: legacy text config → TOML.
- The config model matches the driver exactly (`[[effects]]`, see "Alignment" below).

### Snapshot (`src/reg.rs`)

- `snapshot create / diff / restore`: registry-only baselines used to verify and roll
  back install/uninstall changes; never touches config content.

### Interactive mode (`src/app.rs` / `src/i18n.rs`)

- Run without arguments to enter interactive mode: choose a language
  (English / 中文), then Viewer mode (enumerate endpoints / inspect slots) or
  Driver mode (install/uninstall/config/snapshot).

## Alignment with the Driver / App

- **Install model**: APO slot modes and the child-APO preservation policy are decided
  by the driver `install/selector`; the CLI only passes parameters. The App never
  writes the registry directly — it always goes through the elevated CLI.
- **Config contract**: `version=1` / `enabled` / `[meta]` / `[[effects]]`; legacy type
  names (`maximizer`/`leveler`/`auralenhancer`/`loudnesscorrection`) are compatible at
  driver parse time, and `config convert` generates the current model.
- **Clamping**: write-side clamping follows driver bounds (gain `[-120,+48]`, 31-band
  limit, NaN/inf rejected); the driver only clamps in memory and never writes back.
- **Diagnostics**: `verify` reuses the driver's `test_pipe` / format negotiation to
  guarantee "installed = working".

## Acknowledgments: Equalizer APO

The install model (per-device APO slot registration, preserving the original APO as a
child) and the verification workflow are inspired by
[Equalizer APO](https://sourceforge.net/projects/equalizerapo/); this tool is an
independent implementation with no Equalizer APO code. Equalizer APO © Jonas Thedering,
GPL-2.0.

## Build

```bash
cargo build --release
```

## Usage

### Interactive mode

```bash
vxapo-cli
```

The program first asks you to select a language:

```text
Select language / 选择语言:
  [1] English
  [2] 中文
```

Then you can choose:

- `1` Viewer mode: enumerate endpoints and inspect slots/formats/enhancements.
- `2` Driver mode: install/uninstall/config/snapshot operations.
- `q` Quit.

### Subcommand mode

```bash
vxapo-cli <command> [options]
```

#### List devices

```bash
vxapo-cli list
vxapo-cli list --json
```

Shows device index, name, GUID, install mode, slot occupancy, EAPO/lost status, sample
rate, channels, bit depth, kind, and volume.

#### Install

```bash
vxapo-cli install -d <device>
vxapo-cli install -d <device> --mode SfxEfx
vxapo-cli install -d <device> --mode LfxGfx --no-child
```

`<device>` can be a `{GUID}` or the index shown by `list`.

- `--mode` selects the APO slot mode: `LfxGfx`, `SfxMfx`, or `SfxEfx`.
- `--no-child` disables preserving the original APO as a child APO.
- `--verify` (default) with `--progress-file`: phase-by-phase verification, progress
  written as JSON events.

#### Uninstall

```bash
vxapo-cli uninstall -d <device>
```

#### Config

```bash
vxapo-cli config show -d <device>
vxapo-cli config set -d <device> -f ./config.toml
vxapo-cli config convert old.txt out.toml
```

#### Snapshot

```bash
vxapo-cli snapshot create -d <device>
vxapo-cli snapshot diff -d <device>
vxapo-cli snapshot restore -d <device>
```

Snapshots are registry-only baselines used to verify and roll back install/uninstall
changes.

## Documentation

See `../vxapo-docs`; detailed references under `../vxapo-docs/cli` and
`../vxapo-docs/driver`.

## License

GPL-3.0-or-later
