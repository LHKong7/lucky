pub mod borderless;
pub mod settings;

use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::webview::WebviewWindowBuilder;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            settings::load_settings,
            settings::save_settings,
            settings::pick_directory,
            settings::chat_message,
            settings::continue_chat,
            settings::save_session,
            settings::load_session,
        ])
        .setup(|app| {
            let settings_item = MenuItemBuilder::with_id("settings", "Settings...")
                .accelerator("CmdOrCtrl+,")
                .build(app)?;
            let history_item = MenuItemBuilder::with_id("history", "Chat History")
                .accelerator("CmdOrCtrl+H")
                .build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit Lucky")
                .accelerator("CmdOrCtrl+Q")
                .build(app)?;

            let app_submenu = SubmenuBuilder::new(app, "Lucky")
                .item(&settings_item)
                .item(&history_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let menu = MenuBuilder::new(app)
                .item(&app_submenu)
                .build()?;

            app.set_menu(menu)?;

            app.on_menu_event(move |app_handle, event| {
                match event.id().as_ref() {
                    "settings" => {
                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = window.emit("menu-settings", ());
                        }
                    }
                    "history" => {
                        // Focus existing or create new history window
                        if let Some(win) = app_handle.get_webview_window("history") {
                            let _ = win.set_focus();
                        } else {
                            let _ = WebviewWindowBuilder::new(
                                app_handle,
                                "history",
                                tauri::WebviewUrl::App("index.html?history".into()),
                            )
                            .title("Chat History")
                            .inner_size(420.0, 500.0)
                            .resizable(true)
                            .center()
                            .build();
                        }
                    }
                    "quit" => {
                        app_handle.exit(0);
                    }
                    _ => {}
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
