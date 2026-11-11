use crate::draw::{Frame, Hit, Painter};
use poe_core::{force_complete, parse_chord, visible_lines, AppState, HudCorner};
use std::sync::{Arc, Mutex};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, SendInput, UnregisterHotKey, INPUT,
    INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, MOD_ALT, MOD_CONTROL,
    MOD_NOREPEAT, MOD_SHIFT, VIRTUAL_KEY, VK_A, VK_CONTROL, VK_MENU, VK_RETURN, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumWindows, GetCursorPos, GetForegroundWindow,
    GetWindowRect, IsWindowVisible, SetForegroundWindow,
    PeekMessageW, RegisterClassW, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage,
    UpdateLayeredWindow, CW_USEDEFAULT, GWLP_HWNDPARENT, GWL_EXSTYLE, HWND_TOP, MSG, PM_REMOVE,
    SW_HIDE, SW_SHOWNA, SWP_NOACTIVATE, ULW_ALPHA, WM_HOTKEY, WM_LBUTTONDOWN, WM_LBUTTONUP, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::Win32::Graphics::Gdi::{BLENDFUNCTION, AC_SRC_ALPHA, AC_SRC_OVER};
use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
use windows::core::BOOL;

struct ClickState {
    app: Option<Arc<Mutex<AppState>>>,
    panel: isize,
    button: isize,
    panel_hits: Vec<Hit>,
    drag: Option<(i32, i32, i32, i32)>,
}

static CLICK: Mutex<ClickState> = Mutex::new(ClickState {
    app: None,
    panel: 0,
    button: 0,
    panel_hits: Vec::new(),
    drag: None,
});

pub fn run(state: Arc<Mutex<AppState>>) -> anyhow::Result<()> {
    unsafe { run_inner(state) }
}

pub fn send_paste() -> anyhow::Result<()> {
    unsafe {
        let inputs = [
            key_input(VK_CONTROL, false),
            key_input(VK_A, false),
            key_input(VK_A, true),
            key_input(VK_V, false),
            key_input(VK_V, true),
            key_input(VK_CONTROL, true),
        ];
        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent == 0 {
            anyhow::bail!("SendInput failed");
        }
    }
    Ok(())
}

pub fn send_chat() -> anyhow::Result<()> {
    unsafe {
        let Some(hwnd) = find_poe_window() else {
            anyhow::bail!("Path of Exile is not running");
        };
        let _ = SetForegroundWindow(hwnd);
        std::thread::sleep(std::time::Duration::from_millis(90));
        tap(VK_RETURN)?;
        std::thread::sleep(std::time::Duration::from_millis(70));
        send_paste()?;
        std::thread::sleep(std::time::Duration::from_millis(40));
        tap(VK_RETURN)?;
    }
    Ok(())
}

fn tap(vk: VIRTUAL_KEY) -> anyhow::Result<()> {
    unsafe {
        let inputs = [key_input(vk, false), key_input(vk, true)];
        let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        if sent == 0 {
            anyhow::bail!("SendInput failed");
        }
    }
    Ok(())
}

fn key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

unsafe fn run_inner(state: Arc<Mutex<AppState>>) -> anyhow::Result<()> {
    let class_name = w!("rusty_exile_overlay");
    let class = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        lpszClassName: class_name,
        ..Default::default()
    };
    RegisterClassW(std::ptr::addr_of!(class));
    let panel = create_popup(class_name)?;
    let button = create_popup(class_name)?;
    if let Ok(mut click) = CLICK.lock() {
        click.app = Some(Arc::clone(&state));
        click.panel = panel.0 as isize;
        click.button = button.0 as isize;
    }
    let mut painter = Painter::new();
    let mut msg = MSG::default();
    let mut last = String::new();
    let mut hotkey_sig = String::new();
    loop {
        while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
            if msg.message == WM_HOTKEY {
                on_hotkey(msg.wParam.0 as i32);
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let Some(game) = find_poe_window() else {
            let _ = ShowWindow(panel, SW_HIDE);
            let _ = ShowWindow(button, SW_HIDE);
            std::thread::sleep(std::time::Duration::from_millis(200));
            continue;
        };
        if GetForegroundWindow() != game {
            let _ = ShowWindow(panel, SW_HIDE);
            let _ = ShowWindow(button, SW_HIDE);
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        }
        let _ = SetWindowLongPtrW(panel, GWLP_HWNDPARENT, game.0 as isize);
        let _ = SetWindowLongPtrW(button, GWLP_HWNDPARENT, game.0 as isize);
        let mut rect = RECT::default();
        if GetWindowRect(game, std::ptr::addr_of_mut!(rect)).is_err() {
            continue;
        }
        let width = (rect.right - rect.left).max(1) as u32;
        let height = (rect.bottom - rect.top).max(1) as u32;
        let mut cursor = POINT::default();
        let _ = GetCursorPos(std::ptr::addr_of_mut!(cursor));
        if let Ok(click) = CLICK.lock() {
            if let Some((sx, sy, ox, oy)) = click.drag {
                drop(click);
                if let Ok(mut app) = state.lock() {
                    if app.hud_placing {
                        app.settings.custom_x = Some((ox + cursor.x - sx).max(0));
                        app.settings.custom_y = Some((oy + cursor.y - sy).max(0));
                    } else if let Ok(mut click) = CLICK.lock() {
                        click.drag = None;
                    }
                }
            }
        }
        let near = (cursor.x - (rect.left + width as i32 / 2)).abs() < 90 && (cursor.y - rect.top).abs() < 48;
        let ctrl = GetAsyncKeyState(VK_CONTROL.0 as i32) < 0;
        let alt = GetAsyncKeyState(VK_MENU.0 as i32) < 0;
        let Ok(shared) = state.lock() else { continue };
        sync_hotkeys(panel, &shared, &mut hotkey_sig);
        if !shared.hud_enabled || !shared.settings.experience_chosen {
            drop(shared);
            let _ = ShowWindow(panel, SW_HIDE);
            let _ = ShowWindow(button, SW_HIDE);
            std::thread::sleep(std::time::Duration::from_millis(30));
            continue;
        }
        let lines = shared
            .active_character()
            .map(|character| {
                let mut lines = visible_lines(
                    &character.route,
                    &character.progress,
                    shared.settings.acts,
                    shared.settings.gems,
                    shared.settings.tips,
                );
                if shared.settings.show_landmarks {
                    lines.extend(poe_core::visible_landmarks(
                        &shared.settings.landmarks,
                        &character.progress,
                    ));
                }
                lines
            })
            .unwrap_or_default();
        let theme = shared.settings.theme.clone();
        let opacity = shared.settings.opacity;
        let scale = shared.settings.scale;
        let corner = shared.settings.corner;
        let placing = shared.hud_placing;
        let show_config_button = shared.settings.show_config_button;
        let custom = match (shared.settings.custom_x, shared.settings.custom_y) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None,
        };
        let sig = format!(
            "{}:{}:{}:{}:{near}:{ctrl}:{alt}:{}:{}:{}:{:?}",
            rect.left, rect.top, width, height, lines.len(), placing, show_config_button, custom
        );
        drop(shared);
        if sig != last {
            last = sig;
            let frame = painter.paint(
                width,
                height,
                &lines,
                &theme,
                opacity,
                scale,
                corner,
                near && show_config_button && !placing,
                custom,
                placing,
            );
            place_panel(panel, &frame, rect.left, rect.top, ctrl && alt || placing);
            place_button(button, &frame, rect.left, rect.top, near && show_config_button && !placing);
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}

unsafe fn create_popup(class_name: windows::core::PCWSTR) -> anyhow::Result<HWND> {
    Ok(CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
        class_name,
        w!("RustyExile"),
        WS_POPUP,
        CW_USEDEFAULT,
        CW_USEDEFAULT,
        100,
        100,
        None,
        None,
        None,
        None,
    )?)
}

unsafe fn sync_hotkeys(hwnd: HWND, shared: &AppState, previous: &mut String) {
    let chat = shared
        .settings
        .hotkeys
        .chat
        .iter()
        .map(|cmd| format!("{}={}", cmd.hotkey, cmd.text))
        .collect::<Vec<_>>()
        .join("|");
    let next = format!(
        "{}|{}|{}|{chat}",
        shared.settings.hotkeys.toggle_hud, shared.settings.hotkeys.open_config, shared.settings.hotkeys.paste_regex
    );
    if next == *previous {
        return;
    }
    *previous = next;
    for id in 1..=64 {
        let _ = UnregisterHotKey(Some(hwnd), id);
    }
    bind_hotkey(hwnd, 1, &shared.settings.hotkeys.toggle_hud);
    bind_hotkey(hwnd, 2, &shared.settings.hotkeys.open_config);
    bind_hotkey(hwnd, 3, &shared.settings.hotkeys.paste_regex);
    for (index, cmd) in shared.settings.hotkeys.chat.iter().enumerate() {
        if cmd.hotkey.is_empty() || cmd.text.is_empty() {
            continue;
        }
        bind_hotkey(hwnd, 10 + index as i32, &cmd.hotkey);
    }
}

unsafe fn bind_hotkey(hwnd: HWND, id: i32, chord: &str) {
    let chord = parse_chord(chord);
    let Some(vk) = virtual_key(&chord.key) else { return };
    let mut mods = MOD_NOREPEAT;
    if chord.ctrl {
        mods |= MOD_CONTROL;
    }
    if chord.alt {
        mods |= MOD_ALT;
    }
    if chord.shift {
        mods |= MOD_SHIFT;
    }
    let _ = RegisterHotKey(Some(hwnd), id, mods, vk);
}

fn virtual_key(name: &str) -> Option<u32> {
    let mut chars = name.chars();
    let ch = chars.next()?.to_ascii_uppercase();
    if chars.next().is_none() && ch.is_ascii_alphanumeric() {
        Some(ch as u32)
    } else {
        None
    }
}

fn on_hotkey(id: i32) {
    let Ok(click) = CLICK.lock() else { return };
    let Some(app) = click.app.clone() else { return };
    drop(click);
    let Ok(mut app) = app.lock() else { return };
    if app.capturing_hotkey {
        return;
    }
    match id {
        1 => app.hud_enabled = !app.hud_enabled,
        2 => app.show_config = true,
        3 => {
            let regex = poe_core::vendor_gem_regex(
                &app.active_character()
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
                    .unwrap_or_default(),
            );
            app.status = format!("paste:{regex}");
        }
            id if id >= 10 => {
            let index = (id - 10) as usize;
            if let Some(cmd) = app.settings.hotkeys.chat.get(index) {
                if !cmd.text.is_empty() && !cmd.hotkey.is_empty() {
                    app.status = format!("chat:{}", cmd.text);
                }
            }
        }
        _ => {}
    }
}

unsafe fn place_panel(hwnd: HWND, frame: &Frame, origin_x: i32, origin_y: i32, capture: bool) {
    let [x, y, w, h] = frame.panel;
    if w <= 0 || h <= 0 {
        let _ = ShowWindow(hwnd, SW_HIDE);
        return;
    }
    if let Ok(mut click) = CLICK.lock() {
        click.panel_hits = frame.hits.clone();
    }
    move_layered(hwnd, origin_x + x, origin_y + y, frame.width, frame.height, &frame.bgra);
    set_click_through(hwnd, !capture);
    let _ = ShowWindow(hwnd, SW_SHOWNA);
    let _ = SetWindowPos(hwnd, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOACTIVATE | windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE | windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE);
}

unsafe fn place_button(hwnd: HWND, frame: &Frame, origin_x: i32, origin_y: i32, near: bool) {
    let Some(button) = &frame.button else {
        let _ = ShowWindow(hwnd, SW_HIDE);
        return;
    };
    if !near {
        let _ = ShowWindow(hwnd, SW_HIDE);
        return;
    }
    move_layered(
        hwnd,
        origin_x + button.x,
        origin_y + button.y,
        button.w,
        button.h,
        &button.bgra,
    );
    set_click_through(hwnd, false);
    let _ = ShowWindow(hwnd, SW_SHOWNA);
}

unsafe fn move_layered(hwnd: HWND, x: i32, y: i32, width: u32, height: u32, bgra: &[u8]) {
    let screen = GetDC(None);
    let mem = CreateCompatibleDC(Some(screen));
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    let bitmap = CreateDIBSection(
        Some(mem),
        std::ptr::addr_of!(info),
        DIB_RGB_COLORS,
        std::ptr::addr_of_mut!(bits),
        None,
        0,
    )
    .unwrap_or_default();
    if !bitmap.0.is_null() && !bits.is_null() {
        std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, bgra.len().min((width * height * 4) as usize));
        let old = SelectObject(mem, HGDIOBJ(bitmap.0));
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let dst = POINT { x, y };
        let src = POINT { x: 0, y: 0 };
        let size = SIZE { cx: width as i32, cy: height as i32 };
        let dst_ptr = std::ptr::addr_of!(dst);
        let size_ptr = std::ptr::addr_of!(size);
        let src_ptr = std::ptr::addr_of!(src);
        let blend_ptr = std::ptr::addr_of!(blend);
        let _ = UpdateLayeredWindow(
            hwnd,
            Some(screen),
            Some(dst_ptr),
            Some(size_ptr),
            Some(mem),
            Some(src_ptr),
            COLORREF(0),
            Some(blend_ptr),
            ULW_ALPHA,
        );
        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
    }
    let _ = DeleteDC(mem);
    let _ = ReleaseDC(None, screen);
}

unsafe fn set_click_through(hwnd: HWND, enabled: bool) {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW;
    let mut style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
    if enabled {
        style |= WS_EX_TRANSPARENT.0;
    } else {
        style &= !WS_EX_TRANSPARENT.0;
    }
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
}

unsafe fn find_poe_window() -> Option<HWND> {
    let mut found = None;
    let _ = EnumWindows(Some(enum_proc), LPARAM(&mut found as *mut _ as isize));
    found
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    let mut title = [0u16; 256];
    let len = GetWindowTextW(hwnd, &mut title);
    let text = String::from_utf16_lossy(&title[..len as usize]);
    if text == "Path of Exile" {
        let slot = &mut *(lparam.0 as *mut Option<HWND>);
        *slot = Some(hwnd);
        return BOOL(0);
    }
    BOOL(1)
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_LBUTTONDOWN {
        on_click(hwnd, lparam);
    }
    if msg == WM_LBUTTONUP {
        if let Ok(mut click) = CLICK.lock() {
            click.drag = None;
        }
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

fn on_click(hwnd: HWND, lparam: LPARAM) {
    let x = (lparam.0 as i16) as i32;
    let y = ((lparam.0 >> 16) as i16) as i32;
    let Ok(click) = CLICK.lock() else { return };
    let Some(app) = click.app.clone() else { return };
    let panel = click.panel;
    let button = click.button;
    let hwnd = hwnd.0 as isize;
    let hits = click.panel_hits.clone();
    drop(click);
    let Ok(mut app) = app.lock() else { return };
    if hwnd == button {
        app.show_config = true;
        return;
    }
    if hwnd == panel {
        if app.hud_placing {
            let confirm = hits.iter().any(|hit| {
                hit.id == "confirm" && x >= hit.x && y >= hit.y && x < hit.x + hit.w && y < hit.y + hit.h
            });
            if confirm {
                if app.settings.custom_x.is_none() {
                    app.settings.custom_x = Some(0);
                    app.settings.custom_y = Some(0);
                }
                app.settings.corner = HudCorner::Custom;
                app.hud_placing = false;
                app.dirty_settings = true;
                return;
            }
            let (ox, oy) = panel_offset(panel);
            app.settings.custom_x = Some(ox);
            app.settings.custom_y = Some(oy);
            drop(app);
            let mut cursor = POINT::default();
            unsafe {
                let _ = GetCursorPos(std::ptr::addr_of_mut!(cursor));
            }
            if let Ok(mut click) = CLICK.lock() {
                click.drag = Some((cursor.x, cursor.y, ox, oy));
            }
            return;
        }
        if let Some(hit) = hits.iter().find(|hit| x >= hit.x && y >= hit.y && x < hit.x + hit.w && y < hit.y + hit.h) {
            if let Some(character) = app.active_character_mut() {
                force_complete(&mut character.progress, &hit.id);
                app.dirty_characters = true;
            }
        }
    }
}

fn panel_offset(panel: isize) -> (i32, i32) {
    unsafe {
        let mut panel_rect = RECT::default();
        let mut game_rect = RECT::default();
        let hwnd = HWND(panel as *mut std::ffi::c_void);
        let _ = GetWindowRect(hwnd, std::ptr::addr_of_mut!(panel_rect));
        if let Some(game) = find_poe_window() {
            let _ = GetWindowRect(game, std::ptr::addr_of_mut!(game_rect));
        }
        (panel_rect.left - game_rect.left, panel_rect.top - game_rect.top)
    }
}

