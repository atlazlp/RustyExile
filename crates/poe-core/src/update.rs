use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const REPO: &str = "atlazlp/RustyExile";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    UpToDate,
    NoRelease,
    Installed { version: String },
    Failed(String),
}

#[derive(Debug)]
struct ReleaseAsset {
    version: String,
    download_url: String,
}

pub fn check_for_update() -> UpdateOutcome {
    match fetch_newer_release() {
        Ok(None) => UpdateOutcome::NoRelease,
        Ok(Some(release)) if !is_newer(&release.version, VERSION) => UpdateOutcome::UpToDate,
        Ok(Some(release)) => match download_and_install(&release) {
            Ok(()) => UpdateOutcome::Installed {
                version: release.version,
            },
            Err(err) => UpdateOutcome::Failed(err.to_string()),
        },
        Err(err) => UpdateOutcome::Failed(err.to_string()),
    }
}

pub fn install_archive(
    bytes: &[u8],
    exe: &Path,
    asset_dir: &Path,
    protected: &Path,
) -> anyhow::Result<()> {
    let _pending = pending_guard();
    ensure_outside(exe, protected)?;
    ensure_outside(asset_dir, protected)?;
    clear_pending_update();
    let temp = std::env::temp_dir().join(format!("rusty-update-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).context("could not prepare the update")?;
    let result = (|| {
        extract_zip(bytes, &temp)?;
        let packed = temp.join(binary_name());
        if !packed.is_file() {
            anyhow::bail!("the release archive has no {name}", name = binary_name());
        }
        #[cfg(windows)]
        if !dir_writable(exe.parent()) {
            remember_pending_update(&temp, exe, asset_dir)?;
            return Ok(());
        }
        let assets = temp.join("assets");
        if assets.is_dir() {
            swap_tree(&assets, asset_dir, protected)?;
        }
        replace_executable(&packed, exe)?;
        Ok(())
    })();
    if result.is_err() || !update_needs_permission() {
        let _ = fs::remove_dir_all(&temp);
    }
    result
}

pub fn update_needs_permission() -> bool {
    pending_update_path().is_file()
}

pub fn restart_installed() -> anyhow::Result<()> {
    let exe = running_program()?;
    #[cfg(windows)]
    {
        if let Some(pending) = read_pending_update() {
            match launch_elevated_apply(&pending) {
                Ok(()) => std::process::exit(0),
                Err(err) => {
                    clear_pending_update();
                    return Err(err);
                }
            }
        }
        let pending = pending_binary(&exe);
        let pid = std::process::id();
        let new = powershell_quote(&pending);
        let dest = powershell_quote(&exe);
        let command = format!(
            "$ErrorActionPreference = 'Stop'; $deadline = (Get-Date).AddSeconds(20); while (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ if ((Get-Date) -gt $deadline) {{ exit 1 }}; Start-Sleep -Milliseconds 200 }}; $ok = $false; for ($i = 0; $i -lt 40; $i++) {{ try {{ Move-Item -Force -LiteralPath {new} -Destination {dest}; $ok = $true; break }} catch {{ Start-Sleep -Milliseconds 250 }} }}; if (-not $ok) {{ exit 1 }}; Start-Process -FilePath {dest}"
        );
        spawn_hidden_powershell([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
            command.as_str(),
        ])
        .context("could not start the Windows updater")?;
        std::process::exit(0);
    }
    #[cfg(unix)]
    {
        spawn_unix_restart(&exe)?;
        std::process::exit(0);
    }
    #[cfg(not(any(windows, unix)))]
    {
        std::process::Command::new(&exe)
            .spawn()
            .context("could not start the updated program")?;
        std::process::exit(0);
    }
}

fn running_program() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("could not find the running program")?;
    Ok(strip_deleted(&exe))
}

fn strip_deleted(path: &Path) -> PathBuf {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return path.to_path_buf();
    };
    let Some(clean) = name.strip_suffix(" (deleted)") else {
        return path.to_path_buf();
    };
    if clean.is_empty() {
        return path.to_path_buf();
    }
    path.with_file_name(clean)
}

#[cfg(any(unix, test))]
fn unix_restart_script() -> &'static str {
    "i=0; while kill -0 \"$1\" 2>/dev/null; do i=$((i+1)); if [ \"$i\" -gt 150 ]; then exit 1; fi; sleep 0.2; done; exec \"$2\""
}

#[cfg(unix)]
fn spawn_unix_restart(exe: &Path) -> anyhow::Result<()> {
    spawn_unix_restart_for(exe, std::process::id())
}

#[cfg(unix)]
fn spawn_unix_restart_for(exe: &Path, pid: u32) -> anyhow::Result<()> {
    if !exe.is_file() {
        anyhow::bail!("the updated program is missing");
    }
    set_executable(exe)?;
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c")
        .arg(unix_restart_script())
        .arg("rusty-restart")
        .arg(pid.to_string())
        .arg(exe)
        .stdin(std::process::Stdio::null());
    if let Some(dir) = exe.parent() {
        cmd.current_dir(dir);
    }
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
    cmd.spawn()
        .context("could not start the updated program")?;
    Ok(())
}

fn fetch_newer_release() -> anyhow::Result<Option<ReleaseAsset>> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .build();
    let response = agent
        .get(&url)
        .set("User-Agent", &format!("RustyExile/{VERSION}"))
        .set("Accept", "application/vnd.github+json")
        .call();
    let response = match response {
        Ok(response) => response,
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let body = response.into_string()?;
    let value: serde_json::Value = serde_json::from_str(&body)?;
    let version = value
        .get("tag_name")
        .and_then(|tag| tag.as_str())
        .unwrap_or_default()
        .trim()
        .trim_start_matches('v')
        .to_string();
    if version.is_empty() || !is_newer(&version, VERSION) {
        return Ok(Some(ReleaseAsset {
            version,
            download_url: String::new(),
        }));
    }
    let asset_name = release_asset_name();
    let download_url = value
        .get("assets")
        .and_then(|assets| assets.as_array())
        .and_then(|assets| {
            assets.iter().find_map(|asset| {
                let name = asset.get("name").and_then(|name| name.as_str())?;
                if name != asset_name {
                    return None;
                }
                asset
                    .get("browser_download_url")
                    .and_then(|url| url.as_str())
                    .map(str::to_string)
            })
        })
        .with_context(|| format!("the latest release has no {asset_name}"))?;
    Ok(Some(ReleaseAsset {
        version,
        download_url,
    }))
}

fn download_and_install(release: &ReleaseAsset) -> anyhow::Result<()> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(120))
        .build();
    let response = agent
        .get(&release.download_url)
        .set("User-Agent", &format!("RustyExile/{VERSION}"))
        .call()
        .with_context(|| format!("could not download {}", release.version))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(200 * 1024 * 1024)
        .read_to_end(&mut bytes)?;
    let exe = running_program()?;
    let asset_dir = exe
        .parent()
        .context("the program has no folder")?
        .join("assets");
    install_archive(&bytes, &exe, &asset_dir, &crate::store::root_dir())
}

fn extract_zip(bytes: &[u8], dest: &Path) -> anyhow::Result<()> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).context("the release is not a zip archive")?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let Some(name) = file.enclosed_name() else {
            continue;
        };
        let path = dest.join(name);
        if file.is_dir() {
            fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(&path)?;
        std::io::copy(&mut file, &mut output)?;
    }
    Ok(())
}

fn swap_tree(from: &Path, dest: &Path, protected: &Path) -> anyhow::Result<()> {
    let staging = dest.with_extension("new");
    let backup = dest.with_extension("old");
    ensure_outside(&staging, protected)?;
    ensure_outside(&backup, protected)?;
    clear_path(&staging)?;
    clear_path(&backup)?;
    if let Err(err) = copy_tree(from, &staging, protected) {
        let _ = remove_tree(&staging);
        return Err(err);
    }
    if dest.exists() {
        fs::rename(dest, &backup)
            .with_context(|| format!("could not replace {}", dest.display()))?;
    }
    if let Err(err) = fs::rename(&staging, dest) {
        if backup.exists() {
            let _ = fs::rename(&backup, dest);
        }
        let _ = remove_tree(&staging);
        return Err(err).with_context(|| format!("could not replace {}", dest.display()));
    }
    if backup.exists() {
        let _ = remove_tree(&backup);
    }
    Ok(())
}

fn clear_path(path: &Path) -> anyhow::Result<()> {
    if path.is_dir() {
        remove_tree(path).with_context(|| format!("could not replace {}", path.display()))?;
    } else if path.exists() {
        fs::remove_file(path).with_context(|| format!("could not replace {}", path.display()))?;
    }
    Ok(())
}

fn copy_tree(from: &Path, to: &Path, protected: &Path) -> anyhow::Result<()> {
    ensure_outside(to, protected)?;
    fs::create_dir_all(to).with_context(|| format!("could not create {}", to.display()))?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        ensure_outside(&target, protected)?;
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target, protected)?;
        } else {
            retry_io(|| fs::copy(entry.path(), &target).map(|_| ()))
                .with_context(|| format!("could not copy {}", target.display()))?;
        }
    }
    Ok(())
}

fn replace_executable(from: &Path, exe: &Path) -> anyhow::Result<()> {
    if let Some(parent) = exe.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    #[cfg(windows)]
    let dest = pending_binary(exe);
    #[cfg(not(windows))]
    let dest = exe.to_path_buf();
    let staging = exe.with_file_name(".RustyExile.staging");
    retry_io(|| fs::copy(from, &staging).map(|_| ()))
        .with_context(|| format!("could not stage {}", staging.display()))?;
    set_executable(&staging)?;
    if dest != exe && dest.exists() {
        retry_io(|| fs::remove_file(&dest))
            .with_context(|| format!("could not replace {}", dest.display()))?;
    }
    retry_io(|| fs::rename(&staging, &dest))
        .with_context(|| format!("could not replace {}", dest.display()))?;
    Ok(())
}

fn remove_tree(path: &Path) -> std::io::Result<()> {
    retry_io(|| match fs::remove_dir_all(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    })
}

fn retry_io(mut op: impl FnMut() -> std::io::Result<()>) -> std::io::Result<()> {
    let mut delay = Duration::from_millis(50);
    let mut last = Ok(());
    for attempt in 0..8 {
        match op() {
            Ok(()) => return Ok(()),
            Err(err) if attempt + 1 < 8 && is_transient_lock(&err) => {
                last = Err(err);
                std::thread::sleep(delay);
                delay = (delay * 2).min(Duration::from_millis(400));
            }
            Err(err) => return Err(err),
        }
    }
    last
}

fn is_transient_lock(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::PermissionDenied
        || matches!(err.raw_os_error(), Some(5) | Some(32))
}

fn pending_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|err| err.into_inner())
}

#[cfg_attr(not(windows), allow(dead_code))]
fn dir_writable(dir: Option<&Path>) -> bool {
    let Some(dir) = dir else {
        return false;
    };
    if !dir.is_dir() && fs::create_dir_all(dir).is_err() {
        return false;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let probe = dir.join(format!(".rusty-write-{}-{stamp}", std::process::id()));
    match fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
    {
        Ok(file) => {
            drop(file);
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PendingUpdate {
    stage: PathBuf,
    exe: PathBuf,
    assets: PathBuf,
}

fn pending_update_path() -> PathBuf {
    std::env::temp_dir().join("RustyExile-pending-update.json")
}

#[cfg(windows)]
fn remember_pending_update(stage: &Path, exe: &Path, assets: &Path) -> anyhow::Result<()> {
    let pending = PendingUpdate {
        stage: stage.to_path_buf(),
        exe: exe.to_path_buf(),
        assets: assets.to_path_buf(),
    };
    let text = serde_json::to_string(&pending)?;
    fs::write(pending_update_path(), text).context("could not prepare the update")?;
    Ok(())
}

fn read_pending_update() -> Option<PendingUpdate> {
    let text = fs::read_to_string(pending_update_path()).ok()?;
    serde_json::from_str(&text).ok()
}

fn clear_pending_update() {
    if let Some(pending) = read_pending_update() {
        if pending.stage.starts_with(std::env::temp_dir()) {
            let _ = fs::remove_dir_all(&pending.stage);
        }
    }
    let _ = fs::remove_file(pending_update_path());
}

#[cfg(windows)]
fn launch_elevated_apply(pending: &PendingUpdate) -> anyhow::Result<()> {
    let dir = std::env::temp_dir().join(format!("rusty-update-launch-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)?;
    let apply = dir.join("apply.ps1");
    let launch = dir.join("launch.ps1");
    write_powershell(
        &apply,
        &apply_script(
            std::process::id(),
            &pending.stage,
            &pending.exe,
            &pending.assets,
            &dir,
            true,
        ),
    )?;
    write_powershell(&launch, &launch_script(&apply))?;
    let output = powershell_command()
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(&launch)
        .output()
        .context("could not start the Windows updater")?;
    if output.status.success() {
        return Ok(());
    }
    let _ = fs::remove_dir_all(&dir);
    anyhow::bail!("Windows denied access to the install folder. Allow the permission prompt and try the update again.")
}

#[cfg(windows)]
fn apply_script(
    pid: u32,
    stage: &Path,
    exe: &Path,
    assets: &Path,
    scripts: &Path,
    launch: bool,
) -> String {
    let launch_step = if launch {
        format!(
            "Remove-Item -LiteralPath {} -Force -ErrorAction SilentlyContinue\nStart-Unelevated $exe\nStart-Sleep -Milliseconds 500\nRemove-Item -LiteralPath $scripts -Recurse -Force -ErrorAction SilentlyContinue\n",
            powershell_quote(&pending_update_path())
        )
    } else {
        String::new()
    };
    let body = r#"$ErrorActionPreference = 'Stop'
function Start-Unelevated($path) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = Join-Path $env:SystemRoot 'System32\runas.exe'
    $psi.Arguments = '/trustlevel:0x20000 "' + $path + '"'
    $psi.UseShellExecute = $false
    $psi.WindowStyle = 'Hidden'
    [void][Diagnostics.Process]::Start($psi)
}
$targetPid = %%PID%%
$deadline = (Get-Date).AddMinutes(2)
while (Get-Process -Id $targetPid -ErrorAction SilentlyContinue) {
    if ((Get-Date) -gt $deadline) { exit 1 }
    Start-Sleep -Milliseconds 200
}
Start-Sleep -Milliseconds 400
$stage = %%STAGE%%
$exe = %%EXE%%
$assets = %%ASSETS%%
$scripts = %%SCRIPTS%%
$packed = Join-Path $stage 'RustyExile.exe'
$stagedAssets = Join-Path $stage 'assets'
$destDir = Split-Path -Parent $exe
New-Item -ItemType Directory -Force -Path $destDir | Out-Null
if (Test-Path -LiteralPath $stagedAssets) {
    & robocopy.exe $stagedAssets $assets /MIR /R:10 /W:1 /NFL /NDL /NJH /NJS /nc /ns /np | Out-Null
    if ($LASTEXITCODE -ge 8) {
        Start-Unelevated $exe
        exit 1
    }
}
$copied = $false
for ($i = 0; $i -lt 40; $i++) {
    try {
        Copy-Item -LiteralPath $packed -Destination $exe -Force
        $copied = $true
        break
    } catch {
        Start-Sleep -Milliseconds 250
    }
}
if (-not $copied) {
    Start-Unelevated $exe
    exit 1
}
Remove-Item -LiteralPath $stage -Recurse -Force -ErrorAction SilentlyContinue
%%LAUNCH%%"#;
    body.replace("%%PID%%", &pid.to_string())
        .replace("%%STAGE%%", &powershell_quote(stage))
        .replace("%%EXE%%", &powershell_quote(exe))
        .replace("%%ASSETS%%", &powershell_quote(assets))
        .replace("%%SCRIPTS%%", &powershell_quote(scripts))
        .replace("%%LAUNCH%%", &launch_step)
}

#[cfg(windows)]
fn launch_script(apply: &Path) -> String {
    format!(
        r#"$ErrorActionPreference = 'Stop'
$psi = New-Object System.Diagnostics.ProcessStartInfo
$psi.FileName = {ps}
$psi.Arguments = '-NoProfile -ExecutionPolicy Bypass -File "{apply}"'
$psi.Verb = 'runas'
$psi.UseShellExecute = $true
$psi.WindowStyle = 'Hidden'
[void][Diagnostics.Process]::Start($psi)
"#,
        ps = powershell_quote(&powershell_exe()),
        apply = apply.display().to_string().replace('\'', "''")
    )
}

#[cfg(windows)]
fn write_powershell(path: &Path, body: &str) -> anyhow::Result<()> {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend(body.as_bytes());
    fs::write(path, bytes).with_context(|| format!("could not prepare {}", path.display()))?;
    Ok(())
}

#[cfg(windows)]
fn powershell_exe() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe")
}

#[cfg(windows)]
fn powershell_command() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    let mut command = std::process::Command::new(powershell_exe());
    command.creation_flags(0x08000000);
    command
}

#[cfg(windows)]
fn spawn_hidden_powershell<I, S>(args: I) -> std::io::Result<std::process::Child>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    powershell_command().args(args).spawn()
}

#[cfg(windows)]
fn pending_binary(exe: &Path) -> PathBuf {
    let name = exe
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("RustyExile.exe");
    exe.with_file_name(format!("{name}.new"))
}

fn set_executable(path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }
    let _ = path;
    Ok(())
}

fn ensure_outside(path: &Path, protected: &Path) -> anyhow::Result<()> {
    let path = normalize(path);
    let protected = normalize(protected);
    if is_inside(&path, &protected) {
        anyhow::bail!("refusing to change user data in {}", path.display());
    }
    Ok(())
}

fn is_inside(path: &Path, protected: &Path) -> bool {
    let path: Vec<_> = path.components().map(component_key).collect();
    let protected: Vec<_> = protected.components().map(component_key).collect();
    !protected.is_empty() && path.starts_with(&protected)
}

fn component_key(component: std::path::Component<'_>) -> String {
    let text = component.as_os_str().to_string_lossy();
    #[cfg(windows)]
    {
        return text.to_lowercase();
    }
    #[cfg(not(windows))]
    {
        text.into_owned()
    }
}

fn normalize(path: &Path) -> PathBuf {
    let resolved = path.canonicalize().unwrap_or_else(|_| {
        path.parent()
            .and_then(|parent| parent.canonicalize().ok())
            .map(|parent| parent.join(path.file_name().unwrap_or_default()))
            .unwrap_or_else(|| path.to_path_buf())
    });
    strip_verbatim(resolved)
}

fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = match path.into_os_string().into_string() {
        Ok(text) => text,
        Err(raw) => return PathBuf::from(raw),
    };
    let Some(rest) = text.strip_prefix(r"\\?\") else {
        return PathBuf::from(text);
    };
    if let Some(unc) = rest.strip_prefix(r"UNC\") {
        return PathBuf::from(format!(r"\\{unc}"));
    }
    PathBuf::from(rest)
}

#[cfg(windows)]
fn powershell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "RustyExile.exe"
    } else {
        "RustyExile"
    }
}

fn release_asset_name() -> String {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        std::env::consts::OS
    };
    format!("RustyExile-{os}-{}.zip", std::env::consts::ARCH)
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(next), Some(now)) => next > now,
        _ => false,
    }
}

fn parse_version(text: &str) -> Option<(u64, u64, u64)> {
    let text = text.trim().trim_start_matches('v');
    if text.is_empty() || text.contains('-') {
        return None;
    }
    let mut parts = text.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn replaced_linux_binary_is_not_relaunched_from_the_deleted_path() {
        let replaced = PathBuf::from("/opt/RustyExile/RustyExile (deleted)");
        assert_eq!(
            strip_deleted(&replaced),
            PathBuf::from("/opt/RustyExile/RustyExile")
        );
        let current = PathBuf::from("/opt/RustyExile/RustyExile");
        assert_eq!(strip_deleted(&current), current);
        let script = unix_restart_script();
        assert!(script.contains("kill -0"));
        assert!(script.contains("exec \"$2\""));
    }

    #[cfg(unix)]
    #[test]
    fn restart_helper_starts_the_new_program_after_the_old_one_exits() {
        let root = std::env::temp_dir().join(format!("rusty-restart-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let marker = root.join("started");
        let program = root.join("program");
        fs::write(
            &program,
            format!("#!/bin/sh\nprintf started > '{}'\n", marker.display()),
        )
        .unwrap();
        set_executable(&program).unwrap();
        let mut old = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        spawn_unix_restart_for(&program, old.id()).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert!(
            !marker.exists(),
            "the updated program started while the previous process was still running"
        );
        old.kill().unwrap();
        let _ = old.wait();
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !marker.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(marker.is_file(), "the updated program did not start");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_failed_asset_copy_leaves_the_old_icons() {
        let root = std::env::temp_dir().join(format!("rusty-assets-keep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let assets = root.join("assets");
        fs::create_dir_all(assets.join("data")).unwrap();
        fs::write(assets.join("data/gems.json"), "old-gems").unwrap();
        swap_tree(&root.join("missing"), &assets, &root.join("data")).unwrap_err();
        assert_eq!(
            fs::read_to_string(assets.join("data/gems.json")).unwrap(),
            "old-gems"
        );
        assert!(!assets.with_extension("new").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn newer_release_compares_numbers() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("v0.1.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("0.2.0-beta", "0.1.0"));
    }

    fn serial_update() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|err| err.into_inner())
    }

    #[test]
    fn archive_replaces_the_program_and_keeps_user_files() {
        let _serial = serial_update();
        let root = std::env::temp_dir().join(format!("rusty-update-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let data = root.join("data");
        let app = root.join("app");
        fs::create_dir_all(data.join("characters")).unwrap();
        fs::create_dir_all(&app).unwrap();
        fs::write(data.join("settings.json"), "keep-settings").unwrap();
        fs::write(data.join("characters").join("Templar.json"), "keep-build").unwrap();
        let exe = app.join(binary_name());
        fs::write(&exe, b"old").unwrap();

        let archive = sample_archive();
        install_archive(&archive, &exe, &app.join("assets"), &data).unwrap();

        assert_eq!(
            fs::read_to_string(data.join("settings.json")).unwrap(),
            "keep-settings"
        );
        assert_eq!(
            fs::read_to_string(data.join("characters").join("Templar.json")).unwrap(),
            "keep-build"
        );
        #[cfg(windows)]
        {
            assert_eq!(fs::read(&exe).unwrap(), b"old");
            assert_eq!(fs::read(pending_binary(&exe)).unwrap(), b"new-binary");
        }
        #[cfg(not(windows))]
        assert_eq!(fs::read(&exe).unwrap(), b"new-binary");
        assert_eq!(
            fs::read_to_string(app.join("assets/data/gems.json")).unwrap(),
            "{}"
        );
        assert!(!app.join("assets.new").exists());
        assert!(!app.join("assets.old").exists());
        assert!(!update_needs_permission());
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn protected_install_folder_is_staged_for_an_elevated_apply() {
        let _serial = serial_update();
        let root = std::env::temp_dir().join(format!("rusty-update-deny-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let app = root.join("app");
        let data = root.join("data");
        fs::create_dir_all(&app).unwrap();
        fs::create_dir_all(&data).unwrap();
        let exe = app.join(binary_name());
        fs::write(&exe, b"old").unwrap();
        fs::create_dir_all(app.join("assets/data")).unwrap();
        fs::write(app.join("assets/data/gems.json"), "old-gems").unwrap();
        let user = whoami();
        let deny = std::process::Command::new("icacls")
            .args([app.to_str().unwrap(), "/deny", &format!("{user}:(WD,AD)")])
            .status()
            .unwrap();
        assert!(deny.success(), "icacls deny failed");
        let _restore = AclRestore {
            path: app.clone(),
            user: user.clone(),
        };
        let result = install_archive(&sample_archive(), &exe, &app.join("assets"), &data);
        result.expect("a protected install folder should be staged");
        assert_eq!(fs::read(&exe).unwrap(), b"old");
        assert_eq!(
            fs::read_to_string(app.join("assets/data/gems.json")).unwrap(),
            "old-gems"
        );
        assert!(update_needs_permission());
        let pending: PendingUpdate =
            serde_json::from_str(&fs::read_to_string(pending_update_path()).unwrap()).unwrap();
        assert_eq!(
            fs::read(pending.stage.join(binary_name())).unwrap(),
            b"new-binary"
        );
        assert_eq!(
            fs::read_to_string(pending.stage.join("assets/data/gems.json")).unwrap(),
            "{}"
        );
        drop(_restore);
        clear_pending_update();
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn apply_script_replaces_files_after_the_program_exits() {
        let root = std::env::temp_dir().join(format!("rusty-update-apply-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let stage = root.join("stage");
        let app = root.join("app");
        let scripts = root.join("scripts");
        fs::create_dir_all(stage.join("assets/data")).unwrap();
        fs::create_dir_all(app.join("assets/data")).unwrap();
        fs::create_dir_all(&scripts).unwrap();
        fs::write(stage.join(binary_name()), b"new-binary").unwrap();
        fs::write(stage.join("assets/data/gems.json"), b"{}").unwrap();
        fs::write(app.join(binary_name()), b"old").unwrap();
        fs::write(app.join("assets/data/gems.json"), b"old-gems").unwrap();
        fs::write(app.join("assets/data/stale.txt"), b"stale").unwrap();
        let exe = app.join(binary_name());
        let mut finished = std::process::Command::new("cmd")
            .args(["/c", "exit"])
            .spawn()
            .unwrap();
        let finished_pid = finished.id();
        finished.wait().unwrap();
        let script = scripts.join("apply.ps1");
        write_powershell(
            &script,
            &apply_script(
                finished_pid,
                &stage,
                &exe,
                &app.join("assets"),
                &scripts,
                false,
            ),
        )
        .unwrap();
        let output = powershell_command()
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
            .arg(&script)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "apply failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read(&exe).unwrap(), b"new-binary");
        assert_eq!(
            fs::read_to_string(app.join("assets/data/gems.json")).unwrap(),
            "{}"
        );
        assert!(!app.join("assets/data/stale.txt").exists());
        assert!(!stage.exists());
        let launch = scripts.join("launch.ps1");
        write_powershell(&launch, &launch_script(&script)).unwrap();
        assert_parses(&script);
        assert_parses(&launch);
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    fn assert_parses(path: &Path) {
        let command = format!(
            "$errors = $null; $tokens = $null; [void][System.Management.Automation.Language.Parser]::ParseFile('{}', [ref]$tokens, [ref]$errors); if ($errors) {{ $errors | ForEach-Object {{ $_.ToString() }}; exit 1 }}",
            path.display().to_string().replace('\'', "''")
        );
        let parsed = powershell_command()
            .args(["-NoProfile", "-Command", &command])
            .output()
            .unwrap();
        assert!(
            parsed.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&parsed.stdout),
            String::from_utf8_lossy(&parsed.stderr)
        );
    }

    #[cfg(windows)]
    struct AclRestore {
        path: PathBuf,
        user: String,
    }

    #[cfg(windows)]
    impl Drop for AclRestore {
        fn drop(&mut self) {
            let _ = std::process::Command::new("icacls")
                .args([
                    self.path.to_str().unwrap_or_default(),
                    "/remove:d",
                    &self.user,
                ])
                .status();
        }
    }

    #[cfg(windows)]
    fn whoami() -> String {
        let output = std::process::Command::new("whoami").output().unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn archive_refuses_to_write_inside_user_data() {
        let _serial = serial_update();
        let root = std::env::temp_dir().join(format!("rusty-update-guard-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("characters")).unwrap();
        fs::write(root.join("settings.json"), "keep-settings").unwrap();
        let err = install_archive(
            &sample_archive(),
            &root.join(binary_name()),
            &root.join("assets"),
            &root,
        )
        .unwrap_err();
        assert!(err.to_string().contains("user data"));
        assert_eq!(
            fs::read_to_string(root.join("settings.json")).unwrap(),
            "keep-settings"
        );
        assert!(!root.join("characters").join("Templar.json").exists());
        let _ = fs::remove_dir_all(&root);
    }

    fn sample_archive() -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file(binary_name(), options).unwrap();
            zip.write_all(b"new-binary").unwrap();
            zip.start_file("assets/data/gems.json", options).unwrap();
            zip.write_all(b"{}").unwrap();
            zip.start_file("../settings.json", options).unwrap();
            zip.write_all(b"overwrite").unwrap();
            zip.finish().unwrap();
        }
        cursor.into_inner()
    }
}
