use crate::model::{Catalog, CharacterFile, RegexPreset, Settings};
use anyhow::Context;
use std::fs;
use std::path::PathBuf;

pub fn root_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "poe-campaign", "poe-campaign")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("poe-campaign-data"))
}

pub fn load_settings() -> Settings {
    let path = root_dir().join("settings.json");
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &Settings) -> anyhow::Result<()> {
    let dir = root_dir();
    fs::create_dir_all(&dir)?;
    let text = serde_json::to_string_pretty(settings)?;
    fs::write(dir.join("settings.json"), text)?;
    Ok(())
}

pub fn load_catalog(fallback: Catalog) -> Catalog {
    let path = root_dir().join("database.json");
    if let Ok(text) = fs::read_to_string(&path) {
        if let Ok(catalog) = serde_json::from_str(&text) {
            return catalog;
        }
    }
    if let Err(err) = save_catalog(&fallback) {
        eprintln!("could not write the gem database: {err}");
    }
    fallback
}

pub fn save_catalog(catalog: &Catalog) -> anyhow::Result<()> {
    let dir = root_dir();
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("database.json"), serde_json::to_string(catalog)?)?;
    Ok(())
}

pub fn character_path(name: &str) -> PathBuf {
    root_dir().join("characters").join(format!("{}.json", safe_name(name)))
}

pub fn load_character(name: &str) -> Option<CharacterFile> {
    let text = fs::read_to_string(character_path(name)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_character(file: &CharacterFile) -> anyhow::Result<()> {
    let path = character_path(&file.name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string(file)?)?;
    Ok(())
}

pub fn list_characters() -> Vec<CharacterFile> {
    let dir = root_dir().join("characters");
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        if let Ok(file) = serde_json::from_str::<CharacterFile>(&text) {
            files.push(file);
        }
    }
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    files
}

pub fn load_regex_presets(defaults: Vec<RegexPreset>) -> Vec<RegexPreset> {
    let path = root_dir().join("regex.json");
    if let Ok(text) = fs::read_to_string(&path) {
        if let Ok(presets) = serde_json::from_str(&text) {
            return presets;
        }
    }
    let _ = save_regex_presets(&defaults);
    defaults
}

pub fn save_regex_presets(presets: &[RegexPreset]) -> anyhow::Result<()> {
    let dir = root_dir();
    fs::create_dir_all(&dir).context("create data dir")?;
    fs::write(dir.join("regex.json"), serde_json::to_string_pretty(presets)?)?;
    Ok(())
}

fn safe_name(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

pub fn read_log_tail(path: &std::path::Path, max_bytes: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    if start > 0 {
        if let Some(idx) = text.find('\n') {
            text = text[idx + 1..].to_string();
        }
    }
    Some(text)
}

pub struct LogFollow {
    pub path: PathBuf,
    pub offset: u64,
    pub partial: String,
}

impl LogFollow {
    pub fn open(path: PathBuf) -> Option<Self> {
        let len = fs::metadata(&path).ok()?.len();
        Some(Self {
            path,
            offset: len,
            partial: String::new(),
        })
    }

    pub fn poll(&mut self) -> Vec<String> {
        use std::io::{Read, Seek, SeekFrom};
        let mut file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(_) => return Vec::new(),
        };
        let len = file.metadata().map(|m| m.len()).unwrap_or(self.offset);
        if len < self.offset {
            self.offset = 0;
            self.partial.clear();
        }
        if len == self.offset {
            return Vec::new();
        }
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut buf = String::new();
        if file.read_to_string(&mut buf).is_err() {
            return Vec::new();
        }
        self.offset = len;
        self.partial.push_str(&buf);
        let mut lines = Vec::new();
        while let Some(idx) = self.partial.find('\n') {
            let line = self.partial[..idx].trim_end_matches('\r').to_string();
            self.partial = self.partial[idx + 1..].to_string();
            if !line.is_empty() {
                lines.push(line);
            }
        }
        lines
    }
}
