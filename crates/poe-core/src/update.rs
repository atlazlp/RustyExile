use anyhow::Context;
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
            Ok(()) => UpdateOutcome::Installed { version: release.version },
            Err(err) => UpdateOutcome::Failed(err.to_string()),
        },
        Err(err) => UpdateOutcome::Failed(err.to_string()),
    }
}

pub fn install_archive(bytes: &[u8], exe: &Path, asset_dir: &Path, protected: &Path) -> anyhow::Result<()> {
    ensure_outside(exe, protected)?;
    ensure_outside(asset_dir, protected)?;
    let temp = std::env::temp_dir().join(format!("rusty-update-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp)?;
    let result = (|| {
        extract_zip(bytes, &temp)?;
        let assets = temp.join("assets");
        if assets.is_dir() {
            if asset_dir.exists() {
                fs::remove_dir_all(asset_dir)?;
            }
            copy_tree(&assets, asset_dir, protected)?;
        }
        let packed = temp.join(binary_name());
        if !packed.is_file() {
            anyhow::bail!("the release archive has no {name}", name = binary_name());
        }
        replace_executable(&packed, exe)?;
        Ok(())
    })();
    let _ = fs::remove_dir_all(&temp);
    result
}

pub fn restart_installed() -> anyhow::Result<()> {
    let exe = std::env::current_exe().context("could not find the running program")?;
    #[cfg(windows)]
    {
        let pending = pending_binary(&exe);
        let pid = std::process::id();
        let new = powershell_quote(&pending);
        let dest = powershell_quote(&exe);
        let command = format!(
            "while (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ Start-Sleep -Milliseconds 200 }}; Move-Item -Force -LiteralPath {new} -Destination {dest}; Start-Process -FilePath {dest}"
        );
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &command])
            .spawn()
            .context("could not start the Windows updater")?;
        std::process::exit(0);
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new(&exe)
            .spawn()
            .context("could not start the updated program")?;
        std::process::exit(0);
    }
}

fn fetch_newer_release() -> anyhow::Result<Option<ReleaseAsset>> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(30)).build();
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
        return Ok(Some(ReleaseAsset { version, download_url: String::new() }));
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
                asset.get("browser_download_url").and_then(|url| url.as_str()).map(str::to_string)
            })
        })
        .with_context(|| format!("the latest release has no {asset_name}"))?;
    Ok(Some(ReleaseAsset { version, download_url }))
}

fn download_and_install(release: &ReleaseAsset) -> anyhow::Result<()> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(120)).build();
    let response = agent
        .get(&release.download_url)
        .set("User-Agent", &format!("RustyExile/{VERSION}"))
        .call()
        .with_context(|| format!("could not download {}", release.version))?;
    let mut bytes = Vec::new();
    response.into_reader().take(200 * 1024 * 1024).read_to_end(&mut bytes)?;
    let exe = std::env::current_exe().context("could not find the running program")?;
    let asset_dir = exe.parent().context("the program has no folder")?.join("assets");
    install_archive(&bytes, &exe, &asset_dir, &crate::store::root_dir())
}

fn extract_zip(bytes: &[u8], dest: &Path) -> anyhow::Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("the release is not a zip archive")?;
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

fn copy_tree(from: &Path, to: &Path, protected: &Path) -> anyhow::Result<()> {
    ensure_outside(to, protected)?;
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        ensure_outside(&target, protected)?;
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target, protected)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn replace_executable(from: &Path, exe: &Path) -> anyhow::Result<()> {
    if let Some(parent) = exe.parent() {
        fs::create_dir_all(parent)?;
    }
    #[cfg(windows)]
    let dest = pending_binary(exe);
    #[cfg(not(windows))]
    let dest = exe.to_path_buf();
    let staging = exe.with_file_name(".RustyExile.staging");
    fs::copy(from, &staging)?;
    set_executable(&staging)?;
    if dest != exe && dest.exists() {
        let _ = fs::remove_file(&dest);
    }
    fs::rename(&staging, &dest)?;
    Ok(())
}

#[cfg(windows)]
fn pending_binary(exe: &Path) -> PathBuf {
    let name = exe.file_name().and_then(|name| name.to_str()).unwrap_or("RustyExile.exe");
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
    if path.starts_with(&protected) {
        anyhow::bail!("refusing to change user data in {}", path.display());
    }
    Ok(())
}

fn normalize(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(windows)]
fn powershell_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

fn binary_name() -> &'static str {
    if cfg!(windows) { "RustyExile.exe" } else { "RustyExile" }
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
    fn newer_release_compares_numbers() {
        assert!(is_newer("0.2.0", "0.1.0"));
        assert!(is_newer("v0.1.1", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.2.0"));
        assert!(!is_newer("0.2.0-beta", "0.1.0"));
    }

    #[test]
    fn archive_replaces_the_program_and_keeps_user_files() {
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

        assert_eq!(fs::read_to_string(data.join("settings.json")).unwrap(), "keep-settings");
        assert_eq!(fs::read_to_string(data.join("characters").join("Templar.json")).unwrap(), "keep-build");
        assert_eq!(fs::read(&exe).unwrap(), b"new-binary");
        assert_eq!(fs::read_to_string(app.join("assets/data/gems.json")).unwrap(), "{}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn archive_refuses_to_write_inside_user_data() {
        let root = std::env::temp_dir().join(format!("rusty-update-guard-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("characters")).unwrap();
        fs::write(root.join("settings.json"), "keep-settings").unwrap();
        let err = install_archive(&sample_archive(), &root.join(binary_name()), &root.join("assets"), &root).unwrap_err();
        assert!(err.to_string().contains("user data"));
        assert_eq!(fs::read_to_string(root.join("settings.json")).unwrap(), "keep-settings");
        assert!(!root.join("characters").join("Templar.json").exists());
        let _ = fs::remove_dir_all(&root);
    }

    fn sample_archive() -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
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
