use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use tauri::{AppHandle, Emitter, State};

use crate::executor::{execute_pipeline, ExecutionStatus, Pipeline};
use crate::recorder::CaptureState;

pub struct AppState {
    pub stop_flag: Arc<AtomicBool>,
    pub capture: Arc<CaptureState>,
}

#[tauri::command]
pub fn run_pipeline(
    app: AppHandle,
    state: State<'_, AppState>,
    pipeline: Pipeline,
) -> Result<(), String> {
    let stop_flag = state.stop_flag.clone();
    stop_flag.store(false, Ordering::Relaxed);

    thread::spawn(move || {
        execute_pipeline(&pipeline, stop_flag.clone(), |status: ExecutionStatus| {
            let _ = app.emit("pipeline-status", &status);
        });

        let _ = app.emit(
            "pipeline-status",
            &ExecutionStatus {
                running: false,
                current_loop: 0,
                current_action: 0,
            },
        );
    });

    Ok(())
}

#[tauri::command]
pub fn stop_pipeline(state: State<'_, AppState>) -> Result<(), String> {
    state.stop_flag.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn start_recording(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.capture.recording.store(true, Ordering::Relaxed);
    crate::recorder::ensure_listener(app, state.capture.clone())
}

#[tauri::command]
pub fn stop_recording(state: State<'_, AppState>) -> Result<(), String> {
    state.capture.recording.store(false, Ordering::Relaxed);
    Ok(())
}

/// Arm a one-shot coordinate pick: the next mouse click anywhere on screen is
/// captured and emitted as a `picked-position` event, then disarmed.
#[tauri::command]
pub fn start_pick_position(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.capture.arm();
    state.capture.pick_position.store(true, Ordering::Relaxed);
    crate::recorder::ensure_listener(app, state.capture.clone())
}

/// Arm a one-shot key pick (Windows/Linux only). macOS keyboard capture is
/// unavailable (rdev segfaults on the tap thread), so we reject it up front.
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn start_pick_key(_app: AppHandle, _state: State<'_, AppState>) -> Result<(), String> {
    Err("macOS 暂不支持键位拾取,请在 Windows 上录制键盘,或手动填写按键。".to_string())
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub fn start_pick_key(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    state.capture.arm();
    state.capture.pick_key.store(true, Ordering::Relaxed);
    crate::recorder::ensure_listener(app, state.capture.clone())
}

/// Disarm any pending one-shot pick (e.g. the user closed the form).
#[tauri::command]
pub fn cancel_pick(state: State<'_, AppState>) -> Result<(), String> {
    state.capture.pick_position.store(false, Ordering::Relaxed);
    state.capture.pick_key.store(false, Ordering::Relaxed);
    Ok(())
}
