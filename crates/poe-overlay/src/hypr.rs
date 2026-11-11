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
    #[allow(dead_code)]
    pub focused: bool,
    pub monitor: i64,
    pub address: String,
    pub workspace: String,
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
    let sig =
        std::env::var("HYPRLAND_INSTANCE_SIGNATURE").context("HYPRLAND_INSTANCE_SIGNATURE")?;
    let runtime = std::env::var("XDG_RUNTIME_DIR").context("XDG_RUNTIME_DIR")?;
    let path = PathBuf::from(runtime)
        .join("hypr")
        .join(sig)
        .join(".socket.sock");
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
    let active_addr = serde_json::from_str::<Value>(&active).ok().and_then(|v| {
        v.get("address")
            .and_then(|a| a.as_str())
            .map(|s| s.to_string())
    });
    let list: Vec<Value> = serde_json::from_str(&clients).ok()?;
    let mut best: Option<(u8, GameWindow)> = None;
    for client in list {
        let class = client.get("class").and_then(|v| v.as_str()).unwrap_or("");
        let title = client.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let Some(rank) = poe_window_rank(class, title) else {
            continue;
        };
        if best.as_ref().is_some_and(|(have, _)| *have >= rank) {
            continue;
        }
        let Some(at) = client.get("at").and_then(|v| v.as_array()) else {
            continue;
        };
        let Some(size) = client.get("size").and_then(|v| v.as_array()) else {
            continue;
        };
        let Some(address) = client.get("address").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(x) = at.first().and_then(|v| v.as_i64()) else {
            continue;
        };
        let Some(y) = at.get(1).and_then(|v| v.as_i64()) else {
            continue;
        };
        let Some(w) = size.first().and_then(|v| v.as_u64()) else {
            continue;
        };
        let Some(h) = size.get(1).and_then(|v| v.as_u64()) else {
            continue;
        };
        best = Some((
            rank,
            GameWindow {
                x: x as i32,
                y: y as i32,
                w: w as u32,
                h: h as u32,
                focused: active_addr.as_deref() == Some(address),
                monitor: client.get("monitor").and_then(|v| v.as_i64()).unwrap_or(0),
                address: address.to_string(),
                workspace: client
                    .get("workspace")
                    .and_then(|v| v.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                title: title.to_string(),
            },
        ));
    }
    best.map(|(_, window)| window)
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
            let transform = m.get("transform").and_then(|v| v.as_i64()).unwrap_or(0);
            let (w, h) = logical_size(
                transform,
                m.get("width").and_then(|v| v.as_i64())? as i32,
                m.get("height").and_then(|v| v.as_i64())? as i32,
            );
            Some(Monitor {
                id: m.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
                x: m.get("x").and_then(|v| v.as_i64())? as i32,
                y: m.get("y").and_then(|v| v.as_i64())? as i32,
                w,
                h,
                name: m
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect()
}

pub fn logical_size(transform: i64, width: i32, height: i32) -> (i32, i32) {
    if matches!(transform, 1 | 3 | 5 | 7) {
        (height, width)
    } else {
        (width, height)
    }
}

static SCREENSHOT_UNTIL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn pause_for_screenshot() {
    let now = unix_millis();
    SCREENSHOT_UNTIL.store(
        now.saturating_add(2_000),
        std::sync::atomic::Ordering::Relaxed,
    );
}

pub fn screenshot_paused() -> bool {
    unix_millis() < SCREENSHOT_UNTIL.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn capture_suppressed() -> bool {
    if consume_screenshot_flag() {
        pause_for_screenshot();
        return true;
    }
    if !screenshot_paused() {
        return false;
    }
    if picker_open() {
        let now = unix_millis();
        let until = SCREENSHOT_UNTIL.load(std::sync::atomic::Ordering::Relaxed);
        if until < now.saturating_add(500) {
            SCREENSHOT_UNTIL.store(
                now.saturating_add(500),
                std::sync::atomic::Ordering::Relaxed,
            );
        }
    }
    true
}

fn consume_screenshot_flag() -> bool {
    let Some(path) = screenshot_flag_path() else {
        return false;
    };
    std::fs::remove_file(path).is_ok()
}

fn screenshot_flag_path() -> Option<PathBuf> {
    let runtime = std::env::var("XDG_RUNTIME_DIR").ok()?;
    Some(PathBuf::from(runtime).join("rustyexile-screenshot"))
}

fn picker_open() -> bool {
    static NEXT_CHECK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    static OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let now = unix_millis();
    if now < NEXT_CHECK.load(std::sync::atomic::Ordering::Relaxed) {
        return OPEN.load(std::sync::atomic::Ordering::Relaxed);
    }
    NEXT_CHECK.store(
        now.saturating_add(200),
        std::sync::atomic::Ordering::Relaxed,
    );
    let open = command("j/layers")
        .map(|text| text.contains("solanum-area-picker"))
        .unwrap_or(false);
    OPEN.store(open, std::sync::atomic::Ordering::Relaxed);
    open
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

pub fn cursor() -> Option<(i32, i32)> {
    let text = command("cursorpos").ok()?;
    let (x, y) = text.trim().split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

pub fn send_paste() -> anyhow::Result<()> {
    let selector = focus_game_now()?;
    std::thread::sleep(std::time::Duration::from_millis(80));
    send_shortcut("CTRL", "F", &selector)?;
    std::thread::sleep(std::time::Duration::from_millis(60));
    send_shortcut("CTRL", "A", &selector)?;
    std::thread::sleep(std::time::Duration::from_millis(40));
    send_shortcut("CTRL", "V", &selector)?;
    Ok(())
}

pub fn send_chat(text: &str) -> anyhow::Result<()> {
    copy_chat(text)?;
    let selector = focus_game_now()?;
    std::thread::sleep(std::time::Duration::from_millis(90));
    send_shortcut("", "Return", &selector)?;
    std::thread::sleep(std::time::Duration::from_millis(80));
    send_shortcut("CTRL", "A", &selector)?;
    std::thread::sleep(std::time::Duration::from_millis(40));
    send_shortcut("CTRL", "V", &selector)?;
    std::thread::sleep(std::time::Duration::from_millis(50));
    send_shortcut("", "Return", &selector)?;
    drop_clipboard_history(text);
    Ok(())
}

fn focus_game_now() -> anyhow::Result<String> {
    let Some(window) = game_window() else {
        anyhow::bail!("Path of Exile is not running");
    };
    let selector = selector_for(&window.address);
    if !selector.is_empty() {
        let _ = command(&format!(
            "dispatch hl.dsp.focus({{ window = \"{selector}\" }})"
        ));
    }
    Ok(selector)
}

fn copy_chat(text: &str) -> anyhow::Result<()> {
    let mut child = std::process::Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|_| anyhow::anyhow!("wl-copy is not available"))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(text.as_bytes())?;
    }
    let status = child.wait()?;
    if status.success() {
        return Ok(());
    }
    anyhow::bail!("could not copy the chat text");
}

fn drop_clipboard_history(text: &str) {
    let needle = text.trim();
    if needle.is_empty() {
        return;
    }
    for _ in 0..10 {
        if delete_clipboard_entry(needle) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(80));
    }
    let _ = std::process::Command::new("wl-copy")
        .arg("--clear")
        .status();
}

fn delete_clipboard_entry(needle: &str) -> bool {
    let Ok(output) = std::process::Command::new("cliphist").arg("list").output() else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let Ok(list) = String::from_utf8(output.stdout) else {
        return false;
    };
    let Some(line) = list.lines().find(|line| clipboard_preview(line) == needle) else {
        return false;
    };
    let Ok(mut child) = std::process::Command::new("cliphist")
        .arg("delete")
        .stdin(std::process::Stdio::piped())
        .spawn()
    else {
        return false;
    };
    let Some(mut stdin) = child.stdin.take() else {
        return false;
    };
    use std::io::Write;
    if writeln!(stdin, "{line}").is_err() {
        return false;
    }
    drop(stdin);
    child.wait().ok().is_some_and(|status| status.success())
}

fn clipboard_preview(line: &str) -> &str {
    line.split_once('\t')
        .map(|(_, preview)| preview)
        .unwrap_or("")
}

pub fn prepare_config_window() {
    ensure_config_window_rule();
}

pub fn raise_config_window() {
    ensure_config_window_rule();
    let ticket = LIFT_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    std::thread::spawn(move || {
        let game_fs = fullscreen_game();
        let started = std::time::Instant::now();
        while started.elapsed() < std::time::Duration::from_millis(1_500) {
            if LIFT_GEN.load(std::sync::atomic::Ordering::Relaxed) != ticket {
                return;
            }
            if config_window_mapped() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        if !config_window_mapped() || LIFT_GEN.load(std::sync::atomic::Ordering::Relaxed) != ticket
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(60));
        if LIFT_GEN.load(std::sync::atomic::Ordering::Relaxed) != ticket {
            return;
        }
        present_config();
        std::thread::sleep(std::time::Duration::from_millis(80));
        if LIFT_GEN.load(std::sync::atomic::Ordering::Relaxed) != ticket {
            return;
        }
        if game_window().is_some_and(|game| !config_inside_game(&game)) {
            present_config();
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
        if let Some(game_fs) = &game_fs {
            restore_fullscreen(game_fs);
        }
        if LIFT_GEN.load(std::sync::atomic::Ordering::Relaxed) != ticket {
            return;
        }
        if let Some(selector) = config_window_selector() {
            let _ = command(&format!("dispatch {}", pin_request_body(&selector)));
            let _ = command(&focus_request(&selector));
        }
    });
}

pub fn config_popup_origin() -> Option<(i32, i32)> {
    let game = game_window()?;
    let (width, height) = config_size().unwrap_or((1240, 820));
    Some(place_inside(game.x, game.y, game.w, game.h, width, height))
}

pub fn dismiss_config_window() {
    LIFT_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let game = fullscreen_game();
    if let Some(selector) = config_window_selector() {
        for request in config_dismiss_requests(&selector) {
            let _ = command(&request);
        }
    }
    if let Some(game) = &game {
        restore_fullscreen(game);
    }
    focus_game();
}

static LIFT_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn present_config() {
    let Some(selector) = config_window_selector() else {
        return;
    };
    let Some(game) = game_window() else {
        let _ = command(&format!(
            "eval {}; {}",
            float_request_body(&selector),
            pin_request_body(&selector)
        ));
        return;
    };
    let (width, height) = config_size().unwrap_or((1240, 820));
    let (x, y) = place_inside(game.x, game.y, game.w, game.h, width, height);
    let monitor = monitors()
        .into_iter()
        .find(|monitor| monitor.id == game.monitor)
        .and_then(|monitor| safe_monitor_name(&monitor.name));
    let _ = command(&place_config_eval(
        &selector,
        monitor.as_deref(),
        safe_workspace(&game.workspace),
        x,
        y,
    ));
}

fn config_inside_game(game: &GameWindow) -> bool {
    let Some((x, y, w, h)) = config_rect() else {
        return false;
    };
    let cx = x + w / 2;
    let cy = y + h / 2;
    cx >= game.x && cy >= game.y && cx < game.x + game.w as i32 && cy < game.y + game.h as i32
}

fn config_size() -> Option<(u32, u32)> {
    let (_x, _y, width, height) = config_rect()?;
    (width > 0 && height > 0).then_some((width as u32, height as u32))
}

fn config_rect() -> Option<(i32, i32, i32, i32)> {
    let text = command("j/clients").ok()?;
    let list: Vec<Value> = serde_json::from_str(&text).ok()?;
    list.into_iter().find_map(|client| {
        let class = client.get("class").and_then(|v| v.as_str()).unwrap_or("");
        let title = client.get("title").and_then(|v| v.as_str()).unwrap_or("");
        if class != "RustyExile" && title != "RustyExile" {
            return None;
        }
        let at = client.get("at")?.as_array()?;
        let size = client.get("size")?.as_array()?;
        Some((
            at.first()?.as_i64()? as i32,
            at.get(1)?.as_i64()? as i32,
            size.first()?.as_i64()? as i32,
            size.get(1)?.as_i64()? as i32,
        ))
    })
}

fn place_inside(
    game_x: i32,
    game_y: i32,
    game_w: u32,
    game_h: u32,
    window_w: u32,
    window_h: u32,
) -> (i32, i32) {
    let game_w = game_w as i32;
    let game_h = game_h as i32;
    let window_w = window_w as i32;
    let window_h = window_h as i32;
    let x = if window_w >= game_w {
        game_x
    } else {
        (game_x + (game_w - window_w) / 2).clamp(game_x, game_x + game_w - window_w)
    };
    let y = if window_h >= game_h {
        game_y
    } else {
        (game_y + (game_h - window_h) / 2).clamp(game_y, game_y + game_h - window_h)
    };
    (x, y)
}

fn focus_game() {
    let Some(window) = game_window() else {
        return;
    };
    let selector = selector_for(&window.address);
    if selector.is_empty() {
        return;
    }
    let _ = command(&format!(
        "dispatch hl.dsp.focus({{ window = \"{selector}\" }})"
    ));
}

fn restore_fullscreen(game: &FullscreenGame) {
    let _ = command(&restore_fullscreen_request(
        &game.selector,
        game.internal,
        game.client_mode,
    ));
}

fn ensure_config_window_rule() {
    static GUARD: std::sync::Mutex<bool> = std::sync::Mutex::new(false);
    let Ok(mut done) = GUARD.lock() else {
        return;
    };
    if *done {
        return;
    }
    let Ok(out) = command(
        r#"eval hl.window_rule({ name = "rustyexile-config", enabled = false }); hl.window_rule({ name = "rustyexile-float", match = { class = "^RustyExile$" }, float = true, pin = true, no_screen_share = true }); hl.layer_rule({ name = "rustyexile-noshare", match = { namespace = "^RustyExile" }, no_screen_share = true })"#,
    ) else {
        return;
    };
    if !out.contains("error") {
        *done = true;
    }
}

struct FullscreenGame {
    selector: String,
    internal: i64,
    client_mode: i64,
}

fn fullscreen_game() -> Option<FullscreenGame> {
    let text = command("j/clients").ok()?;
    let list: Vec<Value> = serde_json::from_str(&text).ok()?;
    list.into_iter().find_map(|client| {
        let class = client.get("class").and_then(|v| v.as_str()).unwrap_or("");
        let title = client.get("title").and_then(|v| v.as_str()).unwrap_or("");
        if !is_poe_window(class, title) {
            return None;
        }
        let internal = client
            .get("fullscreen")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let client_mode = client
            .get("fullscreenClient")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        if internal <= 0 && client_mode <= 0 {
            return None;
        }
        let selector = client
            .get("address")
            .and_then(|v| v.as_str())
            .and_then(safe_selector)?;
        Some(FullscreenGame {
            selector,
            internal,
            client_mode,
        })
    })
}

fn config_window_mapped() -> bool {
    let Ok(text) = command("j/clients") else {
        return false;
    };
    let Ok(list) = serde_json::from_str::<Vec<Value>>(&text) else {
        return false;
    };
    list.into_iter().any(|client| {
        let class = client.get("class").and_then(|v| v.as_str()).unwrap_or("");
        let title = client.get("title").and_then(|v| v.as_str()).unwrap_or("");
        (class == "RustyExile" || title == "RustyExile")
            && client
                .get("mapped")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
    })
}

fn config_window_selector() -> Option<String> {
    let text = command("j/clients").ok()?;
    let list: Vec<Value> = serde_json::from_str(&text).ok()?;
    list.into_iter().find_map(|client| {
        let class = client.get("class").and_then(|v| v.as_str()).unwrap_or("");
        let title = client.get("title").and_then(|v| v.as_str()).unwrap_or("");
        if class == "RustyExile" || title == "RustyExile" {
            client
                .get("address")
                .and_then(|v| v.as_str())
                .and_then(safe_selector)
        } else {
            None
        }
    })
}

#[cfg(test)]
fn config_raise_requests(selector: &str) -> [String; 3] {
    [
        float_request(selector),
        pin_request(selector),
        focus_request(selector),
    ]
}

#[cfg(test)]
fn float_request(selector: &str) -> String {
    format!("dispatch {}", float_request_body(selector))
}

fn float_request_body(selector: &str) -> String {
    format!("hl.dsp.window.float({{ action = \"enable\", window = \"{selector}\" }})")
}

#[cfg(test)]
fn pin_request(selector: &str) -> String {
    format!("dispatch {}", pin_request_body(selector))
}

fn pin_request_body(selector: &str) -> String {
    format!("hl.dsp.window.pin({{ action = \"enable\", window = \"{selector}\" }})")
}

fn focus_request(selector: &str) -> String {
    format!("dispatch hl.dsp.focus({{ window = \"{selector}\" }})")
}

fn move_to_workspace_request(selector: &str, workspace: &str) -> String {
    format!(
        "dispatch hl.dsp.window.move({{ workspace = \"{workspace}\", follow = false, window = \"{selector}\" }})"
    )
}

#[cfg(test)]
fn move_to_position_request(selector: &str, x: i32, y: i32) -> String {
    format!("dispatch {}", move_to_position_body(selector, x, y))
}

fn move_to_position_body(selector: &str, x: i32, y: i32) -> String {
    format!("hl.dsp.window.move({{ x = {x}, y = {y}, relative = false, window = \"{selector}\" }})")
}

fn place_config_eval(
    selector: &str,
    monitor: Option<&str>,
    workspace: Option<&str>,
    x: i32,
    y: i32,
) -> String {
    let mut steps = vec![float_request_body(selector)];
    if let Some(monitor) = monitor {
        steps.push(format!(
            "hl.dsp.window.move({{ monitor = \"{monitor}\", follow = false, window = \"{selector}\" }})"
        ));
    }
    if let Some(workspace) = workspace {
        steps.push(format!(
            "hl.dsp.window.move({{ workspace = \"{workspace}\", follow = false, window = \"{selector}\" }})"
        ));
    }
    steps.push(move_to_position_body(selector, x, y));
    format!("eval {}", steps.join("; "))
}

fn safe_monitor_name(name: &str) -> Option<String> {
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        None
    } else {
        Some(name.to_string())
    }
}

fn safe_workspace(name: &str) -> Option<&str> {
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == ':')
    {
        None
    } else {
        Some(name)
    }
}

fn config_dismiss_requests(selector: &str) -> [String; 2] {
    [
        format!("dispatch hl.dsp.window.pin({{ action = \"disable\", window = \"{selector}\" }})"),
        move_to_workspace_request(selector, "special:rustyexile"),
    ]
}

fn restore_fullscreen_request(selector: &str, internal: i64, client_mode: i64) -> String {
    format!(
        "dispatch hl.dsp.window.fullscreen_state({{ internal = {internal}, client = {client_mode}, action = \"set\", window = \"{selector}\" }})"
    )
}

pub fn sync_config_bind(previous: &str, next: &str) {
    let mut steps = Vec::new();
    if let Some(old) = hypr_accel(previous) {
        steps.push(format!("hl.unbind(\"{old}\")"));
    }
    if let Some(accel) = hypr_accel(next) {
        steps.push(format!(
            "hl.bind(\"{accel}\", hl.dsp.global(\"RustyExile:config\"))"
        ));
    }
    if steps.is_empty() {
        return;
    }
    let _ = command(&format!("eval {}", steps.join(" ")));
}

pub fn hypr_accel(raw: &str) -> Option<String> {
    let chord = poe_core::parse_chord(raw);
    if chord.key.is_empty() || !chord.key.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return None;
    }
    let mut mods = Vec::new();
    if chord.ctrl {
        mods.push("CTRL");
    }
    if chord.alt {
        mods.push("ALT");
    }
    if chord.shift {
        mods.push("SHIFT");
    }
    let key = hypr_key(&chord.key)?;
    if mods.is_empty() {
        Some(key)
    } else {
        Some(format!("{} + {key}", mods.join(" + ")))
    }
}

fn hypr_key(key: &str) -> Option<String> {
    let name = match key {
        "return" | "enter" => "Return",
        "escape" | "esc" => "Escape",
        "space" => "Space",
        "tab" => "Tab",
        "backspace" => "Backspace",
        other if other.len() == 1 => return Some(other.to_ascii_uppercase()),
        other => other,
    };
    let mut chars = name.chars();
    let first = chars.next()?.to_ascii_uppercase().to_string();
    Some(first + chars.as_str())
}

fn safe_selector(address: &str) -> Option<String> {
    let bare = address.strip_prefix("address:").unwrap_or(address);
    if bare.is_empty() || !bare.chars().all(|ch| ch.is_ascii_hexdigit() || ch == 'x') {
        return None;
    }
    Some(selector_for(bare))
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
    poe_window_rank(class, title).is_some()
}

fn poe_window_rank(class: &str, title: &str) -> Option<u8> {
    let class_l = class.to_ascii_lowercase();
    let title_l = title.to_ascii_lowercase();
    if title_l.contains("path of exile 2")
        || class_l.contains("path of exile 2")
        || class_l.contains("pathofexile2")
    {
        return None;
    }
    if class_l.contains("steam_app_238960") || class_l.contains("pathofexile") {
        return Some(2);
    }
    if title_l == "path of exile" {
        return Some(2);
    }
    if title_l.contains("path of exile") {
        return Some(1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_history_line_uses_the_preview() {
        assert_eq!(clipboard_preview("12\t/hideout"), "/hideout");
        assert_eq!(clipboard_preview("no-tab"), "");
    }

    #[test]
    fn game_window_outranks_a_browser_tab() {
        assert!(
            poe_window_rank("steam_app_238960", "Path of Exile").unwrap()
                > poe_window_rank("firefox", "Path of Exile - poe-leveling").unwrap()
        );
        assert!(poe_window_rank("firefox", "YouTube").is_none());
    }

    #[test]
    fn config_raise_leaves_the_game_fullscreen() {
        let requests = config_raise_requests("address:0xabc");
        assert!(requests
            .iter()
            .all(|request| !request.contains("fullscreen")));
        assert!(requests[0].contains("float") && requests[0].contains("enable"));
        assert!(requests[1].contains("pin") && requests[1].contains("enable"));
        assert!(requests[2].contains("focus"));
        let restore = restore_fullscreen_request("address:0xdef", 2, 2);
        assert!(restore.contains("internal = 2"));
        assert!(restore.contains("client = 2"));
        assert!(!restore.contains("internal = 0"));
        let dismiss = config_dismiss_requests("address:0xabc");
        assert!(dismiss[0].contains("pin") && dismiss[0].contains("disable"));
        assert!(dismiss[1].contains("special:rustyexile"));
        assert!(dismiss[1].contains("follow = false"));
        assert!(dismiss
            .iter()
            .all(|request| !request.contains("fullscreen")));
        let (x, y) = place_inside(768, 0, 1920, 1080, 1240, 820);
        assert_eq!((x, y), (1108, 130));
        assert!(x >= 768 && x + 1240 <= 768 + 1920);
        assert!(y >= 0 && y + 820 <= 1080);
        let (x, y) = place_inside(768, 0, 1000, 700, 1240, 820);
        assert_eq!((x, y), (768, 0));
        assert_eq!(logical_size(1, 1366, 768), (768, 1366));
        assert_eq!(logical_size(0, 1920, 1080), (1920, 1080));
        let position = move_to_position_request("address:0xabc", 1108, 130);
        assert!(position.contains("relative = false"));
        assert!(position.contains("x = 1108"));
        let placed = place_config_eval("address:0xabc", Some("DP-2"), Some("1"), 1108, 130);
        let monitor_at = placed.find("monitor = \"DP-2\"").expect("monitor");
        let pixel_at = placed.find("x = 1108").expect("pixel");
        assert!(monitor_at < pixel_at);
        assert!(placed.contains("follow = false"));
        assert!(!placed.contains("fullscreen"));
        assert!(!placed.contains("pin"));
        assert_eq!(safe_monitor_name("DP-2").as_deref(), Some("DP-2"));
        assert_eq!(safe_monitor_name("HDMI-A-1").as_deref(), Some("HDMI-A-1"));
        assert!(safe_monitor_name("DP-2; evil").is_none());
    }

    #[test]
    fn config_hotkey_uses_hyprland_accelerator() {
        assert_eq!(
            hypr_accel("ctrl+shift+o").as_deref(),
            Some("CTRL + SHIFT + O")
        );
        assert_eq!(hypr_accel("ctrl+alt+h").as_deref(), Some("CTRL + ALT + H"));
    }

    #[test]
    fn matches_poe1_titles_only() {
        assert!(is_poe_window("steam_app_238960", "Path of Exile"));
        assert!(!is_poe_window("steam_app_2694490", "Path of Exile 2"));
    }
}
