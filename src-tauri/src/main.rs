#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod commands;
mod engine;
mod media;
mod native;
mod whisper;

use commands::AppState;
use tauri::{Emitter, Manager, TitleBarStyle, WebviewUrl, WebviewWindowBuilder};

fn main() {
    // `ClipCraft import <video>` / `ClipCraft export <project>` run headless.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::run(&args) {
        std::process::exit(code);
    }

    native::trim_edit_menu();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .on_window_event(|win, ev| {
            // closing the window asks the UI first, so unsaved edits are never dropped silently
            if let tauri::WindowEvent::CloseRequested { api, .. } = ev {
                api.prevent_close();
                let _ = win.emit("close-requested", ());
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_projects,
            commands::import_video,
            commands::cancel_import,
            commands::open_project,
            commands::save_project,
            commands::clip_path,
            commands::clip_captions,
            commands::thumbnail,
            commands::local_pick,
            commands::read_suggestions,
            commands::export_clips,
            commands::cancel_export,
            commands::reveal,
            commands::delete_project,
            commands::relink_source,
            commands::connect_claude_desktop,
            commands::download_model,
            commands::mcp_seen,
            commands::media_url,
            commands::exit_app,
        ])
        .setup(|app| {
            let h = app.handle().clone();
            WebviewWindowBuilder::new(&h, "main", WebviewUrl::App("index.html".into()))
                .title("ClipCraft")
                .inner_size(1440.0, 900.0)
                .min_inner_size(1120.0, 720.0)
                .title_bar_style(TitleBarStyle::Overlay)
                .hidden_title(true)
                .accept_first_mouse(true)
                .build()?;
            commands::start_watcher(h);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building ClipCraft")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = &event {
                if !commands::ALLOW_EXIT.load(std::sync::atomic::Ordering::SeqCst) {
                    api.prevent_exit();
                    let _ = app.emit("close-requested", ());
                }
            }
            if let tauri::RunEvent::Reopen { .. } = event {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        });
}
