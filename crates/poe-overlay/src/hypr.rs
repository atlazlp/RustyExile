use anyhow::Context;
use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct GameWindow {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub focused: bool,
    pub monitor: i64,
    pub address: String,
    #[allow(dead_code)]
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct Monitor {
    pub id: i64,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    #[allow(dead_code)]
    pub name: String,
}

pub fn command(request: &str) -> anyhow::Result<String> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").context("HYPRLAND_INSTANCE_SIGNATURE")?;
    let runtime = std::env::var("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR")?;
    let path = PathBuf::from(runtime).join("hypr").join(sig).join(".socket.sock");
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(std::time::Duration::from_millis(300)))?;
    stream.set_write_timeout(Some(std::time::Duration::from_millis(300)))?;
    stream.write_all(request.as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut out = String::new();
    stream.read_to_string(&mut out)?;
    Ok(out)
}

pub fn game_window() -> Option<GameWindow> {
    let clients = command("j/clients").ok()?;
    let active = command("j/activewindow").ok().unwrap_or_default();
    let active_addr = serde_json::from_str::<Value>(&active)
        .ok()
        .and_then(|v| v.get("address").and_then(|a| a.as_str()).map(|s| s.to_string()));
    let list: Vec<Value> = serde_json::from_str(&clients).ok()?;
    list.into_iter().find_map(|client| {
        let class = client.get("class").and_then(|v| v.as_str()).unwrap_or("");
        let title = client.get("title").and_then(|v| v.as_str()).unwrap_or("");
        if !is_poe_window(class, title) {
            return None;
        }
        let at = client.get("at").and_then(|v| v.as_array())?;
        let size = client.get("size").and_then(|v| v.as_array())?;
        let address = client.get("address").and_then(|v| v.as_str()).unwrap_or("");
        Some(GameWindow {
            x: at.first()?.as_i64()? as i32,
            y: at.get(1)?.as_i64()? as i32,
            w: size.first()?.as_u64()? as u32,
            h: size.get(1)?.as_u64()? as u32,
            focused: active_addr.as_deref() == Some(address),
            monitor: client.get("monitor").and_then(|v| v.as_i64()).unwrap_or(0),
            address: address.to_string(),
            title: title.to_string(),
        })
    })
}

pub fn monitors() -> Vec<Monitor> {
    let Ok(text) = command("j/monitors") else {
        return Vec::new();
    };
    let Ok(list) = serde_json::from_str::<Vec<Value>>(&text) else {
        return Vec::new();
    };
    list.into_iter()
        .filter_map(|m| {
            Some(Monitor {
                id: m.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
                x: m.get("x").and_then(|v| v.as_i64())? as i32,
                y: m.get("y").and_then(|v| v.as_i64())? as i32,
                w: m.get("width").and_then(|v| v.as_i64())? as i32,
                h: m.get("height").and_then(|v| v.as_i64())? as i32,
                name: m.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            })
        })
        .collect()
}

pub fn cursor() -> Option<(i32, i32)> {
    let text = command("cursorpos").ok()?;
    let (x, y) = text.trim().split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

pub fn send_paste() -> anyhow::Result<()> {
    let window = game_selector();
    send_shortcut("CTRL", "A", &window)?;
    std::thread::sleep(std::time::Duration::from_millis(40));
    send_shortcut("CTRL", "V", &window)?;
    Ok(())
}

pub fn send_chat() -> anyhow::Result<()> {
    let Some(window) = game_window() else {
        anyhow::bail!("Path of Exile is not running");
    };
    let selector = selector_for(&window.address);
    if !selector.is_empty() {
        let _ = command(&format!("dispatch hl.dsp.focus({{ window = \"{selector}\" }})"));
    }
    std::thread::sleep(std::time::Duration::from_millis(90));
    send_shortcut("", "return", &selector)?;
    std::thread::sleep(std::time::Duration::from_millis(70));
    send_shortcut("CTRL", "A", &selector)?;
    std::thread::sleep(std::time::Duration::from_millis(30));
    send_shortcut("CTRL", "V", &selector)?;
    std::thread::sleep(std::time::Duration::from_millis(40));
    send_shortcut("", "return", &selector)?;
    Ok(())
}

fn game_selector() -> String {
    game_window().map(|window| selector_for(&window.address)).unwrap_or_default()
}

fn selector_for(address: &str) -> String {
    if address.is_empty() {
        String::new()
    } else if address.starts_with("address:") {
        address.to_string()
    } else {
        format!("address:{address}")
    }
}

fn send_shortcut(mods: &str, key: &str, window: &str) -> anyhow::Result<()> {
    let target = if window.is_empty() {
        String::new()
    } else {
        format!(", window = \"{window}\"")
    };
    let out = command(&format!(
        "dispatch hl.dsp.send_shortcut({{ mods = \"{mods}\", key = \"{key}\"{target} }})"
    ))?;
    if out.contains("error:") || out.contains("not found") {
        anyhow::bail!(out.trim().to_string());
    }
    Ok(())
}

fn is_poe_window(class: &str, title: &str) -> bool {
    let class_l = class.to_ascii_lowercase();
    let title_l = title.to_ascii_lowercase();
    if title_l.contains("path of exile 2") || class_l.contains("path of exile 2") {
        return false;
    }
    title_l.contains("path of exile")
        || class_l.contains("pathofexile")
        || class_l.contains("steam_app_238960")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_poe1_titles_only() {
        assert!(is_poe_window("steam_app_238960", "Path of Exile"));
        assert!(!is_poe_window("steam_app_2694490", "Path of Exile 2"));
    }
}
