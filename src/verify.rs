//! vxapo-cli/src/verify.rs — `install --verify`：写入 → 整服重启 → 管道验证 → 计分重试
//!
//! 流程（EAPO DeviceTestThread 同构）：
//! 每模式：`write_install_config`（纯注册表写入）→ SCM 停/启 AudioSrv（带依赖服务
//! 与轮询）→ 建命名管道 + 写 `HKLM\SOFTWARE\VxAPO\DeviceTestPipeName` →
//! `IMMDevice→IAudioClient` 触发 audiodg 建图 → driver DLL Initialize 回连管道上报
//! 阶段 → 计分；未满分则覆写下一模式重试；全部失败保留最高分配置并确保服务运行。
//!
//! 事件输出：每行一条 JSON，同时打 stdout 与 `--progress-file`（App 提权路径读取）。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::json;
use vxapo_driver::install::device::slots::{ChildApoKind, InstallMode, read_child_apo_guid};
use vxapo_driver::install::selector::operation::{
    InstallConfig, find_endpoint_path, write_install_config,
};
use vxapo_driver::sys::registry::RegKey;
use windows::Win32::Foundation::{HANDLE, HLOCAL, INVALID_HANDLE_VALUE, LocalFree};
use windows::Win32::Storage::FileSystem::{ReadFile, PIPE_ACCESS_INBOUND};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE, PIPE_TYPE_MESSAGE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::System::Registry::HKEY_LOCAL_MACHINE;
use windows::core::HSTRING;

use crate::commands::DeviceRef;
use crate::i18n::tr;

/// 验证管道名（固定，与 driver `object/apo/test_pipe.rs` 约定一致）。
const PIPE_NAME: &str = "VxAPODeviceTest";
/// 全局键：HKLM\SOFTWARE\VxAPO\DeviceTestPipeName。
const TEST_VALUE: &str = "DeviceTestPipeName";
const TEST_KEY: &str = r"SOFTWARE\VxAPO";

/// 阶段超时（秒）——安装必须快速给出结果，失败也别让用户等。
const STOP_TIMEOUT_SECS: u32 = 3;
const START_TIMEOUT_SECS: u32 = 5;
const PIPE_WAIT_SECS: u64 = 2;
/// 服务 RUNNING 后等待音频引擎就绪的静默时间（毫秒），降低触发时阻塞概率。
const POST_SERVICE_SETTLE_MS: u64 = 300;
/// 全局看门狗（秒）：无论任何线程/COM 调用卡死，进程都在 20s 内强制终止。
const GLOBAL_WATCHDOG_SECS: u64 = 20;

/// 事件输出：stdout 一行 + progress 文件追加一行。
pub(crate) struct EventSink<'a> {
    progress_file: Option<&'a Path>,
}

impl<'a> EventSink<'a> {
    pub(crate) fn new(progress_file: Option<&'a Path>) -> Self {
        Self { progress_file }
    }

    pub(crate) fn emit(&mut self, event: serde_json::Value) {
        let line = event.to_string();
        println!("{line}");
        if let Some(p) = self.progress_file {
            if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(p) {
                let _ = writeln!(f, "{line}");
                let _ = f.flush();
            }
        }
    }
}

/// 阶段进度事件（pre-verify 步骤也上报，便于定位卡点；仅在有 progress 文件时输出）。
pub(crate) fn emit_phase(progress_file: Option<&Path>, name: &str) {
    let line = json!({"event": "phase", "name": name}).to_string();
    println!("{line}");
    if let Some(p) = progress_file {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{line}");
            let _ = f.flush();
        }
    }
}

/// 模式名（事件字段用，用户端展示大写）。
fn mode_str(m: InstallMode) -> &'static str {
    match m {
        InstallMode::LfxGfx => "LFX_GFX",
        InstallMode::SfxMfx => "SFX_MFX",
        InstallMode::SfxEfx => "SFX_EFX",
    }
}

/// 模式尝试顺序：preferred 第一，其余按 EAPO 回退序 [SfxEfx, SfxMfx, LfxGfx]。
fn mode_order(preferred: InstallMode) -> Vec<InstallMode> {
    let mut v = vec![preferred];
    for m in [InstallMode::SfxEfx, InstallMode::SfxMfx, InstallMode::LfxGfx] {
        if m != preferred {
            v.push(m);
        }
    }
    v
}

/// 管道上报集合。
#[derive(Default)]
struct PipeReport {
    premix_init: bool,
    postmix_init: bool,
    child_premix: bool,
    child_postmix: bool,
    messages: u32,
}

fn parse_pipe_message(line: &str, report: &mut PipeReport) {
    report.messages += 1;
    let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
        return;
    };
    let stage = v.get("stage").and_then(|s| s.as_str()).unwrap_or("");
    let phase = v.get("phase").and_then(|s| s.as_str()).unwrap_or("");
    match (stage, phase) {
        ("premix", "initialize") => report.premix_init = true,
        ("postmix", "initialize") => report.postmix_init = true,
        ("premix", "child_apo") => report.child_premix = true,
        ("postmix", "child_apo") => report.child_postmix = true,
        _ => {}
    }
}

/// 计分：premix init 20 + child_premix 2 + postmix init 10 + child_postmix 1。
/// 满分按设备类型：render=33、capture=22（capture 不装 PostMix）。
/// 子 APO 判据：期望存在（注册表有 PreMixChild/PostMixChild）且收到 child_apo，
/// 或期望为空视为通过。
fn score_of(
    report: &PipeReport,
    is_capture: bool,
    expected_premix: bool,
    expected_postmix: bool,
) -> u32 {
    let mut s = 0u32;
    if report.premix_init {
        s += 20;
    }
    if !expected_premix || report.child_premix {
        s += 2;
    }
    if !is_capture {
        if report.postmix_init {
            s += 10;
        }
        if !expected_postmix || report.child_postmix {
            s += 1;
        }
    }
    s
}

/// `install --verify` 主流程。成功返回 Ok；全部模式未通过返回 Err（退出码 1）。
pub(crate) fn install_verify(
    dev: &DeviceRef,
    config: &InstallConfig,
    timeout_secs: u64,
    progress_file: Option<&Path>,
) -> Result<(), String> {
    // 全局看门狗：无论任何线程/COM 调用卡死，进程都在 20s 内强制终止。
    // 用 abort()（SIGABRT）而非 exit()，跳过 CRT/atexit 清理，避免清理本身被
    // 其他阻塞线程卡死。
    let _watchdog = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(GLOBAL_WATCHDOG_SECS));
        let _ = std::process::abort();
    });

    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let is_capture = find_endpoint_path(&dev.guid)
        .map(|p| p.contains("Capture"))
        .unwrap_or(false);
    let max_score: u32 = if is_capture { 22 } else { 33 };
    let mut sink = EventSink::new(progress_file);

    let modes = mode_order(config.install_mode);
    let mut best_score: u32 = 0;
    let mut best_mode: Option<InstallMode> = None;
    let mut attempts: u32 = 0;

    for (idx, mode) in modes.iter().enumerate() {
        if Instant::now() > deadline {
            break;
        }
        attempts += 1;
        let mut mode_config = config.clone();
        mode_config.install_mode = *mode;
        let mode_name = mode_str(*mode);

        // 1. 纯注册表写入（覆盖安装天然安全，无需先 uninstall）。
        sink.emit(json!({"event": "install_write", "mode": mode_name}));
        write_install_config(&dev.guid, &dev.name, &dev.connection, &mode_config)
            .map_err(|e| format!("写入安装配置失败：{e}"))?;

        // 2. 停 AudioSrv（含依赖服务）。
        sink.emit(json!({"event": "service", "action": "stopping"}));
        vxapo_driver::install::audiodg::stop_audio_service_with_dependents(STOP_TIMEOUT_SECS)
            .map_err(|e| format!("停止音频服务失败：{e}"))?;
        sink.emit(json!({"event": "service", "action": "stopped"}));

        // 3. 启动 AudioSrv（含依赖服务，轮询 RUNNING）。
        sink.emit(json!({"event": "service", "action": "starting"}));
        vxapo_driver::install::audiodg::start_audio_service_with_dependents(START_TIMEOUT_SECS)
            .map_err(|e| format!("启动音频服务失败：{e}"))?;
        sink.emit(json!({"event": "service", "action": "running"}));
        // SCM 报 RUNNING 不代表音频引擎已就绪：先静默等待，避免后续
        // IMMDevice/IAudioClient 激活在引擎启动窗口内无限期阻塞。
        std::thread::sleep(Duration::from_millis(POST_SERVICE_SETTLE_MS));

        // 4. 管道验证（建管道 → 触发 → 收集 → 清理）。
        let expected_premix = read_child_apo_guid(&dev.guid, ChildApoKind::PreMix).is_some();
        let expected_postmix = read_child_apo_guid(&dev.guid, ChildApoKind::PostMix).is_some();
        let report = run_pipe_verify(
            &dev.guid,
            is_capture,
            expected_premix,
            expected_postmix,
            mode_name,
            &mut sink,
        )?;

        // 5. 计分与事件。
        let score = score_of(&report, is_capture, expected_premix, expected_postmix);
        // 用户端只接收 mode；计分仅内部用于模式重试与 complete 事件。
        sink.emit(json!({"event": "test", "mode": mode_name}));
        if score > best_score {
            best_score = score;
            best_mode = Some(*mode);
        }

        if score == max_score {
            // 成功后定向重启端点设备：让新 APO 配置立即生效，
            // 避免用户需要在系统声音设置里来回切换设备才恢复正常播放。
            let _ = vxapo_driver::install::audiodg::restart_endpoint_device(&dev.guid, is_capture);
            sink.emit(json!({
                "event": "complete", "success": true, "mode": mode_name,
                "score": score, "attempts": attempts,
            }));
            return Ok(());
        }
        if let Some(next) = modes.get(idx + 1) {
            sink.emit(json!({
                "event": "retry", "from": mode_name, "to": mode_str(*next),
                "reason": format!("score {score} < {max_score}"),
            }));
        }
    }

    // 全部失败：**回滚注册表**（uninstall_endpoint 清槽位/信息区/恢复 sysfx），
    // 设备不残留"已安装"状态；确保音频服务运行后报告失败。
    let _ = vxapo_driver::install::selector::operation::uninstall_endpoint(&dev.guid);
    let _ = vxapo_driver::install::audiodg::start_audio_service_with_dependents(START_TIMEOUT_SECS);
    sink.emit(json!({
        "event": "complete", "success": false,
        "best_mode": best_mode.map(mode_str),
        "best_score": best_score, "attempts": attempts,
    }));
    Err(tr(
        "所有安装模式均未通过验证",
        "All install modes failed verification",
    )
    .to_string())
}

/// 单模式管道验证：建管道 → 写注册表 → 触发 APO 加载 → 收集上报 → 清理。
fn run_pipe_verify(
    guid: &str,
    is_capture: bool,
    _expected_premix: bool,
    _expected_postmix: bool,
    mode_name: &str,
    sink: &mut EventSink,
) -> Result<PipeReport, String> {
    let full_path = format!(r"\\.\pipe\{PIPE_NAME}");
    let handle = create_pipe_server(&full_path)?;

    write_test_pipe_name(PIPE_NAME)?;
    sink.emit(json!({"event": "test", "pipe": PIPE_NAME, "mode": mode_name}));

    // 服务端线程：接受多个客户端连接（每个 APO 实例连一次、发一行即关），
    // 消息经 channel 送回主线程。
    let (tx, rx) = mpsc::channel::<String>();
    let trace_file = sink.progress_file.map(|p| p.to_path_buf());
    // HANDLE 不是 Send，线程内以裸指针地址重建（CLI 进程内有效）。
    let server_handle_ptr = handle.0 as usize;
    let server_trace = trace_file.clone();
    let _server = std::thread::spawn(move || {
        let server_handle = HANDLE(server_handle_ptr as *mut core::ffi::c_void);
        let mut buf: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            let connected = unsafe { ConnectNamedPipe(server_handle, None) };
            if connected.is_err() {
                // ERROR_PIPE_CONNECTED=535：客户端在 Connect 前已连上，视为成功。
                let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32;
                if code != 535 {
                    trace_emit(&server_trace, json!({"event":"trace","step":"server_exit","err":code}));
                    break;
                }
            }
            buf.clear();
            loop {
                let mut read = 0u32;
                if unsafe { ReadFile(server_handle, Some(&mut chunk), Some(&mut read), None) }.is_err()
                {
                    break;
                }
                if read == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..read as usize]);
                while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                    let line: Vec<u8> = buf.drain(..=pos).collect();
                    let s = String::from_utf8_lossy(&line).trim().to_string();
                    if !s.is_empty() {
                        trace_emit(&server_trace, json!({"event":"trace","step":"server_msg","line":s}));
                        let _ = tx.send(s);
                    }
                }
            }
            let _ = unsafe { DisconnectNamedPipe(server_handle) };
        }
    });
    trace_emit(&trace_file, json!({"event": "trace", "step": "server_spawned"}));

    // 触发 audiodg 建图。慢设备上 COM 调用可能耗时 5–8s，不再设触发级超时
    // （避免误杀）；触发线程 panic 时 recv 返回 Disconnected，按"无消息"继续。
    // 20s 全局看门狗仍作为最终兜底。
    let (trigger_tx, trigger_rx) = mpsc::channel::<Result<(), String>>();
    let trigger_guid = guid.to_string();
    let trigger_trace = trace_file.clone();
    let trigger_thread = std::thread::spawn(move || {
        trace_emit(&trigger_trace, json!({"event": "trace", "step": "trigger_start"}));
        let r = trigger_apo_load(&trigger_guid, is_capture);
        match &r {
            Ok(()) => trace_emit(&trigger_trace, json!({"event": "trace", "step": "trigger_ok"})),
            Err(e) => trace_emit(
                &trigger_trace,
                json!({"event": "trace", "step": "trigger_err", "err": e}),
            ),
        }
        let _ = trigger_tx.send(r);
    });
    trace_emit(&trace_file, json!({"event": "trace", "step": "trigger_spawned"}));
    let _ = trigger_rx.recv();
    trace_emit(&trace_file, json!({"event": "trace", "step": "trigger_wait_done"}));
    let _ = trigger_thread.join();

    // 收集上报直到全部预期阶段到齐或超时。
    trace_emit(&trace_file, json!({"event": "trace", "step": "collect_start"}));
    let mut report = PipeReport::default();
    // 固定迭代次数 + 短超时（200ms × PIPE_WAIT_SECS*5），绝对有界。
    for _ in 0..(PIPE_WAIT_SECS * 5) {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                trace_emit(&trace_file, json!({"event":"trace","step":"pipe_msg","line":line}));
                parse_pipe_message(&line, &mut report);
            }
            Err(_) => break,
        }
    }
    trace_emit(
        &trace_file,
        json!({"event": "trace", "step": "collect_done", "messages": report.messages}),
    );

    // 清理：删注册表值、关管道（服务端线程随之退出）。
    clear_test_pipe_name();
    // 注意：**不能从主线程 CloseHandle(pipe)**——服务端线程正阻塞在
    // ConnectNamedPipe 上，CloseHandle 会一直等该等待完成（导致主线程永久卡死，
    // 只能靠全局看门狗 abort）。进程在 main 返回时由系统回收所有句柄，
    // 这里直接放行即可。
    trace_emit(&trace_file, json!({"event": "trace", "step": "cleanup_done"}));
    // 注意：**不能 join 服务端线程**——它阻塞在 ConnectNamedPipe 等待 APO 连接，
    // 主线程 CloseHandle 无法可靠唤醒该等待；进程在 main 返回时结束所有线程，
    // 直接放行即可（阻塞线程不会阻止 Rust 进程退出）。
    Ok(report)
}

/// 步骤追踪（调试卡点）：与事件同写到 progress 文件 + stdout，App 忽略该事件类型。
fn trace_emit(progress_file: &Option<PathBuf>, event: serde_json::Value) {
    let line = event.to_string();
    println!("{line}");
    if let Some(p) = progress_file {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(p) {
            let _ = writeln!(f, "{line}");
            let _ = f.flush();
        }
    }
}

/// 创建命名管道服务端（DACL：SYSTEM + Administrators + Everyone）。
///
/// 实测 audiodg 的访问身份对不上 SYSTEM/Administrators ACE（CreateFileW 报
/// ERROR_ACCESS_DENIED=5），EAPO 的验证管道同样允许 Everyone；验证管道仅存活
/// 数秒且名称固定，放开 Everyone 可写是安全的。
fn create_pipe_server(full_path: &str) -> Result<windows::Win32::Foundation::HANDLE, String> {
    // SAFETY: 无前置条件；SD 由 LocalFree 回收。
    let mut sd = PSECURITY_DESCRIPTOR(std::ptr::null_mut());
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            &HSTRING::from("D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;WD)"),
            1,
            &mut sd,
            None,
        )
    }
    .map_err(|e| format!("安全描述符转换失败：{e}"))?;

    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd.0,
        bInheritHandle: false.into(),
    };

    // SAFETY: full_path 为合法管道名；sa 持有有效 SD。
    let handle = unsafe {
        CreateNamedPipeW(
            &HSTRING::from(full_path),
            PIPE_ACCESS_INBOUND,
            PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT,
            PIPE_UNLIMITED_INSTANCES,
            0,
            4096,
            0,
            Some(&sa),
        )
    };
    // SD 生命周期到此结束（CreateNamedPipeW 已复制）。
    unsafe {
        let _ = LocalFree(Some(HLOCAL(sd.0)));
    }
    if handle == INVALID_HANDLE_VALUE {
        return Err(tr("创建验证管道失败", "Failed to create verification pipe").to_string());
    }
    Ok(handle)
}

/// 触发指定端点 APO 建图：IMMDevice → IAudioClient → GetMixFormat → Initialize。
/// 仅 Initialize、不启动/停止流（对齐 EAPO testAPOInstallation，避免干扰设备状态）。
/// E_PENDING / AUDCLNT_E_DEVICE_INVALIDATED 等瞬时错误重试 5×500ms。
fn trigger_apo_load(guid: &str, is_capture: bool) -> Result<(), String> {
    use windows::Win32::Media::Audio::{
        AUDCLNT_E_DEVICE_INVALIDATED, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_NOPERSIST,
        IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    };

    let flow = if is_capture { "0.0.1.00000000" } else { "0.0.0.00000000" };
    let id = format!("{{{flow}}}.{guid}");
    let mut last_err = tr("触发 APO 加载失败", "APO load trigger failed").to_string();

    for _ in 0..5 {
        // SAFETY: COM 初始化无前置条件；S_OK/S_FALSE 均合法。
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let result = (|| -> windows::core::Result<()> {
            // SAFETY: MMDeviceEnumerator 为有效 COM 类；GetDevice 需要合法端点 id。
            let enumerator: IMMDeviceEnumerator =
                unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }?;
            let device = unsafe { enumerator.GetDevice(&HSTRING::from(&id)) }?;
            let client: IAudioClient = unsafe { device.Activate(CLSCTX_ALL, None) }?;
            let format = unsafe { client.GetMixFormat() }?;
            let hr = unsafe {
                client.Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_NOPERSIST,
                    1_000_000,
                    0,
                    format,
                    None,
                )
            };
            if hr.is_err() {
                // SAFETY: format 由 GetMixFormat 分配，须 CoTaskMemFree。
                unsafe {
                    CoTaskMemFree(Some(format as *const _));
                }
                return hr;
            }
            // SAFETY: format 由 GetMixFormat 分配，须 CoTaskMemFree。
            unsafe {
                CoTaskMemFree(Some(format as *const _));
            }
            Ok(())
        })();

        match result {
            Ok(()) => return Ok(()),
            Err(e) => {
                let code = e.code().0 as u32;
                last_err = format!("{e}");
                if code == AUDCLNT_E_DEVICE_INVALIDATED.0 as u32 || code == 0x8000_000A {
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
    Err(last_err)
}

/// 写入 DeviceTestPipeName（HKLM\SOFTWARE\VxAPO）。
fn write_test_pipe_name(name: &str) -> Result<(), String> {
    let key = RegKey::create(HKEY_LOCAL_MACHINE, TEST_KEY)
        .map_err(|e| format!("打开 VxAPO 注册表键失败：{e}"))?;
    key.write_sz(TEST_VALUE, name)
        .map_err(|e| format!("写入 {TEST_VALUE} 失败：{e}"))
}

/// 清理 DeviceTestPipeName（幂等，不存在不算错误）。
fn clear_test_pipe_name() {
    if let Ok(key) = RegKey::open(HKEY_LOCAL_MACHINE, TEST_KEY) {
        let _ = key.delete_value(TEST_VALUE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_order_preferred_first_with_eapo_fallback() {
        assert_eq!(
            mode_order(InstallMode::SfxMfx),
            vec![InstallMode::SfxMfx, InstallMode::SfxEfx, InstallMode::LfxGfx]
        );
        assert_eq!(
            mode_order(InstallMode::LfxGfx),
            vec![InstallMode::LfxGfx, InstallMode::SfxEfx, InstallMode::SfxMfx]
        );
        assert_eq!(
            mode_order(InstallMode::SfxEfx),
            vec![InstallMode::SfxEfx, InstallMode::SfxMfx, InstallMode::LfxGfx]
        );
    }

    #[test]
    fn render_full_score_is_33() {
        let report = PipeReport {
            premix_init: true,
            postmix_init: true,
            child_premix: true,
            child_postmix: true,
            messages: 4,
        };
        assert_eq!(score_of(&report, false, true, true), 33);
    }

    #[test]
    fn capture_full_score_is_22_and_postmix_ignored() {
        let report = PipeReport {
            premix_init: true,
            postmix_init: true,
            child_premix: true,
            child_postmix: true,
            messages: 4,
        };
        assert_eq!(score_of(&report, true, true, true), 22);
    }

    #[test]
    fn clean_install_without_original_apo_scores_full() {
        // 无原始 APO → 期望为空 → child 视为通过，干净安装也拿满分。
        let report = PipeReport {
            premix_init: true,
            postmix_init: true,
            child_premix: false,
            child_postmix: false,
            messages: 2,
        };
        assert_eq!(score_of(&report, false, false, false), 33);
    }

    #[test]
    fn missing_expected_child_scores_lower() {
        let report = PipeReport {
            premix_init: true,
            postmix_init: true,
            child_premix: false,
            child_postmix: true,
            messages: 3,
        };
        // 期望 PreMix 子 APO 但未创建 → 缺 +2。
        assert_eq!(score_of(&report, false, true, true), 31);
    }

    #[test]
    fn parse_pipe_message_sets_flags() {
        let mut r = PipeReport::default();
        parse_pipe_message(
            r#"{"deviceGuid":"{x}","stage":"premix","phase":"initialize"}"#,
            &mut r,
        );
        parse_pipe_message(
            r#"{"deviceGuid":"{x}","stage":"postmix","phase":"child_apo"}"#,
            &mut r,
        );
        assert!(r.premix_init);
        assert!(r.child_postmix);
        assert!(!r.postmix_init);
        assert!(!r.child_premix);
    }

    #[test]
    fn mode_str_matches_event_contract() {
        assert_eq!(mode_str(InstallMode::LfxGfx), "LFX_GFX");
        assert_eq!(mode_str(InstallMode::SfxMfx), "SFX_MFX");
        assert_eq!(mode_str(InstallMode::SfxEfx), "SFX_EFX");
    }
}
