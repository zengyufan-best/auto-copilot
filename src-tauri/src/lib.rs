mod commands;
mod executor;
mod recorder;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            stop_flag: Arc::new(AtomicBool::new(false)),
            capture: Arc::new(recorder::CaptureState::new()),
        })
        .invoke_handler(tauri::generate_handler![
            commands::run_pipeline,
            commands::stop_pipeline,
            commands::start_recording,
            commands::stop_recording,
            commands::start_pick_position,
            commands::start_pick_key,
            commands::cancel_pick,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
