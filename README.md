# vxapo-cli

CLI tool to inspect Windows audio APO registrations under `MMDevices\Audio`.

## Features

- Enumerate all playback and capture endpoints
- Display APO slot status per device (SFX, MFX, EFX) with system/third-party distinction
- Summarize active slots in endpoint list (`[SFX + EFX]`, `[MFX + EFX]`, `[none]`, `[Windows default]`)
- Show audio format details (sample rate, channels, bit depth, channel mask)
- Show SysFX and Enhancements status with correct flag parsing
- Dump raw registry data for any endpoint with friendly-name resolution for known property keys
- Annotate non-standard FxProperties slots and unknown device states
- Color-coded terminal output

## Behavior Reference

All APO slot definitions, installation modes, registry paths, and device state logic follow the behavioral specification of **Equalizer APO** (`DeviceAPOInfo`, `AbstractAPOInfo`, `ClassFactory`, `DllMain`).

## Usage

```bash
vxapo-cli.exe
```

Select an endpoint by number, then press `x` to view its registry details, `b` to go back.

## Build

```bash
cargo build --release
```

## Project Structure

```
src/
├── main.rs         # Entry point
├── app.rs          # Application loop and command dispatch
├── endpoint.rs     # Endpoint data model
├── probe.rs        # Endpoint enumeration (MMDevice + registry)
├── reg.rs          # Raw registry read helpers
├── regdump.rs      # Registry dump formatter
├── display.rs      # Terminal output and color rendering
└── knowledge.rs    # Known property key definitions and friendly-name maps
```

# 中文说明

检查 Windows 音频 APO 注册信息的命令行工具，从 `MMDevices\Audio` 注册表路径读取数据。

## 功能特性

- 枚举所有播放和录音端点设备
- 显示每个设备的 APO 槽位状态（SFX、MFX、EFX），区分系统 APO 和第三方 APO
- 在端点列表中汇总活跃槽位（`[SFX + EFX]`、`[MFX + EFX]`、`[none]`、`[Windows default]`）
- 显示音频格式详情（采样率、声道数、位深、通道掩码）
- 显示 SysFX 和 Enhancements 状态，正确解析标志位
- 导出任意端点的原始注册表数据，已知属性键自动映射为友好名称
- 标注非标准 FxProperties 槽位和无法识别的设备状态
- 彩色终端输出

## 行为规范参考

所有 APO 槽位定义、安装模式、注册表路径、设备状态判定逻辑均参考 **Equalizer APO** 的行为规范（`DeviceAPOInfo`、`AbstractAPOInfo`、`ClassFactory`、`DllMain`）。

## 使用方法

```bash
vxapo-cli.exe
```

选择端点对应的数字序号，然后按下：
- `x` 查看该端点的注册表详细信息
- `b` 返回上级菜单

## 编译构建

```bash
cargo build --release
```

## 项目结构

```
src/
├── main.rs         # 程序入口
├── app.rs          # 应用主循环和命令分发
├── endpoint.rs     # 端点数据模型
├── probe.rs        # 端点枚举（MMDevice + 注册表）
├── reg.rs          # 注册表原始读取辅助函数
├── regdump.rs      # 注册表导出格式化
├── display.rs      # 终端输出和颜色渲染
└── knowledge.rs    # 已知属性键定义和友好名称映射
```