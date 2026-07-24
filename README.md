# vxapo-cli

CLI tool to inspect Windows audio APO registrations (SFX/MFX) under `MMDevices\Audio`.

---

## Features

- Enumerate all playback and capture endpoints
- Display SFX/MFX APO status per device (`[SFX+MFX]`, `[SFX only]`, `[MFX only]`, `[none]`, `[Windows default]`)
- Distinguish system default APOs from third-party APOs
- Dump raw registry data for any endpoint with filtering for meaningful values
- Color-coded terminal output

---

## Usage

```bash
vxapo-cli.exe
```

Select an endpoint by number, then press `x` to view its registry details.

---

## Build

```bash
cargo build --release
```

---

## 中文说明

检查 Windows 音频 APO 注册信息的命令行工具，读取 `MMDevices\Audio` 路径下的注册表数据。

**功能：**

- 枚举所有播放和录音端点
- 显示每个设备的 SFX/MFX APO 状态（`[SFX+MFX]`、`[SFX only]`、`[MFX only]`、`[none]`、`[Windows default]`）
- 区分系统默认 APO 和第三方 APO
- 查看任意端点的原始注册表数据（过滤显示有意义的值）
- 彩色终端输出

**使用：**

```bash
vxapo-cli.exe
```

输入端点对应的数字，按 `x` 查看注册表详情。

**编译：**

```bash
cargo build --release
```