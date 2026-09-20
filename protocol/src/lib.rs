//! VxAPO CLI `--json` 输出契约（CLI 与 App 的单一来源）。
//!
//! 只含纯数据结构：不依赖 `windows` / driver，`src-tauri` 可 path 依赖本 crate 并
//! 生成 TS 类型：
//!
//! ```text
//! TS_RS_EXPORT_DIR=../../vxapo-app/src/lib/generated cargo test -p vxapo-protocol export_bindings
//! ```
//!
//! App 的 `model.ts` / `api.ts` 直接 re-export 生成类型，不再维护第二份手写定义。
//! 字段名与 serde 输出即 CLI 的 `--json` 契约，改动此处即改契约。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// `list --json` 的设备项（JSON 数组元素）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Device {
    /// 枚举序号（与交互菜单/`-d <index>` 一致）。
    pub index: u32,
    pub name: String,
    pub guid: String,
    pub device_id: String,
    /// 连接类型（当前固定空串，保留字段给 App 展示）。
    pub connection: String,
    pub installed_version: String,
    pub install_mode: String,
    pub slots: DeviceSlots,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub bit_depth: Option<u32>,
    pub kind: DeviceKind,
    /// 端点主音量（0.0–1.0；查询失败为 null）。
    pub volume: Option<f32>,
    /// EAPO 占用状态；未占用时省略该字段（与既有 JSON 一致）。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub eapo: Option<String>,
    /// 槽位失守描述；未失守时省略该字段。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lost_slot: Option<String>,
}

/// 设备类型（`probe` 的流向来向）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DeviceKind {
    Playback,
    Capture,
}

/// 5 槽位占用（`null` = 空槽）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DeviceSlots {
    #[serde(rename = "LFX")]
    pub lfx: Option<String>,
    #[serde(rename = "GFX")]
    pub gfx: Option<String>,
    #[serde(rename = "SFX")]
    pub sfx: Option<String>,
    #[serde(rename = "MFX")]
    pub mfx: Option<String>,
    #[serde(rename = "EFX")]
    pub efx: Option<String>,
}

impl DeviceSlots {
    /// 按 `[LFX, GFX, SFX, MFX, EFX]` 顺序取值（与 `enumerate_devices` 一致）。
    pub fn from_slice(slots: [Option<String>; 5]) -> Self {
        let [lfx, gfx, sfx, mfx, efx] = slots;
        Self {
            lfx,
            gfx,
            sfx,
            mfx,
            efx,
        }
    }
}

/// 命令成功回执（`install` / `uninstall` / `stale cleanup` / `stale fix-acl`）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CliOk {
    /// 恒为 `true`（保留字段以便 App 判别回执类型）。
    pub ok: bool,
    pub device: String,
    /// 面向用户的结论文案（由调用方按语言给出）。
    pub message: String,
    /// 安装模式（仅 `install` 回执携带）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

impl CliOk {
    /// 构造成功回执；`message` 由调用方按语言给出。
    pub fn new(device: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: true,
            device: device.into(),
            message: message.into(),
            mode: None,
        }
    }

    /// 附带安装模式（`install` 回执用）。
    pub fn with_mode(mut self, mode: impl Into<String>) -> Self {
        self.mode = Some(mode.into());
        self
    }

    /// 序列化为单行 JSON（CLI 的 `--json` 输出）。
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| format!(r#"{{"ok":false,"error":"{e}"}}"#))
    }
}

/// 命令失败回执（`--json` 下的错误输出，进程退出码 1）。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CliError {
    /// 恒为 `false`。
    pub ok: bool,
    pub error: String,
}

impl CliError {
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: error.into(),
        }
    }

    /// 序列化为单行 JSON（CLI 的 `--json` 错误输出）。
    pub fn to_json(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|_| r#"{"ok":false,"error":"serialize failed"}"#.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_json_shape_is_stable() {
        let d = Device {
            index: 0,
            name: "Spk".into(),
            guid: "{ABC}".into(),
            device_id: "dev".into(),
            connection: String::new(),
            installed_version: "1.0".into(),
            install_mode: "SfxEfx".into(),
            slots: DeviceSlots::from_slice([Some("VxAPO PreMix".into()), None, None, None, None]),
            sample_rate: Some(48_000),
            channels: Some(2),
            bit_depth: Some(32),
            kind: DeviceKind::Playback,
            volume: Some(0.5),
            eapo: None,
            lost_slot: None,
        };
        let json = serde_json::to_string(&d).unwrap();
        // 空可选字段省略（与既有 CLI 输出一致），槽位保持 LFX..EFX 顺序与 null 空槽。
        assert!(!json.contains("eapo"), "{json}");
        assert!(!json.contains("lost_slot"), "{json}");
        assert!(
            json.contains(r#""slots":{"LFX":"VxAPO PreMix","GFX":null,"SFX":null,"MFX":null,"EFX":null}"#),
            "{json}"
        );
        assert!(json.contains(r#""kind":"playback""#), "{json}");
        let back: Device = serde_json::from_str(&json).unwrap();
        assert_eq!(back.guid, d.guid);
        assert_eq!(back.kind, DeviceKind::Playback);
    }

    #[test]
    fn cli_ok_and_error_shapes() {
        assert_eq!(
            CliOk::new("g", "已安装").with_mode("SfxEfx").to_json(),
            r#"{"ok":true,"device":"g","message":"已安装","mode":"SfxEfx"}"#
        );
        assert_eq!(
            CliOk::new("g", "cleaned").to_json(),
            r#"{"ok":true,"device":"g","message":"cleaned"}"#
        );
        assert_eq!(CliError::new("boom").to_json(), r#"{"ok":false,"error":"boom"}"#);
    }

    #[test]
    fn cli_error_escapes_quotes() {
        assert_eq!(
            CliError::new(r#"bad "x" \ path"#).to_json(),
            r#"{"ok":false,"error":"bad \"x\" \\ path"}"#
        );
    }


    /// 进度事件的 JSON 形状与 CLI 原手写 `json!` 输出逐字节一致（契约回归）。
    #[test]
    fn progress_event_shapes_match_legacy_json() {
        let cases: Vec<(InstallProgressEvent, &str)> = vec![
            (
                InstallProgressEvent::Phase {
                    name: "pre-verify".into(),
                },
                r#"{"event":"phase","name":"pre-verify"}"#,
            ),
            (
                InstallProgressEvent::InstallWrite {
                    mode: "SfxEfx".into(),
                },
                r#"{"event":"install_write","mode":"SfxEfx"}"#,
            ),
            (
                InstallProgressEvent::Service {
                    action: ServiceAction::Stopping,
                },
                r#"{"event":"service","action":"stopping"}"#,
            ),
            (
                InstallProgressEvent::Test {
                    mode: "SfxEfx".into(),
                    pipe: None,
                },
                r#"{"event":"test","mode":"SfxEfx"}"#,
            ),
            (
                InstallProgressEvent::Test {
                    mode: "SfxEfx".into(),
                    pipe: Some(r"\\.\pipe\vxapo_test".into()),
                },
                r#"{"event":"test","mode":"SfxEfx","pipe":"\\\\.\\pipe\\vxapo_test"}"#,
            ),
            (
                InstallProgressEvent::Retry {
                    from: "SfxEfx".into(),
                    to: "SfxMfx".into(),
                    reason: Some("score 1 < 3".into()),
                },
                r#"{"event":"retry","from":"SfxEfx","to":"SfxMfx","reason":"score 1 < 3"}"#,
            ),
            (
                InstallProgressEvent::Complete {
                    success: true,
                    mode: Some("SfxEfx".into()),
                    score: Some(3),
                    attempts: 2,
                    best_mode: None,
                    best_score: None,
                },
                r#"{"event":"complete","success":true,"mode":"SfxEfx","score":3,"attempts":2}"#,
            ),
            (
                InstallProgressEvent::Complete {
                    success: false,
                    mode: None,
                    score: None,
                    attempts: 3,
                    best_mode: Some("SfxEfx".into()),
                    best_score: Some(1),
                },
                r#"{"event":"complete","success":false,"attempts":3,"best_mode":"SfxEfx","best_score":1}"#,
            ),
        ];
        for (event, want) in cases {
            assert_eq!(serde_json::to_string(&event).unwrap(), want);
        }
    }
}

/// 安装进度事件（`--progress-file` 逐行 JSON，App 经 `install-progress` 事件转发）。
///
/// 与 `#[serde(tag = "event")]` 一一对应；诊断用的 `trace` 事件不属对外契约，
/// 由 CLI 侧按需直接拼 JSON。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "event", rename_all = "snake_case")]
#[ts(export)]
pub enum InstallProgressEvent {
    /// 阶段推进（pre-verify 步骤也上报，便于定位卡点）。
    Phase { name: String },
    /// 写入注册表安装配置。
    InstallWrite { mode: String },
    /// 音频服务动作。
    Service { action: ServiceAction },
    /// 建图测试（`pipe` 仅管道测试事件携带）。
    Test {
        mode: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        pipe: Option<String>,
    },
    /// 模式回退重试。
    Retry {
        from: String,
        to: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        reason: Option<String>,
    },
    /// 安装结束：成功携带 `mode`/`score`，失败携带 `best_mode`/`best_score`。
    Complete {
        success: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        mode: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        score: Option<u32>,
        attempts: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        best_mode: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        best_score: Option<u32>,
    },
}

/// 服务动作（`service` 事件的 `action`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ServiceAction {
    Stopping,
    Stopped,
    Starting,
    Running,
}

/// 旧 GUID 残留记录的配对命中来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum StaleMatchedBy {
    /// 端点历史属性（GUID 刷新前的老 GUID）。
    EndpointHistory,
    /// 老端点键读出的实例 ID。
    DeviceInstanceId,
    /// 记录键落盘的稳定身份。
    StoredIdentity,
    /// 硬件 ID 兜底。
    HardwareId,
}

/// 旧 GUID 残留记录相对当前端点的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum StaleTargetState {
    /// 命中端点但槽位/信息区不完整。
    MatchedPartial,
    /// 命中端点且状态健康。
    MatchedHealthy,
    /// 未命中任何活跃端点（只能清理）。
    Unmatched,
}

/// 旧 GUID 安装记录（`stale list --json` 元素）；字段与 driver `StaleInstall` 对齐。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StaleInstall {
    pub guid: String,
    pub device_instance_id: String,
    pub display_name: String,
    /// 命中来源；未命中为 `null`。
    pub matched_by: Option<StaleMatchedBy>,
    pub config_path: Option<String>,
    #[ts(type = "number")]
    pub config_mtime_ms: Option<u64>,
    pub snapshot_path: Option<String>,
    #[ts(type = "number")]
    pub snapshot_mtime_ms: Option<u64>,
    pub premix_slot: Option<String>,
    pub postmix_slot: Option<String>,
    pub inferred_mode: String,
    pub has_child_backup: bool,
    pub has_sysfx_backup: bool,
    pub target_guid: Option<String>,
    pub target_name: Option<String>,
    pub target_state: StaleTargetState,
}

/// 旧 GUID 迁移报告（`stale migrate --json`）；字段与 driver `MigrationReport` 对齐。
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MigrationReport {
    pub success: bool,
    pub target_guid: String,
    pub config_from: Option<String>,
    pub snapshot_from: Option<String>,
    pub config_migrated: bool,
    pub snapshot_migrated: bool,
    pub install_repaired: bool,
    pub removed_guids: Vec<String>,
    pub warnings: Vec<String>,
}
