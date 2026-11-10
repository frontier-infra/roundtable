//! Roundtable desktop app: Tauri 2 shell around the Rust engine.

pub mod claude;
pub mod commands;
pub mod engine_install;
pub mod jebadiah;
pub mod menu;
pub mod update;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .menu(menu::build)
        .on_menu_event(|app, event| menu::handle(app, event.id().as_ref()))
        .manage(commands::AppState::default())
        .manage(update::Updates::default())
        .setup(|_app| {
            commands::refresh_engine_on_launch();
            commands::purge_history_trash();
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
            commands::providers_list,
            commands::providers_detect,
            commands::provider_models,
            commands::provider_add,
            commands::provider_remove,
            commands::cli_login,
            commands::claude_status,
            commands::claude_connect_app,
            commands::claude_connect_code,
            commands::claude_disable_plugin,
            commands::claude_disconnect,
            commands::coach_questions,
            commands::coach_brief,
            commands::suggest_panel,
            commands::attach_prepare,
            commands::ask_start,
            commands::ask_stop,
            commands::ask_retry,
            commands::head_test,
            commands::history_list,
            commands::history_get,
            commands::history_delete,
            commands::history_restore,
            commands::history_purge,
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
            jebadiah::jebadiah_local,
            update::update_check,
            update::update_download,
            update::update_on_quit,
            update::update_install,
        ])
        .build(tauri::generate_context!())
        .expect("error while running Roundtable")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                update::install_on_exit(&app.state::<update::Updates>());
            }
        });
}
