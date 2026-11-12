mod draw;
mod gemfind;

#[cfg(target_os = "linux")]
mod hypr;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(windows)]
mod win32;
#[cfg(windows)]
mod win_icon;

use poe_core::AppState;
use std::sync::{Arc, Mutex};

pub fn spawn(state: Arc<Mutex<AppState>>) {
    std::thread::Builder::new()
        .name("poe-overlay".into())
        .spawn(move || {
            if let Err(err) = run(state) {
                eprintln!("overlay stopped: {err:#}");
            }
        })
        .ok();
}

fn run(state: Arc<Mutex<AppState>>) -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        linux::run(state)
    }
    #[cfg(windows)]
    {
        win32::run(state)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = state;
        anyhow::bail!("this operating system has no overlay backend");
    }
}

pub use draw::Painter;

#[cfg(target_os = "linux")]
pub fn cursor_pos() -> Option<(i32, i32)> {
    hypr::cursor()
}

#[cfg(target_os = "linux")]
pub fn game_focused() -> bool {
    hypr::game_window().is_some_and(|window| window.focused)
}

#[cfg(not(target_os = "linux"))]
pub fn cursor_pos() -> Option<(i32, i32)> {
    None
}

#[cfg(target_os = "linux")]
pub fn paste_into_game() -> anyhow::Result<()> {
    hypr::send_paste()
}

#[cfg(windows)]
pub fn paste_into_game() -> anyhow::Result<()> {
    win32::send_paste()
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn paste_into_game() -> anyhow::Result<()> {
    anyhow::bail!("vendor paste is wired for Hyprland and Windows");
}

#[cfg(target_os = "linux")]
pub fn send_chat(text: &str) -> anyhow::Result<()> {
    hypr::send_chat(text)
}

#[cfg(target_os = "linux")]
pub fn prepare_config_window() {
    hypr::prepare_config_window();
}

#[cfg(windows)]
pub fn prepare_config_window() {
    win32::prepare_process();
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn prepare_config_window() {}

#[cfg(windows)]
pub fn apply_app_icon() {
    win_icon::apply_to_titled_windows("RustyExile");
}

#[cfg(not(windows))]
pub fn apply_app_icon() {}

#[cfg(target_os = "linux")]
pub fn dismiss_config_window() {
    hypr::dismiss_config_window();
}

#[cfg(windows)]
pub fn dismiss_config_window() {
    win32::dismiss_config_window();
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn dismiss_config_window() {}

#[cfg(target_os = "linux")]
pub fn raise_config_window() {
    hypr::raise_config_window();
}

#[cfg(windows)]
pub fn raise_config_window() {
    win32::raise_config_window();
}

#[cfg(windows)]
pub use win32::SingleInstance;

#[cfg(windows)]
pub fn claim_single_instance() -> Option<SingleInstance> {
    win32::claim_single_instance()
}

#[cfg(windows)]
pub fn watch_single_instance(instance: SingleInstance, on_activate: impl Fn() + Send + 'static) {
    win32::watch_single_instance(instance, on_activate);
}

#[cfg(windows)]
pub fn game_is_open() -> bool {
    win32::game_is_open()
}

#[cfg(not(windows))]
pub fn game_is_open() -> bool {
    false
}

#[cfg(target_os = "linux")]
pub fn config_popup_origin() -> Option<(i32, i32)> {
    hypr::config_popup_origin()
}

#[cfg(not(target_os = "linux"))]
pub fn config_popup_origin() -> Option<(i32, i32)> {
    None
}

#[cfg(target_os = "linux")]
pub fn pause_for_screenshot() {
    hypr::pause_for_screenshot();
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn raise_config_window() {}

#[cfg(target_os = "linux")]
pub fn sync_config_bind(previous: &str, next: &str) {
    hypr::sync_config_bind(previous, next);
}

#[cfg(not(target_os = "linux"))]
pub fn sync_config_bind(_previous: &str, _next: &str) {}

#[cfg(windows)]
pub fn send_chat(text: &str) -> anyhow::Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(text.to_owned())?;
    drop(clipboard);
    win32::send_chat()
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn send_chat(_text: &str) -> anyhow::Result<()> {
    anyhow::bail!("chat hotkeys are wired for Hyprland and Windows");
}
