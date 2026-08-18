# VxAPO CLI

VxAPO CLI is the command-line tool for managing VxAPO devices and configuration.

## Features

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

```bash
vxapo-cli list
vxapo-cli install -d <device>
vxapo-cli uninstall -d <device>
vxapo-cli config show -d <device>
vxapo-cli config set -d <device> -f <file>
vxapo-cli snapshot diff -d <device>
```

## Documentation

See `../vxapo-docs` for project documentation.

## License

GPL-3.0-or-later

---

# VxAPO CLI

VxAPO CLI 是用于管理 VxAPO 设备和配置的命令行工具。

## 功能

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

```bash
vxapo-cli list
vxapo-cli install -d <device>
vxapo-cli uninstall -d <device>
vxapo-cli config show -d <device>
vxapo-cli config set -d <device> -f <file>
vxapo-cli snapshot diff -d <device>
```

## 文档

项目文档见 `../vxapo-docs`。

## 许可证

GPL-3.0-or-later
