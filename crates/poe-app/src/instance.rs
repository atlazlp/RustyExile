use poe_core::AppState;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub fn acquire() -> Option<UnixListener> {
    let path = socket_path();
    if ping(&path) {
        return None;
    }
    if path.exists() {
        let _ = std::fs::remove_file(&path);
    }
    match UnixListener::bind(&path) {
        Ok(listener) => Some(listener),
        Err(_) if ping(&path) => None,
        Err(_) => {
            let _ = std::fs::remove_file(&path);
            UnixListener::bind(&path).ok()
        }
    }
}

pub fn serve(listener: UnixListener, state: Arc<Mutex<AppState>>) {
    std::thread::Builder::new()
        .name("poe-instance".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = stream;
                crate::request_config(&state);
            }
        })
        .ok();
}

fn ping(path: &std::path::Path) -> bool {
    let Ok(mut stream) = UnixStream::connect(path) else {
        return false;
    };
    let _ = stream.write_all(b"config\n");
    let _ = stream.flush();
    true
}

fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("rustyexile.sock")
}
