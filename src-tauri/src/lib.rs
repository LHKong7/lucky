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
            // Lucky app menu
            let quit_item = MenuItemBuilder::with_id("quit", "Quit Lucky")
                .accelerator("CmdOrCtrl+Q")
                .build(app)?;
            let app_submenu = SubmenuBuilder::new(app, "Lucky")
                .item(&quit_item)
                .build()?;

            // Settings menu (top-level)
            let settings_item = MenuItemBuilder::with_id("settings", "Open Settings")
                .accelerator("CmdOrCtrl+,")
                .build(app)?;
            let settings_submenu = SubmenuBuilder::new(app, "Settings")
                .item(&settings_item)
                .build()?;

            // History menu (top-level)
            let history_item = MenuItemBuilder::with_id("history", "View All Messages")
                .accelerator("CmdOrCtrl+H")
                .build(app)?;
            let history_submenu = SubmenuBuilder::new(app, "History")
                .item(&history_item)
                .build()?;

            let menu = MenuBuilder::new(app)
                .item(&app_submenu)
                .item(&settings_submenu)
                .item(&history_submenu)
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
