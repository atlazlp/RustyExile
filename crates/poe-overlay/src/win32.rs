use crate::draw::{Frame, Hit, Painter};
use crate::gemfind::{self, GemPass, GemWatch};
use poe_core::{
    force_complete, nudge_events, nudge_groups, parse_chord, visible_lines, AppState, HudCorner,
};
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::core::w;
use windows::core::{BOOL, PCWSTR};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, SetLastError, COLORREF, ERROR_ALREADY_EXISTS, HANDLE, HWND, LPARAM,
    LRESULT, POINT, RECT, SIZE, WAIT_OBJECT_0, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ, SRCCOPY,
};
use windows::Win32::Graphics::Gdi::{AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION};
use windows::Win32::System::Threading::{
    AttachThreadInput, CreateEventW, CreateMutexW, GetCurrentThreadId, OpenEventW, OpenMutexW,
    SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE, SYNCHRONIZATION_SYNCHRONIZE,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, SendInput, UnregisterHotKey, INPUT, INPUT_0, INPUT_KEYBOARD,
    KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
    VIRTUAL_KEY, VK_A, VK_CONTROL, VK_LBUTTON, VK_MENU, VK_RETURN, VK_V,
};
use windows::Win32::UI::Shell::{
    SetCurrentProcessExplicitAppUserModelID, Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP,
    NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, AppendMenuW, BringWindowToTop, CreateIcon, CreatePopupMenu,
    CreateWindowExW, DefWindowProcW, DestroyIcon, DestroyMenu, DispatchMessageW, EnumWindows,
    GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, PeekMessageW, RegisterClassW,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, TrackPopupMenu,
    TranslateMessage, UpdateLayeredWindow, ASFW_ANY, CW_USEDEFAULT, GWLP_HWNDPARENT, GWL_EXSTYLE,
    HICON, HWND_NOTOPMOST, HWND_TOPMOST, MF_STRING, MSG, PM_REMOVE, SWP_HIDEWINDOW, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_RESTORE, SW_SHOW, SW_SHOWNA, TPM_LEFTALIGN,
    TPM_RIGHTBUTTON, ULW_ALPHA, WM_APP, WM_COMMAND, WM_HOTKEY, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_RBUTTONUP, WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
    WS_POPUP,
};

const WM_TRAYICON: u32 = WM_APP + 40;
const TRAY_ID: u32 = 1;
const TRAY_CFG: usize = 1001;
const TRAY_EXIT: usize = 1002;

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

pub fn prepare_process() {
    unsafe {
        let _ = SetCurrentProcessExplicitAppUserModelID(w!("atlazlp.RustyExile"));
    }
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
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
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
    let gems = create_popup(class_name)?;
    let gem_watch = GemWatch::spawn(capture_desktop);
    let mut last_gem_submit = Instant::now() - Duration::from_secs(2);
    let mut gem_down = false;
    let tray_icon = add_tray_icon(panel);
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
            if msg.message == WM_COMMAND {
                match msg.wParam.0 & 0xffff {
                    TRAY_CFG => {
                        if let Ok(mut app) = state.lock() {
                            app.show_config = true;
                        }
                        raise_config_window();
                    }
                    TRAY_EXIT => {
                        remove_tray_icon(panel);
                        if let Some(icon) = tray_icon {
                            let _ = DestroyIcon(icon);
                        }
                        std::process::exit(0);
                    }
                    _ => {}
                }
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let Some(game) = find_poe_window() else {
            conceal(panel, button, gems, &mut last);
            std::thread::sleep(std::time::Duration::from_millis(200));
            continue;
        };
        if GetForegroundWindow() != game || IsIconic(game).as_bool() {
            conceal(panel, button, gems, &mut last);
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        }
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
        let near = (cursor.x - (rect.left + width as i32 / 2)).abs() < 90
            && (cursor.y - rect.top).abs() < 48;
        let ctrl = GetAsyncKeyState(VK_CONTROL.0 as i32) < 0;
        let alt = GetAsyncKeyState(VK_MENU.0 as i32) < 0;
        let Ok(shared) = state.lock() else { continue };
        sync_hotkeys(panel, &shared, &mut hotkey_sig);
        if !shared.hud_enabled || !shared.settings.experience_chosen {
            drop(shared);
            conceal(panel, button, gems, &mut last);
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
        let line_sig = lines
            .iter()
            .map(|line| format!("{}:{}", line.event_id, line.text))
            .collect::<Vec<_>>()
            .join("|");
        let sig = format!(
            "{}:{}:{}:{}:{near}:{ctrl}:{alt}:{opacity}:{scale}:{:?}:{line_sig}:{placing}:{show_config_button}:{custom:?}",
            rect.left, rect.top, width, height, corner
        );
        drop(shared);
        let panel_hidden = !IsWindowVisible(panel).as_bool();
        if sig != last || panel_hidden {
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
            if place_panel(panel, &frame, rect.left, rect.top, ctrl && alt || placing) {
                place_button(
                    button,
                    &frame,
                    rect.left,
                    rect.top,
                    near && show_config_button && !placing,
                );
                last = sig;
            }
        }
        let town = state.lock().ok().is_some_and(|shared| shared.in_town());
        sync_gem_window(
            gems,
            &state,
            &gem_watch,
            &mut last_gem_submit,
            rect.left,
            rect.top,
            width,
            height,
            &lines,
            town,
        );
        let pressed = GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0;
        if pressed && !gem_down {
            click_gem_spot(&state);
        }
        gem_down = pressed;
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}

fn click_gem_spot(state: &Arc<Mutex<AppState>>) {
    let mut cursor = POINT::default();
    if unsafe { GetCursorPos(std::ptr::addr_of_mut!(cursor)) }.is_err() {
        return;
    }
    if let Ok(mut shared) = state.lock() {
        shared.click_gem(cursor.x, cursor.y);
    }
}

fn sync_gem_window(
    hwnd: HWND,
    state: &Arc<Mutex<AppState>>,
    watch: &GemWatch,
    last_submit: &mut Instant,
    origin_x: i32,
    origin_y: i32,
    window_w: u32,
    window_h: u32,
    lines: &[poe_core::VisibleLine],
    town: bool,
) {
    match watch.advance(
        last_submit,
        town,
        origin_x,
        origin_y,
        window_w,
        window_h,
        lines,
    ) {
        GemPass::Idle => {
            if let Ok(mut shared) = state.lock() {
                shared.gem_spots.clear();
            }
            unsafe { hide_layered(hwnd) };
        }
        GemPass::Pending => {}
        GemPass::Ready(marks) => {
            if let Ok(mut shared) = state.lock() {
                shared.gem_spots = gemfind::spots(&marks);
            }
            let Some(frame) = gemfind::compose(&marks, poe_data::load_icon) else {
                unsafe { hide_layered(hwnd) };
                return;
            };
            unsafe {
                reveal(hwnd);
                move_layered(hwnd, frame.x, frame.y, frame.width, frame.height, &frame.bgra);
                set_click_through(hwnd, true);
                reveal(hwnd);
            }
        }
    }
}

fn capture_desktop(x: i32, y: i32, width: u32, height: u32) -> Option<image::RgbImage> {
    if width == 0 || height == 0 || width > 8000 || height > 8000 {
        return None;
    }
    unsafe { capture_desktop_inner(x, y, width, height) }
}

unsafe fn capture_desktop_inner(x: i32, y: i32, width: u32, height: u32) -> Option<image::RgbImage> {
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
    if bitmap.0.is_null() || bits.is_null() {
        let _ = DeleteDC(mem);
        let _ = ReleaseDC(None, screen);
        return None;
    }
    let old = SelectObject(mem, HGDIOBJ(bitmap.0));
    let copied = BitBlt(
        mem,
        0,
        0,
        width as i32,
        height as i32,
        Some(screen),
        x,
        y,
        SRCCOPY,
    )
    .is_ok();
    let mut rgb = vec![0u8; (width * height * 3) as usize];
    if copied {
        let src = std::slice::from_raw_parts(bits as *const u8, (width * height * 4) as usize);
        for (index, pixel) in src.chunks_exact(4).enumerate() {
            let offset = index * 3;
            rgb[offset] = pixel[2];
            rgb[offset + 1] = pixel[1];
            rgb[offset + 2] = pixel[0];
        }
    }
    SelectObject(mem, old);
    let _ = DeleteObject(HGDIOBJ(bitmap.0));
    let _ = DeleteDC(mem);
    let _ = ReleaseDC(None, screen);
    if !copied {
        return None;
    }
    image::RgbImage::from_raw(width, height, rgb)
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
        "{}|{}|{}|{}|{}|{}|{}|{chat}",
        shared.settings.hotkeys.toggle_hud,
        shared.settings.hotkeys.open_config,
        shared.settings.hotkeys.paste_regex,
        shared.settings.hotkeys.events_back,
        shared.settings.hotkeys.events_forward,
        shared.settings.hotkeys.group_back,
        shared.settings.hotkeys.group_forward,
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
    bind_hotkey(hwnd, 4, &shared.settings.hotkeys.events_back);
    bind_hotkey(hwnd, 5, &shared.settings.hotkeys.events_forward);
    bind_hotkey(hwnd, 6, &shared.settings.hotkeys.group_back);
    bind_hotkey(hwnd, 7, &shared.settings.hotkeys.group_forward);
    for (index, cmd) in shared.settings.hotkeys.chat.iter().enumerate() {
        if cmd.hotkey.is_empty() || cmd.text.is_empty() {
            continue;
        }
        bind_hotkey(hwnd, 10 + index as i32, &cmd.hotkey);
    }
}

unsafe fn bind_hotkey(hwnd: HWND, id: i32, chord: &str) {
    let chord = parse_chord(chord);
    let Some(vk) = virtual_key(&chord.key) else {
        return;
    };
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
    match name {
        "comma" => return Some(0xBC),
        "dot" | "period" => return Some(0xBE),
        "left" => return Some(0x25),
        "right" => return Some(0x27),
        "up" => return Some(0x26),
        "down" => return Some(0x28),
        _ => {}
    }
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
    let raise = id == 2;
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
        4 => shift_hotkey(&mut app, -1, false),
        5 => shift_hotkey(&mut app, 1, false),
        6 => shift_hotkey(&mut app, -1, true),
        7 => shift_hotkey(&mut app, 1, true),
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
    drop(app);
    if raise {
        raise_config_window();
    }
}

fn shift_hotkey(app: &mut AppState, delta: i32, groups: bool) {
    let Some(character) = app.active_character_mut() else {
        return;
    };
    if groups {
        nudge_groups(character, delta);
    } else {
        nudge_events(character, delta);
    }
    app.dirty_characters = true;
}

unsafe fn place_panel(
    hwnd: HWND,
    frame: &Frame,
    origin_x: i32,
    origin_y: i32,
    capture: bool,
) -> bool {
    let [x, y, w, h] = frame.panel;
    if w <= 0 || h <= 0 {
        hide_layered(hwnd);
        return true;
    }
    if let Ok(mut click) = CLICK.lock() {
        click.panel_hits = frame.hits.clone();
    }
    reveal(hwnd);
    let painted = move_layered(
        hwnd,
        origin_x + x,
        origin_y + y,
        frame.width,
        frame.height,
        &frame.bgra,
    );
    set_click_through(hwnd, !capture);
    reveal(hwnd);
    painted && IsWindowVisible(hwnd).as_bool()
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
    reveal(hwnd);
    move_layered(
        hwnd,
        origin_x + button.x,
        origin_y + button.y,
        button.w,
        button.h,
        &button.bgra,
    );
    set_click_through(hwnd, false);
    reveal(hwnd);
}

unsafe fn conceal(panel: HWND, button: HWND, gems: HWND, last: &mut String) {
    if last.is_empty()
        && !IsWindowVisible(panel).as_bool()
        && !IsWindowVisible(button).as_bool()
        && !IsWindowVisible(gems).as_bool()
    {
        return;
    }
    last.clear();
    hide_layered(panel);
    hide_layered(button);
    hide_layered(gems);
}

unsafe fn hide_layered(hwnd: HWND) {
    let _ = SetWindowPos(
        hwnd,
        Some(HWND_NOTOPMOST),
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_HIDEWINDOW,
    );
}

unsafe fn reveal(hwnd: HWND) {
    // Owning the game window gets this popup cloaked on alt-tab. On some
    // machines it then stays invisible until Rusty is focused and the game is clicked.
    if GetWindowLongPtrW(hwnd, GWLP_HWNDPARENT) != 0 {
        SetWindowLongPtrW(hwnd, GWLP_HWNDPARENT, 0);
    }
    let _ = ShowWindow(hwnd, SW_SHOWNA);
    let _ = SetWindowPos(
        hwnd,
        Some(HWND_TOPMOST),
        0,
        0,
        0,
        0,
        SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
    );
}

unsafe fn move_layered(hwnd: HWND, x: i32, y: i32, width: u32, height: u32, bgra: &[u8]) -> bool {
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
        std::ptr::copy_nonoverlapping(
            bgra.as_ptr(),
            bits as *mut u8,
            bgra.len().min((width * height * 4) as usize),
        );
        let old = SelectObject(mem, HGDIOBJ(bitmap.0));
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let dst = POINT { x, y };
        let src = POINT { x: 0, y: 0 };
        let size = SIZE {
            cx: width as i32,
            cy: height as i32,
        };
        let dst_ptr = std::ptr::addr_of!(dst);
        let size_ptr = std::ptr::addr_of!(size);
        let src_ptr = std::ptr::addr_of!(src);
        let blend_ptr = std::ptr::addr_of!(blend);
        let painted = UpdateLayeredWindow(
            hwnd,
            Some(screen),
            Some(dst_ptr),
            Some(size_ptr),
            Some(mem),
            Some(src_ptr),
            COLORREF(0),
            Some(blend_ptr),
            ULW_ALPHA,
        )
        .is_ok();
        SelectObject(mem, old);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(mem);
        let _ = ReleaseDC(None, screen);
        return painted;
    }
    let _ = DeleteDC(mem);
    let _ = ReleaseDC(None, screen);
    false
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

static LIFT_GEN: AtomicU64 = AtomicU64::new(0);
static CONFIG_WANTED: AtomicBool = AtomicBool::new(false);

pub fn raise_config_window() {
    CONFIG_WANTED.store(true, Ordering::Relaxed);
    let ticket = LIFT_GEN.fetch_add(1, Ordering::Relaxed) + 1;
    std::thread::Builder::new()
        .name("poe-config-raise".into())
        .spawn(move || {
            let started = Instant::now();
            let mut placed = false;
            while started.elapsed() < Duration::from_millis(1_500) {
                if !config_lift_current(ticket) {
                    return;
                }
                unsafe {
                    present_config(ticket, &mut placed);
                }
                if !config_lift_current(ticket) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(40));
            }
        })
        .ok();
}

pub fn game_is_open() -> bool {
    unsafe { find_poe_window().is_some() }
}

pub fn dismiss_config_window() {
    CONFIG_WANTED.store(false, Ordering::Relaxed);
    LIFT_GEN.fetch_add(1, Ordering::Relaxed);
    unsafe {
        if let Some(hwnd) = find_config_window() {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_NOTOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        if let Some(game) = find_poe_window() {
            let _ = SetForegroundWindow(game);
        }
    }
}

fn config_lift_current(ticket: u64) -> bool {
    CONFIG_WANTED.load(Ordering::Relaxed) && LIFT_GEN.load(Ordering::Relaxed) == ticket
}

unsafe fn present_config(ticket: u64, placed: &mut bool) {
    let Some(hwnd) = find_config_window() else {
        return;
    };
    if !config_lift_current(ticket) {
        return;
    }
    let visible = IsWindowVisible(hwnd).as_bool();
    let iconic = IsIconic(hwnd).as_bool();
    let foreground = GetForegroundWindow() == hwnd;
    if visible && !iconic && foreground && *placed {
        return;
    }
    if iconic || !visible {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = ShowWindow(hwnd, SW_SHOW);
    }
    if let Some(game) = find_poe_window() {
        if !*placed {
            *placed = move_over_game(hwnd, game);
        }
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
    } else {
        *placed = true;
    }
    if GetForegroundWindow() != hwnd {
        force_foreground(hwnd);
    }
    if CONFIG_WANTED.load(Ordering::Relaxed) {
        return;
    }
    let _ = SetWindowPos(
        hwnd,
        Some(HWND_NOTOPMOST),
        0,
        0,
        0,
        0,
        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
    );
    let _ = ShowWindow(hwnd, SW_HIDE);
    if let Some(game) = find_poe_window() {
        let _ = SetForegroundWindow(game);
    }
}

unsafe fn move_over_game(config: HWND, game: HWND) -> bool {
    let mut game_rect = RECT::default();
    let mut config_rect = RECT::default();
    if GetWindowRect(game, std::ptr::addr_of_mut!(game_rect)).is_err()
        || GetWindowRect(config, std::ptr::addr_of_mut!(config_rect)).is_err()
    {
        return false;
    }
    let game_w = game_rect.right - game_rect.left;
    let game_h = game_rect.bottom - game_rect.top;
    let window_w = config_rect.right - config_rect.left;
    let window_h = config_rect.bottom - config_rect.top;
    if game_rect.left < -10_000
        || game_rect.top < -10_000
        || game_w < 200
        || game_h < 200
        || window_w < 400
        || window_h < 300
    {
        return false;
    }
    let x = if window_w >= game_w {
        game_rect.left
    } else {
        game_rect.left + (game_w - window_w) / 2
    };
    let y = if window_h >= game_h {
        game_rect.top
    } else {
        game_rect.top + (game_h - window_h) / 2
    };
    SetWindowPos(
        config,
        Some(HWND_TOPMOST),
        x,
        y,
        0,
        0,
        SWP_NOSIZE | SWP_SHOWWINDOW,
    )
    .is_ok()
}

unsafe fn force_foreground(hwnd: HWND) {
    let foreground = GetForegroundWindow();
    let fore_thread = GetWindowThreadProcessId(foreground, None);
    let this_thread = GetCurrentThreadId();
    let attached = fore_thread != 0
        && fore_thread != this_thread
        && AttachThreadInput(this_thread, fore_thread, true).as_bool();
    let _ = BringWindowToTop(hwnd);
    let _ = SetForegroundWindow(hwnd);
    if attached {
        let _ = AttachThreadInput(this_thread, fore_thread, false);
    }
}

unsafe fn find_config_window() -> Option<HWND> {
    let mut found = None;
    let _ = EnumWindows(
        Some(enum_config_proc),
        LPARAM(&mut found as *mut _ as isize),
    );
    found
}

unsafe extern "system" fn enum_config_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
    if ex & WS_EX_TOOLWINDOW.0 != 0 {
        return BOOL(1);
    }
    let mut class_name = [0u16; 64];
    let class_len = GetClassNameW(hwnd, &mut class_name);
    if class_len > 0 {
        let class = String::from_utf16_lossy(&class_name[..class_len as usize]);
        if class == "rusty_exile_overlay" {
            return BOOL(1);
        }
    }
    let mut title = [0u16; 256];
    let len = GetWindowTextW(hwnd, &mut title);
    if len <= 0 {
        return BOOL(1);
    }
    let text = String::from_utf16_lossy(&title[..len as usize]);
    if text == "RustyExile" {
        let slot = &mut *(lparam.0 as *mut Option<HWND>);
        *slot = Some(hwnd);
        return BOOL(0);
    }
    BOOL(1)
}

unsafe fn find_poe_window() -> Option<HWND> {
    let foreground = GetForegroundWindow();
    if is_game_window(foreground) {
        return Some(foreground);
    }
    let mut found = None;
    let _ = EnumWindows(Some(enum_proc), LPARAM(&mut found as *mut _ as isize));
    found
}

unsafe fn is_game_window(hwnd: HWND) -> bool {
    if hwnd.0.is_null() || !IsWindowVisible(hwnd).as_bool() {
        return false;
    }
    if window_title(hwnd) != "Path of Exile" {
        return false;
    }
    let class = window_class(hwnd);
    class != "Chrome_WidgetWin_1" && class != "MozillaWindowClass"
}

unsafe fn window_title(hwnd: HWND) -> String {
    let mut title = [0u16; 256];
    let len = GetWindowTextW(hwnd, &mut title);
    if len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&title[..len as usize])
}

unsafe fn window_class(hwnd: HWND) -> String {
    let mut class_name = [0u16; 64];
    let len = GetClassNameW(hwnd, &mut class_name);
    if len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&class_name[..len as usize])
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if is_game_window(hwnd) {
        let slot = &mut *(lparam.0 as *mut Option<HWND>);
        *slot = Some(hwnd);
        return BOOL(0);
    }
    BOOL(1)
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_TRAYICON {
        let mouse = lparam.0 as u32;
        if mouse == WM_LBUTTONUP {
            if let Ok(click) = CLICK.lock() {
                if let Some(app) = &click.app {
                    if let Ok(mut state) = app.lock() {
                        state.show_config = true;
                    }
                }
            }
            raise_config_window();
        } else if mouse == WM_RBUTTONUP {
            show_tray_menu(hwnd);
        }
        return LRESULT(0);
    }
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

unsafe fn add_tray_icon(hwnd: HWND) -> Option<HICON> {
    let icon = create_badge_hicon(32)?;
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAYICON,
        hIcon: icon,
        ..Default::default()
    };
    let tip: Vec<u16> = "RustyExile\0".encode_utf16().collect();
    let copy = tip.len().min(nid.szTip.len());
    nid.szTip[..copy].copy_from_slice(&tip[..copy]);
    if Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
        Some(icon)
    } else {
        let _ = DestroyIcon(icon);
        None
    }
}

unsafe fn remove_tray_icon(hwnd: HWND) {
    let nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        ..Default::default()
    };
    let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
}

unsafe fn show_tray_menu(hwnd: HWND) {
    let Ok(menu) = CreatePopupMenu() else {
        return;
    };
    let _ = AppendMenuW(menu, MF_STRING, TRAY_CFG, w!("Config"));
    let _ = AppendMenuW(menu, MF_STRING, TRAY_EXIT, w!("Exit Rusty"));
    let mut cursor = POINT::default();
    let _ = GetCursorPos(std::ptr::addr_of_mut!(cursor));
    let _ = SetForegroundWindow(hwnd);
    let _ = TrackPopupMenu(
        menu,
        TPM_LEFTALIGN | TPM_RIGHTBUTTON,
        cursor.x,
        cursor.y,
        Some(0),
        hwnd,
        None,
    );
    let _ = DestroyMenu(menu);
}

unsafe fn create_badge_hicon(size: u32) -> Option<HICON> {
    let (width, height, rgba) = poe_data::window_icon_rgba(size)?;
    let pixel_count = (width * height) as usize;
    let mut bgra = rgba;
    let mut mask = Vec::with_capacity(pixel_count);
    for pixel in bgra.chunks_exact_mut(4) {
        mask.push(if pixel[3] < 128 { 0xff } else { 0 });
        pixel.swap(0, 2);
    }
    let handle = CreateIcon(
        None,
        width as i32,
        height as i32,
        1,
        32,
        mask.as_ptr(),
        bgra.as_ptr(),
    )
    .ok()?;
    Some(handle)
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
        drop(app);
        raise_config_window();
        return;
    }
    if hwnd == panel {
        if app.hud_placing {
            let confirm = hits.iter().any(|hit| {
                hit.id == "confirm"
                    && x >= hit.x
                    && y >= hit.y
                    && x < hit.x + hit.w
                    && y < hit.y + hit.h
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
        if let Some(hit) = hits
            .iter()
            .find(|hit| x >= hit.x && y >= hit.y && x < hit.x + hit.w && y < hit.y + hit.h)
        {
            if let Some(character) = app.active_character_mut() {
                force_complete(&mut character.progress, &hit.id);
                app.dirty_characters = true;
            }
        }
    }
}

const INSTANCE_MUTEX: &str = "Local\\RustyExile.Instance";
const INSTANCE_EVENT: &str = "Local\\RustyExile.Activate";

pub struct SingleInstance {
    mutex: isize,
    event: isize,
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(owned_handle(self.mutex));
            let _ = CloseHandle(owned_handle(self.event));
        }
    }
}

fn owned_handle(raw: isize) -> HANDLE {
    HANDLE(raw as *mut std::ffi::c_void)
}

pub fn claim_single_instance() -> Option<SingleInstance> {
    claim_named(INSTANCE_MUTEX, INSTANCE_EVENT)
}

pub fn watch_single_instance(instance: SingleInstance, on_activate: impl Fn() + Send + 'static) {
    std::thread::Builder::new()
        .name("poe-instance".into())
        .spawn(move || {
            let held = instance;
            loop {
                let status = unsafe { WaitForSingleObject(owned_handle(held.event), u32::MAX) };
                if status != WAIT_OBJECT_0 {
                    break;
                }
                on_activate();
            }
        })
        .ok();
}

fn claim_named(mutex_name: &str, event_name: &str) -> Option<SingleInstance> {
    let mutex_wide = wide_name(mutex_name);
    let event_wide = wide_name(event_name);
    unsafe {
        if open_mutex(&mutex_wide) {
            signal_existing(&event_wide);
            return None;
        }
        SetLastError(windows::Win32::Foundation::WIN32_ERROR(0));
        let mutex = CreateMutexW(None, false, PCWSTR(mutex_wide.as_ptr())).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(mutex);
            signal_existing(&event_wide);
            return None;
        }
        SetLastError(windows::Win32::Foundation::WIN32_ERROR(0));
        let event = CreateEventW(None, false, false, PCWSTR(event_wide.as_ptr())).ok()?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let _ = CloseHandle(mutex);
            let _ = CloseHandle(event);
            signal_existing(&event_wide);
            return None;
        }
        Some(SingleInstance {
            mutex: mutex.0 as isize,
            event: event.0 as isize,
        })
    }
}

fn wide_name(name: &str) -> Vec<u16> {
    name.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn open_mutex(name: &[u16]) -> bool {
    match OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, PCWSTR(name.as_ptr())) {
        Ok(handle) => {
            let _ = CloseHandle(handle);
            true
        }
        Err(_) => false,
    }
}

unsafe fn signal_existing(event_name: &[u16]) {
    allow_existing_foreground();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        if let Ok(event) = OpenEventW(EVENT_MODIFY_STATE, false, PCWSTR(event_name.as_ptr())) {
            let _ = SetEvent(event);
            let _ = CloseHandle(event);
            return;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

unsafe fn allow_existing_foreground() {
    let mut pid = 0u32;
    if let Some(hwnd) = find_config_window() {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    if pid == 0 {
        let _ = AllowSetForegroundWindow(ASFW_ANY);
    } else {
        let _ = AllowSetForegroundWindow(pid);
    }
}

#[cfg(test)]
mod instance_tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn second_launch_wakes_the_running_instance() {
        let id = std::process::id();
        let mutex = format!("Local\\RustyExile.Test.{id}");
        let event = format!("Local\\RustyExile.Test.Activate.{id}");
        let primary = claim_named(&mutex, &event).expect("primary instance");
        let hits = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&hits);
        watch_single_instance(primary, move || {
            counted.fetch_add(1, Ordering::SeqCst);
        });
        assert!(claim_named(&mutex, &event).is_none());
        let started = Instant::now();
        while hits.load(Ordering::SeqCst) == 0 && started.elapsed() < Duration::from_secs(2) {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(hits.load(Ordering::SeqCst), 1);
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
        (
            panel_rect.left - game_rect.left,
            panel_rect.top - game_rect.top,
        )
    }
}
