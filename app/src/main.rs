#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod cli;
mod editor;
mod engine;
mod home;
mod icons;
mod inspector;
mod menu;
mod modals;
mod native;
mod player;
mod state;
mod theme;
mod timeline;
mod whisper;

use eframe::egui;

fn main() -> eframe::Result {
    // `OpenClip import <video>` / `OpenClip export <project>` run headless.
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::run(&args) {
        std::process::exit(code);
    }

    let mut viewport = egui::ViewportBuilder::default();
    // Without this the Dock shows eframe's default icon instead of the bundle's.
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../icons/icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport: viewport
            .with_title("OpenClip")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1120.0, 720.0])
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("OpenClip", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
