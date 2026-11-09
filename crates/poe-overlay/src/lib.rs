mod draw;

#[cfg(target_os = "linux")]
mod hypr;
#[cfg(target_os = "linux")]
mod linux;

#[cfg(windows)]
mod win32;

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
pub fn send_chat() -> anyhow::Result<()> {
    hypr::send_chat()
}

#[cfg(windows)]
pub fn send_chat() -> anyhow::Result<()> {
    win32::send_chat()
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn send_chat() -> anyhow::Result<()> {
    anyhow::bail!("chat hotkeys are wired for Hyprland and Windows");
}
