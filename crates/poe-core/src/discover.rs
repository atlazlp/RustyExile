use crate::model::{InstallKind, SavedInstall};
use anyhow::Context;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct GameLocation {
    pub kind: InstallKind,
    pub install_dir: PathBuf,
    pub log_path: PathBuf,
    pub pid: Option<u32>,
}

impl GameLocation {
    pub fn saved(&self) -> SavedInstall {
        SavedInstall {
            kind: self.kind,
            install_dir: self.install_dir.display().to_string(),
            log_path: self.log_path.display().to_string(),
        }
    }
}

pub fn detect_running() -> Option<GameLocation> {
    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    for (pid, process) in system.processes() {
        let cmd = process
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let exe = process
            .exe()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let hay = format!("{exe} {cmd}");
        if !is_poe_process(&hay) {
            continue;
        }
        let kind = if hay.contains("Steam") || hay.contains("steamapps") || hay.contains("238960") {
            InstallKind::Steam
        } else {
            InstallKind::Standalone
        };
        let install_dir = guess_install(&exe, &cmd).unwrap_or_else(|| PathBuf::from("."));
        let environ = read_environ(pid.as_u32());
        let log_path = resolve_log(&install_dir, &environ, kind);
        return Some(GameLocation {
            kind,
            install_dir,
            log_path,
            pid: Some(pid.as_u32()),
        });
    }
    None
}

pub fn is_poe_process(hay: &str) -> bool {
    if hay.contains("Path of Exile 2") || hay.contains("PathOfExile2") {
        return false;
    }
    [
        "PathOfExile_x64Steam.exe",
        "PathOfExileSteam.exe",
        "PathOfExile_x64.exe",
        "PathOfExile.exe",
    ]
    .iter()
    .any(|name| hay.contains(name))
}

fn guess_install(exe: &str, cmd: &str) -> Option<PathBuf> {
    for candidate in [exe].into_iter().chain(cmd.split([' ', '\0'])) {
        if is_poe_process(candidate) {
            let path = PathBuf::from(candidate.trim_matches('"'));
            if let Some(parent) = path.parent() {
                if parent.exists() {
                    return Some(parent.to_path_buf());
                }
            }
        }
    }
    for marker in ["steamapps/common/Path of Exile", "Grinding Gear Games/Path of Exile"] {
        if let Some(idx) = cmd.find(marker).or_else(|| exe.find(marker)) {
            let start = cmd.get(idx..).or_else(|| exe.get(idx..))?;
            let end = start.find(".exe").map(|e| e).unwrap_or(start.len());
            let path = PathBuf::from(&start[..end]);
            if let Some(parent) = path.parent() {
                return Some(parent.to_path_buf());
            }
        }
    }
    None
}

fn resolve_log(install_dir: &Path, environ: &[(String, String)], kind: InstallKind) -> PathBuf {
    let beside = install_dir.join("logs").join("Client.txt");
    if beside.is_file() {
        return beside;
    }
    if let Some(compat) = env_get(environ, "STEAM_COMPAT_DATA_PATH") {
        let proton = PathBuf::from(compat)
            .join("pfx/drive_c/users/steamuser/Documents/My Games/Path of Exile/logs/Client.txt");
        if proton.is_file() {
            return proton;
        }
    }
    if let Some(prefix) = env_get(environ, "WINEPREFIX") {
        let wine = PathBuf::from(prefix)
            .join("drive_c/users/steamuser/Documents/My Games/Path of Exile/logs/Client.txt");
        if wine.is_file() {
            return wine;
        }
    }
    if let Some(home) = directories::UserDirs::new() {
        let docs = home
            .document_dir()
            .unwrap_or(home.home_dir())
            .join("My Games/Path of Exile/logs/Client.txt");
        if docs.is_file() {
            return docs;
        }
    }
    if kind == InstallKind::Steam {
        if let Some(home) = directories::UserDirs::new() {
            let steam = home.home_dir().join(
                ".local/share/Steam/steamapps/common/Path of Exile/logs/Client.txt",
            );
            if steam.is_file() {
                return steam;
            }
        }
    }
    beside
}

fn env_get(environ: &[(String, String)], key: &str) -> Option<String> {
    environ
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .or_else(|| std::env::var(key).ok())
}

fn read_environ(pid: u32) -> Vec<(String, String)> {
    #[cfg(target_os = "linux")]
    {
        let Ok(bytes) = std::fs::read(format!("/proc/{pid}/environ")) else {
            return Vec::new();
        };
        bytes
            .split(|b| *b == 0)
            .filter_map(|entry| {
                let text = String::from_utf8_lossy(entry);
                let (k, v) = text.split_once('=')?;
                Some((k.to_string(), v.to_string()))
            })
            .collect()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        Vec::new()
    }
}

pub fn launch_game(install: Option<&SavedInstall>) -> anyhow::Result<()> {
    if let Some(install) = install {
        if install.kind == InstallKind::Standalone {
            if let Some(exe) = standalone_exe(Path::new(&install.install_dir)) {
                return spawn_program(&exe.to_string_lossy(), &[]);
            }
        }
    }
    launch_steam()
}

fn standalone_exe(dir: &Path) -> Option<PathBuf> {
    for name in ["PathOfExile_x64.exe", "PathOfExile.exe", "PathOfExile_x64Steam.exe", "PathOfExileSteam.exe"] {
        let path = dir.join(name);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn launch_steam() -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    {
        spawn_program("cmd", &["/C", "start", "steam://rungameid/238960"])
    }
    #[cfg(not(target_os = "windows"))]
    {
        spawn_program("steam", &["steam://rungameid/238960"])
            .or_else(|_| spawn_program("xdg-open", &["steam://rungameid/238960"]))
    }
}

fn spawn_program(program: &str, args: &[&str]) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                close_inherited_fds();
                Ok(())
            });
        }
        cmd.process_group(0);
    }
    cmd.spawn()
        .with_context(|| format!("could not start {program}"))?;
    Ok(())
}

#[cfg(unix)]
fn close_inherited_fds() {
    unsafe extern "C" {
        fn close_range(first: u32, last: u32, flags: i32) -> i32;
    }
    unsafe {
        let _ = close_range(3, u32::MAX, 0);
    }
}

pub fn classify_exe(path: &str) -> InstallKind {
    if path.contains("Steam") || path.contains("steamapps") {
        InstallKind::Steam
    } else if path.contains("PathOfExile") {
        InstallKind::Standalone
    } else {
        InstallKind::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_steam_and_standalone_names() {
        assert!(is_poe_process(
            "/games/Path of Exile/PathOfExile_x64Steam.exe"
        ));
        assert_eq!(
            classify_exe("C:/Program Files/steamapps/common/Path of Exile/PathOfExile_x64Steam.exe"),
            InstallKind::Steam
        );
        assert_eq!(
            classify_exe("C:/Program Files/Grinding Gear Games/Path of Exile/PathOfExile_x64.exe"),
            InstallKind::Standalone
        );
        assert!(!is_poe_process(
            "/games/Path of Exile 2/PathOfExile_x64Steam.exe"
        ));
    }
}
