#![cfg_attr(windows, windows_subsystem = "windows")]

use poe_core::{
    apply_parsed, detect_running, load_catalog, load_character, load_regex_presets, load_settings,
    note_current_zone, nudge_events, read_log_tail, refresh_generated_route, save_catalog,
    save_character, save_regex_presets, save_settings, sync_skipped, vendor_gem_regex,
    visible_lines, AppState, CharacterFile, LogFollow, LogParser, ParsedLog,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[cfg(any(unix, windows))]
mod instance;
#[cfg(target_os = "linux")]
mod tray;

fn main() {
    #[cfg(any(unix, windows))]
    let instance = match instance::acquire() {
        Some(instance) => instance,
        None => return,
    };
    poe_overlay::prepare_config_window();
    let state = Arc::new(Mutex::new(load_state()));
    #[cfg(any(unix, windows))]
    instance::serve(instance, Arc::clone(&state));
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
    if settings.experience == poe_core::Experience::Advanced
        && !settings.tips
        && !settings.acts
        && !settings.gems
    {
        settings.gems = true;
        dirty_settings = true;
    }
    let embedded = poe_data::load_catalog();
    let catalog = load_catalog(embedded);
    let regex_presets = load_regex_presets(poe_data::load_regex_defaults());
    let mut characters = poe_core::list_characters();
    let mut dirty_characters = false;
    for character in &mut characters {
        if !character.user_edited {
            refresh_generated_route(character, &catalog, settings.league_start);
            dirty_characters = true;
        }
    }
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
        dirty_characters,
        dirty_regex: false,
        ctrl_down: false,
        shift_down: false,
        alt_down: false,
        capturing_hotkey: false,
        hud_placing: false,
    }
}

fn watch_game(state: Arc<Mutex<AppState>>) {
    let mut follow: Option<LogFollow> = None;
    let mut parser = LogParser::default();
    let mut followed = String::new();
    let mut last_save = Instant::now();
    let mut cached: Option<poe_core::GameLocation> = None;
    let mut last_scan = Instant::now() - Duration::from_secs(5);
    let mut defer_since: Option<Instant> = None;
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
        let defer = state.lock().ok().is_some_and(|app| {
            (app.status.starts_with("chat:") || app.status.starts_with("paste:"))
                && (app.ctrl_down || app.alt_down || app.shift_down)
        });
        if defer {
            let started = defer_since.get_or_insert_with(Instant::now);
            if started.elapsed() < Duration::from_millis(1200) {
                std::thread::sleep(Duration::from_millis(30));
                continue;
            }
        }
        defer_since = None;
        let mut open_log = None;
        let mut chat = None;
        let mut copy = None;
        let mut paste = None;
        if let Ok(mut app) = state.lock() {
            if let Some(game) = &detected {
                let saved = game.saved();
                if app.settings.install.as_ref() != Some(&saved) {
                    app.settings.install = Some(saved);
                    app.dirty_settings = true;
                }
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
                    open_log = Some(PathBuf::from(path));
                }
            }
            if let Some(text) = app.status.strip_prefix("chat:").map(str::to_string) {
                app.status = "Opening chat.".into();
                chat = Some(text);
            } else if let Some(regex) = app.status.strip_prefix("copy:").map(str::to_string) {
                app.status = "Copied to the clipboard.".into();
                copy = Some(regex);
            } else if let Some(regex) = app.status.strip_prefix("paste:").map(str::to_string) {
                app.status = "Pasting vendor regex.".into();
                paste = Some(regex);
            }
        }
        if let Some(path) = open_log {
            follow = LogFollow::open(path.clone());
            parser = LogParser::default();
            let tail = read_log_tail(&path, 256 * 1024);
            if let Ok(mut app) = state.lock() {
                adopt_recent(&mut app, tail.as_deref());
            }
        }
        if let Some(text) = chat {
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
        } else if let Some(regex) = copy {
            let _ = copy_regex(&regex);
        } else if let Some(regex) = paste {
            let _ = paste_regex(&regex);
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

fn adopt_recent(app: &mut AppState, text: Option<&str>) {
    let Some(text) = text else {
        return;
    };
    let mut parser = LogParser::default();
    let mut zone = None;
    let mut level = None;
    for line in text.lines() {
        match parser.push_line(line) {
            Some(ParsedLog::Entered { name, area_id }) => zone = Some((name, area_id)),
            Some(ParsedLog::LevelUp {
                name,
                class_name,
                level: lvl,
            }) => {
                level = Some((name, class_name, lvl));
            }
            _ => {}
        }
    }
    if let Some((name, class_name, lvl)) = level {
        if app.settings.active_character.is_none() {
            ensure_character(app, &name, &class_name);
            app.settings.active_character = Some(name.clone());
            app.dirty_settings = true;
        }
        if let Some(character) = app.active_character_mut() {
            if character.name.eq_ignore_ascii_case(&name) {
                character.progress.level = character.progress.level.max(lvl);
                character.class_name = class_name.clone();
                character.progress.class_name = class_name;
            } else if character.progress.level < lvl {
                character.progress.level = lvl;
            }
            app.dirty_characters = true;
        }
    }
    if let Some((name, area_id)) = zone {
        let act = area_id
            .as_ref()
            .and_then(|id| app.catalog.area(id).map(|a| a.act));
        if let Some(character) = app.active_character_mut() {
            note_current_zone(character, &name, area_id.as_deref(), act);
            sync_skipped(character);
            app.dirty_characters = true;
        }
    }
}

fn apply_line(app: &mut AppState, parsed: ParsedLog) {
    if let ParsedLog::LevelUp {
        name,
        class_name,
        level,
    } = &parsed
    {
        let selected = app.settings.active_character.clone();
        match selected {
            Some(active) if !active.eq_ignore_ascii_case(name) => {
                if let Some(character) = app.characters.iter_mut().find(|c| c.name == active) {
                    if character.progress.level < *level {
                        character.progress.level = *level;
                        app.dirty_characters = true;
                    }
                }
            }
            _ => {
                ensure_character(app, name, class_name);
                app.settings.active_character = Some(name.clone());
                app.dirty_settings = true;
            }
        }
    }
    let area_act = if let ParsedLog::Entered {
        area_id: Some(id), ..
    } = &parsed
    {
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

pub(crate) fn request_config(state: &Mutex<AppState>) {
    poe_overlay::raise_config_window();
    if let Ok(mut app) = state.lock() {
        app.show_config = true;
    }
}

pub(crate) fn persist(state: &Mutex<AppState>) {
    let snapshot = {
        let Ok(mut app) = state.lock() else { return };
        let settings = app.dirty_settings.then(|| {
            app.dirty_settings = false;
            app.settings.clone()
        });
        let catalog = app.dirty_catalog.then(|| {
            app.dirty_catalog = false;
            app.catalog.clone()
        });
        let regex = app.dirty_regex.then(|| {
            app.dirty_regex = false;
            app.regex_presets.clone()
        });
        let characters = app.dirty_characters.then(|| {
            app.dirty_characters = false;
            app.characters.clone()
        });
        (settings, catalog, regex, characters)
    };
    if let Some(settings) = snapshot.0 {
        let _ = save_settings(&settings);
    }
    if let Some(catalog) = snapshot.1 {
        let _ = save_catalog(&catalog);
    }
    if let Some(regex) = snapshot.2 {
        let _ = save_regex_presets(&regex);
    }
    if let Some(characters) = snapshot.3 {
        for character in &characters {
            let _ = save_character(character);
        }
    }
}

fn send_chat_line(text: &str) -> anyhow::Result<()> {
    if text.trim().is_empty() {
        anyhow::bail!("the chat text is empty");
    }
    poe_overlay::send_chat(text)
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
    let mut child = std::process::Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        stdin.write_all(regex.as_bytes())?;
    }
    let status = child.wait()?;
    if status.success() {
        return Ok(());
    }
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
        let Some(keys) = device.supported_keys() else {
            continue;
        };
        if !keys.contains(evdev::KeyCode::KEY_LEFTCTRL) && !keys.contains(evdev::KeyCode::KEY_A) {
            continue;
        }
        let _ = device.set_nonblocking(true);
        devices.push(device);
    }
    if devices.is_empty() {
        eprintln!(
            "no keyboard devices could be opened; Ctrl+Alt+click needs the user in the input group"
        );
    }
    let mut held = vec![ModState::default(); devices.len()];
    let mut bound_config = String::new();
    loop {
        let config_chord = state
            .lock()
            .ok()
            .map(|app| app.settings.hotkeys.open_config.clone())
            .unwrap_or_default();
        if config_chord != bound_config {
            poe_overlay::sync_config_bind(&bound_config, &config_chord);
            bound_config = config_chord;
        }
        for (index, device) in devices.iter_mut().enumerate() {
            let Ok(events) = device.fetch_events() else {
                continue;
            };
            for event in events {
                if event.event_type() != evdev::EventType::KEY {
                    continue;
                }
                if event.value() == 2 {
                    continue;
                }
                let down = event.value() == 1;
                let key = evdev::KeyCode::new(event.code());
                let name = match key {
                    evdev::KeyCode::KEY_LEFTCTRL | evdev::KeyCode::KEY_RIGHTCTRL => {
                        held[index].ctrl = down;
                        None
                    }
                    evdev::KeyCode::KEY_LEFTALT | evdev::KeyCode::KEY_RIGHTALT => {
                        held[index].alt = down;
                        None
                    }
                    evdev::KeyCode::KEY_LEFTSHIFT | evdev::KeyCode::KEY_RIGHTSHIFT => {
                        held[index].shift = down;
                        None
                    }
                    evdev::KeyCode::KEY_SYSRQ if down => {
                        poe_overlay::pause_for_screenshot();
                        None
                    }
                    _ if down => key_name(key),
                    _ => None,
                };
                let Some(name) = name else { continue };
                let (ctrl, alt, shift) = combined_mods(&held);
                let mut raise_config = false;
                if let Ok(mut app) = state.lock() {
                    app.ctrl_down = ctrl;
                    app.alt_down = alt;
                    app.shift_down = shift;
                    if app.capturing_hotkey {
                        continue;
                    }
                    if let Some(id) = matched_shortcut(&app, ctrl, alt, shift, &name) {
                        raise_config = apply_shortcut(&mut app, &id);
                    }
                }
                if raise_config {
                    request_config(&state);
                }
            }
        }
        let (ctrl, alt, shift) = combined_mods(&held);
        if let Ok(mut app) = state.lock() {
            app.ctrl_down = ctrl;
            app.alt_down = alt;
            app.shift_down = shift;
        }
        std::thread::sleep(Duration::from_millis(8));
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Default)]
struct ModState {
    ctrl: bool,
    alt: bool,
    shift: bool,
}

#[cfg(target_os = "linux")]
fn combined_mods(states: &[ModState]) -> (bool, bool, bool) {
    states
        .iter()
        .fold((false, false, false), |(ctrl, alt, shift), state| {
            (ctrl || state.ctrl, alt || state.alt, shift || state.shift)
        })
}

fn matched_shortcut(
    app: &AppState,
    ctrl: bool,
    alt: bool,
    shift: bool,
    key: &str,
) -> Option<String> {
    let hotkeys = &app.settings.hotkeys;
    let builtins = [
        ("toggle", hotkeys.toggle_hud.as_str()),
        ("config", hotkeys.open_config.as_str()),
        ("paste", hotkeys.paste_regex.as_str()),
        ("events_back", hotkeys.events_back.as_str()),
        ("events_forward", hotkeys.events_forward.as_str()),
        ("group_back", hotkeys.group_back.as_str()),
        ("group_forward", hotkeys.group_forward.as_str()),
    ];
    for (id, chord) in builtins {
        if chord_hit(&poe_core::parse_chord(chord), ctrl, alt, shift, key) {
            return Some(id.to_string());
        }
    }
    app.settings.hotkeys.chat.iter().find_map(|cmd| {
        if cmd.text.is_empty() || cmd.hotkey.is_empty() {
            return None;
        }
        chord_hit(&poe_core::parse_chord(&cmd.hotkey), ctrl, alt, shift, key)
            .then(|| cmd.id.clone())
    })
}

fn apply_shortcut(app: &mut AppState, id: &str) -> bool {
    match id {
        "config" => true,
        "toggle" => {
            app.hud_enabled = !app.hud_enabled;
            false
        }
        "paste" => {
            let regex = current_regex(app);
            app.status = format!("paste:{regex}");
            false
        }
        "events_back" => {
            shift_events(app, -1);
            false
        }
        "events_forward" => {
            shift_events(app, 1);
            false
        }
        "group_back" => {
            shift_groups(app, -1);
            false
        }
        "group_forward" => {
            shift_groups(app, 1);
            false
        }
        other => {
            if let Some(cmd) = app.settings.hotkeys.chat.iter().find(|cmd| cmd.id == other) {
                if !cmd.text.is_empty() {
                    let text = cmd.text.clone();
                    queue_chat(app, &text);
                }
            }
            false
        }
    }
}

fn shift_events(app: &mut AppState, delta: i32) {
    if let Some(character) = app.active_character_mut() {
        nudge_events(character, delta);
        app.dirty_characters = true;
    }
}

fn shift_groups(app: &mut AppState, delta: i32) {
    if let Some(character) = app.active_character_mut() {
        poe_core::nudge_groups(character, delta);
        app.dirty_characters = true;
    }
}

fn chord_hit(chord: &poe_core::Chord, ctrl: bool, alt: bool, shift: bool, key: &str) -> bool {
    !chord.key.is_empty()
        && chord.ctrl == ctrl
        && chord.alt == alt
        && chord.shift == shift
        && chord.key == key
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
            (
                "toggle".to_string(),
                app.settings.hotkeys.toggle_hud.clone(),
            ),
            (
                "config".to_string(),
                app.settings.hotkeys.open_config.clone(),
            ),
            (
                "paste".to_string(),
                app.settings.hotkeys.paste_regex.clone(),
            ),
            (
                "events_back".to_string(),
                app.settings.hotkeys.events_back.clone(),
            ),
            (
                "events_forward".to_string(),
                app.settings.hotkeys.events_forward.clone(),
            ),
            (
                "group_back".to_string(),
                app.settings.hotkeys.group_back.clone(),
            ),
            (
                "group_forward".to_string(),
                app.settings.hotkeys.group_forward.clone(),
            ),
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
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        let Ok(runtime) = runtime else { return };
        runtime.block_on(async move {
            if let Err(err) = portal_session(state, shortcuts).await {
                eprintln!(
                    "global shortcuts portal unavailable, using the keyboard device instead: {err}"
                );
            }
        });
    });
    Ok(())
}

#[cfg(target_os = "linux")]
async fn portal_session(
    state: Arc<Mutex<AppState>>,
    shortcuts: Vec<(String, String)>,
) -> anyhow::Result<()> {
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
        let id = event.shortcut_id().to_string();
        let raise_config = {
            let Ok(mut app) = state.lock() else { continue };
            if app.capturing_hotkey {
                continue;
            }
            apply_shortcut(&mut app, &id)
        };
        if raise_config {
            request_config(&state);
        }
    }
    Ok(())
}
