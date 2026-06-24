use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use tauri::{AppHandle, Emitter, State};

use crate::executor::{execute_pipeline, ExecutionStatus, Pipeline};

pub struct AppState {
    pub stop_flag: Arc<AtomicBool>,
    pub recording_flag: Arc<AtomicBool>,
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
    state.recording_flag.store(true, Ordering::Relaxed);
    crate::recorder::start_recording(app, state.recording_flag.clone());
    Ok(())
}

#[tauri::command]
pub fn stop_recording(state: State<'_, AppState>) -> Result<(), String> {
    state.recording_flag.store(false, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub fn get_cursor_position() -> Result<(i32, i32), String> {
    use enigo::{Enigo, Mouse, Settings};
    let enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    let (x, y) = enigo.location().map_err(|e| e.to_string())?;
    Ok((x, y))
}
