//! Tauri-шелл darmark: тонкая обёртка над md-core + владение состоянием документов.
//!
//! Рендер Markdown живёт в crate `md-core`; здесь — IPC, файловый ввод-вывод и
//! `DocumentStore` (D5: текст, ревизия, путь и кэш рендера принадлежат хосту).
//! Путь выбирает пользователь в нативном диалоге на фронтенде (модель Notepad++:
//! «белого списка» каталогов нет — диалог и есть согласие).

pub mod ai;
mod error;
pub mod mcp;
pub mod plugins;
mod state;

mod commands;
mod contract;
mod dev_plugin;
mod fs;
mod setup;

use std::sync::{Arc, Mutex};

use state::DocumentStore;

use commands::*;
pub use contract::{CommandInfo, PluginInfo, PluginStatus, PluginViewInfo};
pub use dev_plugin::maybe_run_debug_plugin;
pub(crate) use fs::MAX_FILE_SIZE;
pub use setup::PluginState;
use setup::{disable_browser_accelerator_keys, init_settings_dir, setup_plugins};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let store = Arc::new(Mutex::new(DocumentStore::default()));
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(store)
        .setup(|app| {
            use tauri::Manager;
            disable_browser_accelerator_keys(app);
            init_settings_dir();
            if let Err(e) = setup_plugins(app) {
                eprintln!("плагинная подсистема не запущена: {e}");
                app.manage(PluginState {
                    host: plugins::host::PluginHost::empty(),
                });
            }
            app.manage(ai::state::AiState::init());
            app.manage(mcp_core::server::McpServer::new(mcp::build_registry(
                app.handle(),
            )));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            new_document,
            open_document,
            update_document,
            undo_document,
            redo_document,
            render_document,
            document_snapshot,
            save_document,
            close_document,
            list_documents,
            activate_document,
            plugin_views,
            plugin_view_action,
            list_plugins,
            set_plugin_enabled,
            set_plugin_permissions,
            set_plugin_setting,
            reload_plugin,
            remove_plugin,
            run_plugin_command,
            open_devtools,
            ai::commands::ai_get_settings,
            ai::commands::ai_set_settings,
            ai::commands::ai_set_api_key,
            ai::commands::ai_list_skills,
            ai::commands::ai_chat,
            ai::commands::ai_history_load,
            ai::commands::ai_history_save,
            ai::commands::ai_history_clear,
            ai::commands::ai_test_connection,
            ai::commands::ai_render_markdown,
            ai::doc_harness::ai_diff,
            ai::doc_harness::ai_apply_edit,
            ai::plugin_harness::ai_validate_plugin,
            ai::plugin_harness::ai_create_plugin,
            ai::plugin_harness::ai_update_plugin,
            ai::plugin_harness::ai_delete_plugin
        ])
        .run(tauri::generate_context!())
        .expect("error while running darmark");
}
