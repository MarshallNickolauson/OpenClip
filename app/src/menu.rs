//! The macOS menu bar: just the app menu and Window. Everything else lives in the window itself.

use std::sync::Mutex;

use eframe::egui;
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

static CHOSEN: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub struct AppMenu {
    _menu: Menu,
}

pub fn take() -> Vec<String> {
    CHOSEN.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}

pub fn install(ctx: &egui::Context) -> Option<AppMenu> {
    let sep = PredefinedMenuItem::separator;
    let app = Submenu::new("OpenClip", true);
    // Quit goes through the app so unsaved edits can be offered a save first.
    let quit = MenuItem::with_id("app.quit", "Quit OpenClip", true, Some(Accelerator::new(Modifiers::META, Code::KeyQ)));
    app.append_items(&[
        &PredefinedMenuItem::about(Some("About OpenClip"), None),
        &sep(),
        &PredefinedMenuItem::hide(None),
        &PredefinedMenuItem::hide_others(None),
        &PredefinedMenuItem::show_all(None),
        &sep(),
        &quit,
    ])
    .ok()?;
    let window = Submenu::new("Window", true);
    window.append_items(&[&PredefinedMenuItem::minimize(None), &PredefinedMenuItem::maximize(None)]).ok()?;
    let menu = Menu::new();
    menu.append_items(&[&app, &window]).ok()?;
    #[cfg(target_os = "macos")]
    {
        menu.init_for_nsapp();
        window.set_as_windows_menu_for_nsapp();
    }
    let ctx = ctx.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Ok(mut chosen) = CHOSEN.lock() {
            chosen.push(event.id.0.clone());
        }
        ctx.request_repaint();
    }));
    Some(AppMenu { _menu: menu })
}
