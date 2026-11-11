use crate::discover::GameLocation;
use crate::model::{Catalog, CharacterFile, RegexPreset, Settings};

#[derive(Debug)]
pub struct AppState {
    pub settings: Settings,
    pub catalog: Catalog,
    pub characters: Vec<CharacterFile>,
    pub regex_presets: Vec<RegexPreset>,
    pub show_config: bool,
    pub hud_enabled: bool,
    pub running: Option<GameLocation>,
    pub status: String,
    pub dirty_settings: bool,
    pub dirty_catalog: bool,
    pub dirty_characters: bool,
    pub dirty_regex: bool,
    pub ctrl_down: bool,
    pub alt_down: bool,
    pub shift_down: bool,
    pub capturing_hotkey: bool,
    pub hud_placing: bool,
}

impl AppState {
    pub fn active_character(&self) -> Option<&CharacterFile> {
        let name = self.settings.active_character.as_deref()?;
        self.characters.iter().find(|c| c.name == name)
    }

    pub fn active_character_mut(&mut self) -> Option<&mut CharacterFile> {
        let name = self.settings.active_character.clone()?;
        self.characters.iter_mut().find(|c| c.name == name)
    }
}
