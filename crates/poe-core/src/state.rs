use crate::discover::GameLocation;
use crate::model::{Catalog, CharacterFile, RegexPreset, Settings};

#[derive(Debug, Clone)]
pub struct GemSpot {
    pub event_id: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

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
    pub gem_spots: Vec<GemSpot>,
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

    pub fn in_town(&self) -> bool {
        self.active_character()
            .is_some_and(|character| character.progress.area_id.ends_with("_town"))
    }

    pub fn click_gem(&mut self, x: i32, y: i32) {
        let Some(id) = self
            .gem_spots
            .iter()
            .find(|spot| x >= spot.x && y >= spot.y && x < spot.x + spot.w && y < spot.y + spot.h)
            .map(|spot| spot.event_id.clone())
        else {
            return;
        };
        if let Some(character) = self.active_character_mut() {
            crate::engine::force_complete(&mut character.progress, &id);
            self.dirty_characters = true;
        }
        self.gem_spots.retain(|spot| spot.event_id != id);
    }
}
