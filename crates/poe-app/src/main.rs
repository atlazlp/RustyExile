use poe_core::{
    apply_parsed, detect_running, load_catalog, load_character, load_regex_presets, load_settings,
    note_current_zone, read_log_tail, save_catalog, save_character, save_regex_presets, save_settings,
    vendor_gem_regex, visible_lines, AppState, CharacterFile, LogFollow, LogParser, ParsedLog,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
mod tray;

fn main() {
    let state = Arc::new(Mutex::new(load_state()));
    let watch = Arc::clone(&state);
    std::thread::Builder::new()
        .name("poe-watch".into())
        .spawn(move || watch_game(watch))
        .ok();
    let keys = Arc::clone(&state);
    std::thread::Builder::new()
        .name("poe-keys".into())
        .spawn(move || hotkeys(keys))
        .ok();
    poe_overlay::spawn(Arc::clone(&state));
    #[cfg(target_os = "linux")]
    {
        let tray_state = Arc::clone(&state);
        std::thread::Builder::new()
            .name("poe-tray".into())
            .spawn(move || {
                if let Err(err) = tray::run(tray_state) {
                    eprintln!("tray icon unavailable: {err}");
                }
            })
            .ok();
    }
    if let Err(err) = poe_config::run(state) {
        eprintln!("configuration window closed: {err}");
    }
}

fn load_state() -> AppState {
    let mut settings = load_settings();
    let mut dirty_settings = false;
    if settings.experience == poe_core::Experience::Advanced && !settings.tips && !settings.acts && !settings.gems {
        settings.gems = true;
        dirty_settings = true;
    }
    let embedded = poe_data::load_catalog();
    let catalog = load_catalog(embedded);
    let regex_presets = load_regex_presets(poe_data::load_regex_defaults());
    let characters = poe_core::list_characters();
    AppState {
        settings,
        catalog,
        characters,
        regex_presets,
        show_config: true,
        hud_enabled: true,
        running: None,
        status: String::new(),
        dirty_settings,
        dirty_catalog: false,
        dirty_characters: false,
        dirty_regex: false,
        ctrl_down: false,
        alt_down: false,
        capturing_hotkey: false,
    }
}

fn watch_game(state: Arc<Mutex<AppState>>) {
    let mut follow: Option<LogFollow> = None;
    let mut parser = LogParser::default();
    let mut followed = String::new();
    let mut last_save = Instant::now();
    let mut cached: Option<poe_core::GameLocation> = None;
    let mut last_scan = Instant::now() - Duration::from_secs(5);
    loop {
        let scan = match cached.as_ref().and_then(|game| game.pid) {
            Some(pid) if process_alive(pid) => last_scan.elapsed() > Duration::from_secs(2),
            _ => last_scan.elapsed() > Duration::from_millis(400),
        };
        if scan {
            last_scan = Instant::now();
            cached = detect_running();
        }
        let detected = cached.clone();
        if let Ok(mut app) = state.lock() {
            if let Some(game) = &detected {
                app.settings.install = Some(game.saved());
                app.dirty_settings = true;
                app.running = Some(game.clone());
            } else {
                app.running = None;
            }
            let path = app
                .settings
                .log_path_override
                .clone()
                .or_else(|| detected.as_ref().map(|g| g.log_path.display().to_string()));
            if let Some(path) = path {
                if path != followed {
                    followed = path.clone();
                    follow = LogFollow::open(PathBuf::from(&path));
                    parser = LogParser::default();
                    adopt_recent(&mut app, PathBuf::from(&path));
                }
            }
            let chat = app.status.strip_prefix("chat:").map(str::to_string);
            if let Some(text) = chat {
                app.status = "Opening chat.".into();
                drop(app);
                match send_chat_line(&text) {
                    Ok(()) => {
                        if let Ok(mut app) = state.lock() {
                            app.status = format!("Sent {text}");
                        }
                    }
                    Err(err) => {
                        if let Ok(mut app) = state.lock() {
                            app.status = format!("Could not send chat. {err}");
                        }
                    }
                }
            } else if app.status.starts_with("copy:") {
                let regex = app.status.trim_start_matches("copy:").to_string();
                app.status = "Copied to the clipboard.".into();
                drop(app);
                let _ = copy_regex(&regex);
            } else if app.status.starts_with("paste:") {
                let regex = app.status.trim_start_matches("paste:").to_string();
                app.status = "Pasting vendor regex.".into();
                drop(app);
                let _ = paste_regex(&regex);
            }
        }
        if let Some(follow) = follow.as_mut() {
            for line in follow.poll() {
                if let Some(parsed) = parser.push_line(&line) {
                    if let Ok(mut app) = state.lock() {
                        apply_line(&mut app, parsed);
                    }
                }
            }
        }
        if last_save.elapsed() > Duration::from_secs(1) {
            persist(&state);
            last_save = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(150));
    }
}

fn adopt_recent(app: &mut AppState, path: PathBuf) {
    let Some(text) = read_log_tail(&path, 256 * 1024) else {
        return;
    };
    let mut parser = LogParser::default();
    let mut zone = None;
    let mut level = None;
    for line in text.lines() {
        match parser.push_line(line) {
            Some(ParsedLog::Entered { name, area_id }) => zone = Some((name, area_id)),
            Some(ParsedLog::LevelUp { name, class_name, level: lvl }) => {
                level = Some((name, class_name, lvl));
            }
            _ => {}
        }
    }
    if let Some((name, class_name, lvl)) = level {
        ensure_character(app, &name, &class_name);
        if let Some(character) = app.characters.iter_mut().find(|c| c.name == name) {
            character.progress.level = character.progress.level.max(lvl);
            character.class_name = class_name;
        }
        app.settings.active_character = Some(name);
    }
    if let Some((name, area_id)) = zone {
        let act = area_id.as_ref().and_then(|id| app.catalog.area(id).map(|a| a.act));
        if let Some(character) = app.active_character_mut() {
            note_current_zone(&mut character.progress, &name, area_id.as_deref(), act);
            app.dirty_characters = true;
        }
    }
}

fn apply_line(app: &mut AppState, parsed: ParsedLog) {
    if let ParsedLog::LevelUp { name, class_name, level } = &parsed {
        ensure_character(app, name, class_name);
        app.settings.active_character = Some(name.clone());
        app.dirty_settings = true;
        let _ = level;
    }
    let area_act = if let ParsedLog::Entered { area_id: Some(id), .. } = &parsed {
        app.catalog.area(id).map(|a| a.act)
    } else {
        None
    };
    let Some(name) = app.settings.active_character.clone() else {
        return;
    };
    let Some(character) = app.characters.iter_mut().find(|c| c.name == name) else {
        return;
    };
    if apply_parsed(character, &parsed, area_act) {
        app.dirty_characters = true;
    }
}

fn ensure_character(app: &mut AppState, name: &str, class_name: &str) {
    if app.characters.iter().any(|c| c.name == name) {
        return;
    }
    if let Some(existing) = load_character(name) {
        app.characters.push(existing);
        return;
    }
    let route = poe_core::build_route(&app.catalog, class_name, None, app.settings.league_start);
    app.characters.push(CharacterFile {
        name: name.to_string(),
        class_name: class_name.to_string(),
        route,
        progress: Default::default(),
        user_edited: false,
    });
    app.dirty_characters = true;
}

fn persist(state: &Mutex<AppState>) {
    let Ok(mut app) = state.lock() else { return };
    if app.dirty_settings {
        let _ = save_settings(&app.settings);
        app.dirty_settings = false;
    }
    if app.dirty_catalog {
        let _ = save_catalog(&app.catalog);
        app.dirty_catalog = false;
    }
    if app.dirty_regex {
        let _ = save_regex_presets(&app.regex_presets);
        app.dirty_regex = false;
    }
    if app.dirty_characters {
        for character in &app.characters {
            let _ = save_character(character);
        }
        app.dirty_characters = false;
    }
}

fn send_chat_line(text: &str) -> anyhow::Result<()> {
    if text.trim().is_empty() {
        anyhow::bail!("the chat text is empty");
    }
    copy_regex(text)?;
    poe_overlay::send_chat()
}

fn process_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        false
    }
}
fn copy_regex(regex: &str) -> anyhow::Result<()> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(regex.to_owned())?;
    Ok(())
}

fn paste_regex(regex: &str) -> anyhow::Result<()> {
    copy_regex(regex)?;
    poe_overlay::paste_into_game()?;
    Ok(())
}

fn hotkeys(state: Arc<Mutex<AppState>>) {
    #[cfg(target_os = "linux")]
    {
        let _ = bind_portal(&state);
        evdev_loop(state);
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = state;
    }
}

#[cfg(target_os = "linux")]
fn evdev_loop(state: Arc<Mutex<AppState>>) {
    let mut devices = Vec::new();
    for (_, device) in evdev::enumerate() {
        let Some(keys) = device.supported_keys() else { continue };
        if !keys.contains(evdev::KeyCode::KEY_LEFTCTRL) && !keys.contains(evdev::KeyCode::KEY_A) {
            continue;
        }
        let _ = device.set_nonblocking(true);
        devices.push(device);
    }
    if devices.is_empty() {
        eprintln!("no keyboard devices could be opened; Ctrl+Alt+click needs the user in the input group");
    }
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    loop {
        for device in &mut devices {
            let Ok(events) = device.fetch_events() else { continue };
            for event in events {
                if event.event_type() != evdev::EventType::KEY {
                    continue;
                }
                if event.value() == 2 {
                    continue;
                }
                let down = event.value() == 1;
                let key = evdev::KeyCode::new(event.code());
                match key {
                    evdev::KeyCode::KEY_LEFTCTRL | evdev::KeyCode::KEY_RIGHTCTRL => ctrl = down,
                    evdev::KeyCode::KEY_LEFTALT | evdev::KeyCode::KEY_RIGHTALT => alt = down,
                    evdev::KeyCode::KEY_LEFTSHIFT | evdev::KeyCode::KEY_RIGHTSHIFT => shift = down,
                    _ if down => {
                        if let Some(name) = key_name(key) {
                            if let Ok(mut app) = state.lock() {
                                app.ctrl_down = ctrl;
                                app.alt_down = alt;
                                if app.capturing_hotkey {
                                    continue;
                                }
                                let toggle = poe_core::parse_chord(&app.settings.hotkeys.toggle_hud);
                                let open = poe_core::parse_chord(&app.settings.hotkeys.open_config);
                                let paste = poe_core::parse_chord(&app.settings.hotkeys.paste_regex);
                                if chord_hit(&toggle, ctrl, alt, shift, &name) {
                                    app.hud_enabled = !app.hud_enabled;
                                } else if chord_hit(&open, ctrl, alt, shift, &name) {
                                    app.show_config = true;
                                } else if chord_hit(&paste, ctrl, alt, shift, &name) {
                                    let regex = current_regex(&app);
                                    app.status = format!("paste:{regex}");
                                } else if let Some(text) = chat_text(&app, ctrl, alt, shift, &name) {
                                    queue_chat(&mut app, &text);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Ok(mut app) = state.lock() {
            app.ctrl_down = ctrl;
            app.alt_down = alt;
        }
        std::thread::sleep(Duration::from_millis(8));
    }
}

fn chord_hit(chord: &poe_core::Chord, ctrl: bool, alt: bool, shift: bool, key: &str) -> bool {
    !chord.key.is_empty() && chord.ctrl == ctrl && chord.alt == alt && chord.shift == shift && chord.key == key
}

fn queue_chat(app: &mut AppState, text: &str) {
    if text.is_empty() {
        return;
    }
    static LAST: Mutex<Option<(String, Instant)>> = Mutex::new(None);
    if let Ok(mut last) = LAST.lock() {
        if let Some((prev, when)) = last.as_ref() {
            if prev == text && when.elapsed() < Duration::from_millis(500) {
                return;
            }
        }
        *last = Some((text.to_string(), Instant::now()));
    }
    app.status = format!("chat:{text}");
}

fn chat_text(app: &AppState, ctrl: bool, alt: bool, shift: bool, key: &str) -> Option<String> {
    app.settings.hotkeys.chat.iter().find_map(|cmd| {
        if cmd.text.is_empty() || cmd.hotkey.is_empty() {
            return None;
        }
        let chord = poe_core::parse_chord(&cmd.hotkey);
        chord_hit(&chord, ctrl, alt, shift, key).then(|| cmd.text.clone())
    })
}

fn current_regex(app: &AppState) -> String {
    if let Some(preset) = app.regex_presets.get(app.settings.active_regex) {
        if !preset.regex.is_empty() {
            return preset.regex.clone();
        }
    }
    let names = app
        .active_character()
        .map(|character| {
            visible_lines(
                &character.route,
                &character.progress,
                app.settings.acts,
                app.settings.gems,
                app.settings.tips,
            )
            .into_iter()
            .filter_map(|line| line.gem_name)
            .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    vendor_gem_regex(&names)
}

#[cfg(target_os = "linux")]
fn key_name(key: evdev::KeyCode) -> Option<String> {
    let raw = format!("{key:?}");
    let name = raw.strip_prefix("KEY_")?.to_ascii_lowercase();
    Some(name)
}

#[cfg(target_os = "linux")]
fn bind_portal(state: &Arc<Mutex<AppState>>) -> anyhow::Result<()> {
    let shortcuts = {
        let app = state.lock().map_err(|_| anyhow::anyhow!("lock"))?;
        let mut shortcuts = vec![
            ("toggle".to_string(), app.settings.hotkeys.toggle_hud.clone()),
            ("config".to_string(), app.settings.hotkeys.open_config.clone()),
            ("paste".to_string(), app.settings.hotkeys.paste_regex.clone()),
        ];
        for cmd in &app.settings.hotkeys.chat {
            if cmd.hotkey.is_empty() || cmd.text.is_empty() {
                continue;
            }
            shortcuts.push((cmd.id.clone(), cmd.hotkey.clone()));
        }
        shortcuts
    };
    let state = Arc::clone(state);
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build();
        let Ok(runtime) = runtime else { return };
        runtime.block_on(async move {
            if let Err(err) = portal_session(state, shortcuts).await {
                eprintln!("global shortcuts portal unavailable, using the keyboard device instead: {err}");
            }
        });
    });
    Ok(())
}

#[cfg(target_os = "linux")]
async fn portal_session(state: Arc<Mutex<AppState>>, shortcuts: Vec<(String, String)>) -> anyhow::Result<()> {
    use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
    let proxy = GlobalShortcuts::new().await?;
    let session = proxy.create_session().await?;
    let bound: Vec<NewShortcut> = shortcuts
        .iter()
        .map(|(id, trigger)| {
            NewShortcut::new(id.as_str(), id.as_str()).preferred_trigger(Some(trigger.as_str()))
        })
        .collect();
    proxy.bind_shortcuts(&session, &bound, None).await?;
    let mut activated = proxy.receive_activated().await?;
    use futures_util::StreamExt;
    while let Some(event) = activated.next().await {
        let id = event.shortcut_id();
        let Ok(mut app) = state.lock() else { continue };
        if app.capturing_hotkey {
            continue;
        }
        match id {
            "toggle" => app.hud_enabled = !app.hud_enabled,
            "config" => app.show_config = true,
            "paste" => {
                let regex = current_regex(&app);
                app.status = format!("paste:{regex}");
            }
            other => {
                if let Some(cmd) = app.settings.hotkeys.chat.iter().find(|cmd| cmd.id == other) {
                    if !cmd.text.is_empty() {
                        let text = cmd.text.clone();
                        queue_chat(&mut app, &text);
                    }
                }
            }
        }
    }
    Ok(())
}

