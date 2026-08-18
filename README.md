# VxAPO CLI

VxAPO CLI is the command-line tool for managing VxAPO devices and configuration.

## Features

- Interactive mode with language selection (English / 中文)
- List/status devices with slot and format details
- Install and uninstall APOs
- Read and write per-device config.toml
- Snapshot and restore registry state
- JSON output for automation

## Build

```bash
cargo build --release
```

## Usage

### Interactive mode

Run without arguments:

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

Shows device index, name, GUID, install mode, slot occupancy, EAPO/lost status, sample rate, channels, bit depth, kind, and volume.

#### Install

```bash
vxapo-cli install -d <device>
vxapo-cli install -d <device> --mode SfxEfx
vxapo-cli install -d <device> --mode LfxGfx --no-child
```

`<device>` can be a `{GUID}` or the index shown by `list`.

- `--mode` selects the APO slot mode: `LfxGfx`, `SfxMfx`, or `SfxEfx`.
- `--no-child` disables preserving the original APO as a child APO.

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

Snapshots are registry-only baselines used to verify and roll back install/uninstall changes.

## Documentation

See `../vxapo-docs` for project documentation.

## License

GPL-3.0-or-later

---

# VxAPO CLI

VxAPO CLI 是用于管理 VxAPO 设备和配置的命令行工具。

## 功能

- 交互模式支持语言选择（English / 中文）
- 列出/查看设备槽位与格式信息
- 安装和卸载 APO
- 读写每个设备的 config.toml
- 快照与恢复注册表状态
- 支持 JSON 输出，便于自动化

## 构建

```bash
cargo build --release
```

## 用法

### 交互模式

直接运行：

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
vxapo-cli <命令> [参数]
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

项目文档见 `../vxapo-docs`。

## 许可证

GPL-3.0-or-later
