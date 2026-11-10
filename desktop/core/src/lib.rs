//! Roundtable desktop app: Tauri 2 shell around the Rust engine.

pub mod claude;
pub mod commands;
pub mod engine_install;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(commands::AppState::default())
        .setup(|_app| {
            commands::refresh_engine_on_launch();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::engine_status,
            commands::engine_setup,
            commands::keys_status,
            commands::key_save,
            commands::key_remove,
            commands::keys_import,
            commands::doctor,
            commands::clis_detect,
            commands::cli_login,
            commands::claude_status,
            commands::claude_connect_app,
            commands::claude_connect_code,
            commands::claude_disable_plugin,
            commands::claude_disconnect,
            commands::ask_start,
            commands::ask_stop,
            commands::ask_retry,
            commands::head_test,
            commands::history_list,
            commands::history_get,
            commands::history_delete,
            commands::history_clear,
            commands::personas_list,
            commands::persona_save,
            commands::persona_duplicate,
            commands::persona_delete,
            commands::personas_export,
            commands::personas_import,
            commands::settings_get,
            commands::settings_set,
            commands::save_text,
            commands::terminal_command,
            commands::diagnostics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Roundtable");
}
