use windows::core::{BOOL, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, IsWindowVisible, LoadImageW, SendMessageW, ICON_BIG, ICON_SMALL,
    IMAGE_ICON, LR_DEFAULTSIZE, LR_LOADFROMFILE, WM_SETICON,
};

pub fn apply_to_titled_windows(title: &str) {
    unsafe {
        let big = load_resource_icon(32)
            .or_else(|| portal_ico_path().and_then(|path| load_file_icon(&path, 32)));
        let Some(big) = big else {
            return;
        };
        let small = load_resource_icon(16)
            .or_else(|| portal_ico_path().and_then(|path| load_file_icon(&path, 16)))
            .unwrap_or(big);
        let mut state = EnumState {
            title: title.to_string(),
            big,
            small,
        };
        let _ = EnumWindows(Some(enum_proc), LPARAM(&mut state as *mut EnumState as isize));
    }
}

struct EnumState {
    title: String,
    big: isize,
    small: isize,
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let state = &mut *(lparam.0 as *mut EnumState);
    if !IsWindowVisible(hwnd).as_bool() {
        return BOOL(1);
    }
    let mut buf = [0u16; 256];
    let len = GetWindowTextW(hwnd, &mut buf);
    if len <= 0 {
        return BOOL(1);
    }
    let text = String::from_utf16_lossy(&buf[..len as usize]);
    if text != state.title {
        return BOOL(1);
    }
    let _ = SendMessageW(
        hwnd,
        WM_SETICON,
        Some(WPARAM(ICON_BIG as usize)),
        Some(LPARAM(state.big)),
    );
    let _ = SendMessageW(
        hwnd,
        WM_SETICON,
        Some(WPARAM(ICON_SMALL as usize)),
        Some(LPARAM(state.small)),
    );
    BOOL(1)
}

fn portal_ico_path() -> Option<std::path::PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside = dir.join("portal.ico");
            if beside.is_file() {
                return Some(beside);
            }
        }
    }
    let compiled = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packaging/windows/portal.ico");
    compiled.is_file().then_some(compiled)
}

unsafe fn load_resource_icon(size: i32) -> Option<isize> {
    let module = GetModuleHandleW(None).ok()?;
    let handle = LoadImageW(
        Some(module.into()),
        PCWSTR(1 as *const u16),
        IMAGE_ICON,
        size,
        size,
        LR_DEFAULTSIZE,
    )
    .ok()?;
    Some(handle.0 as isize)
}

unsafe fn load_file_icon(path: &std::path::Path, size: i32) -> Option<isize> {
    use std::os::windows::ffi::OsStrExt;
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let handle = LoadImageW(
        None,
        PCWSTR(wide.as_ptr()),
        IMAGE_ICON,
        size,
        size,
        LR_LOADFROMFILE,
    )
    .ok()?;
    Some(handle.0 as isize)
}
