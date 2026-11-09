use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Experience {
    Beginner,
    Intermediate,
    Advanced,
}

impl Experience {
    pub fn label(self) -> &'static str {
        match self {
            Self::Beginner => "Beginner",
            Self::Intermediate => "Intermediate",
            Self::Advanced => "Advanced",
        }
    }

    pub fn explanation(self) -> &'static str {
        match self {
            Self::Beginner => {
                "Zone tips stay on and the act list stays on. You can change both later in the configuration."
            }
            Self::Intermediate => {
                "Zone tips start off and the act list stays on. You can change both later in the configuration."
            }
            Self::Advanced => {
                "Zone tips and the act list both start off. Gems start off with the acts. You can turn any of them back on in the configuration."
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallKind {
    Steam,
    Standalone,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub background: [u8; 4],
    pub text: [u8; 4],
    pub accent: [u8; 4],
    pub muted: [u8; 4],
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: [12, 10, 16, 210],
            text: [236, 228, 210, 255],
            accent: [196, 148, 74, 255],
            muted: [180, 170, 150, 220],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hotkeys {
    pub toggle_hud: String,
    pub open_config: String,
    pub paste_regex: String,
    /// Modifiers held while clicking a task to clear it. The click is separate.
    pub clear_modifiers: String,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            toggle_hud: "ctrl+shift+h".into(),
            open_config: "ctrl+shift+o".into(),
            paste_regex: "ctrl+shift+v".into(),
            clear_modifiers: "ctrl+alt".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedInstall {
    pub kind: InstallKind,
    pub install_dir: String,
    pub log_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub experience_chosen: bool,
    pub experience: Experience,
    pub tips: bool,
    pub acts: bool,
    pub gems: bool,
    pub opacity: f32,
    pub scale: f32,
    pub corner: HudCorner,
    pub theme: Theme,
    pub hotkeys: Hotkeys,
    pub log_path_override: Option<String>,
    pub league_start: bool,
    pub install: Option<SavedInstall>,
    pub active_character: Option<String>,
    pub active_regex: usize,
    pub poe_leveling_url: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            experience_chosen: false,
            experience: Experience::Beginner,
            tips: true,
            acts: true,
            gems: true,
            opacity: 0.92,
            scale: 1.0,
            corner: HudCorner::TopRight,
            theme: Theme::default(),
            hotkeys: Hotkeys::default(),
            log_path_override: None,
            league_start: true,
            install: None,
            active_character: None,
            active_regex: 0,
            poe_leveling_url: "https://poe-leveling.com/".into(),
        }
    }
}

impl Settings {
    pub fn apply_experience(&mut self, experience: Experience) {
        self.experience = experience;
        self.experience_chosen = true;
        match experience {
            Experience::Beginner => {
                self.tips = true;
                self.acts = true;
                self.gems = true;
            }
            Experience::Intermediate => {
                self.tips = false;
                self.acts = true;
                self.gems = true;
            }
            Experience::Advanced => {
                self.tips = false;
                self.acts = false;
                self.gems = false;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TriggerKind {
    ZoneEnter { area_id: String },
    ZoneName { name: String },
    ActStart { act: u8 },
    ActEnd { act: u8 },
    LevelAtLeast { level: u32 },
    PassiveAllocated { id: String },
    Manual,
}

impl TriggerKind {
    pub fn label(&self) -> String {
        match self {
            Self::ZoneEnter { area_id } => format!("Enter area {area_id}"),
            Self::ZoneName { name } => format!("Enter {name}"),
            Self::ActStart { act } => format!("Start of act {act}"),
            Self::ActEnd { act } => format!("End of act {act}"),
            Self::LevelAtLeast { level } => format!("Level {level}+"),
            Self::PassiveAllocated { id } => format!("Allocate {id}"),
            Self::Manual => "Manual".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Step,
    GemPick,
    GemBuy,
}

impl EventKind {
    pub fn is_gem(self) -> bool {
        matches!(self, Self::GemPick | Self::GemBuy)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignEvent {
    pub id: String,
    pub act: u8,
    pub text: String,
    pub detail: String,
    pub icon: String,
    pub kind: EventKind,
    pub start: Vec<TriggerKind>,
    pub end: Vec<TriggerKind>,
    pub gem_id: Option<String>,
    pub gem_name: Option<String>,
    pub npc: Option<String>,
    pub zone_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventGroup {
    pub id: String,
    pub act: u8,
    pub name: String,
    pub start: Vec<TriggerKind>,
    pub event_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeSpec {
    pub title: String,
    pub nodes: Vec<u32>,
    pub tree_version: String,
    pub class_id: Option<u32>,
    pub ascend_class_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CharacterRoute {
    pub events: Vec<CampaignEvent>,
    pub groups: Vec<EventGroup>,
    pub tree_specs: Vec<TreeSpec>,
    pub pob_notes: String,
    pub pob_url: String,
    pub pob_gems: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CharacterProgress {
    pub level: u32,
    pub area_id: String,
    pub zone_name: String,
    pub class_name: String,
    pub entered_areas: BTreeSet<String>,
    pub entered_names: BTreeSet<String>,
    pub passives: BTreeSet<String>,
    pub completed: BTreeSet<String>,
    pub highest_act: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterFile {
    pub name: String,
    pub class_name: String,
    pub route: CharacterRoute,
    pub progress: CharacterProgress,
    pub user_edited: bool,
}

#[derive(Debug, Clone)]
pub struct VisibleLine {
    pub event_id: String,
    pub group_name: Option<String>,
    pub text: String,
    pub detail: String,
    pub icon: String,
    pub gem_name: Option<String>,
    pub act: u8,
    pub kind: EventKind,
    pub is_header: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AreaInfo {
    pub id: String,
    pub name: String,
    pub act: u8,
    pub is_town: bool,
    pub has_waypoint: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GemInfo {
    pub id: String,
    pub name: String,
    pub primary_attribute: String,
    pub required_level: u32,
    pub is_support: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GemSource {
    Reward,
    Vendor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestGem {
    pub gem_id: String,
    pub classes: Vec<String>,
    pub npc: String,
    pub source: GemSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestInfo {
    pub id: String,
    pub name: String,
    pub act: u8,
    pub npc: String,
    pub gems: Vec<QuestGem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Catalog {
    pub areas: Vec<AreaInfo>,
    pub gems: Vec<GemInfo>,
    pub quests: Vec<QuestInfo>,
    pub routes: Vec<(u8, String)>,
}

impl Catalog {
    pub fn area(&self, id: &str) -> Option<&AreaInfo> {
        self.areas.iter().find(|a| a.id == id)
    }

    pub fn gem(&self, id: &str) -> Option<&GemInfo> {
        self.gems.iter().find(|g| g.id == id)
    }

    pub fn gem_by_name(&self, name: &str) -> Option<&GemInfo> {
        self.gems.iter().find(|g| g.name.eq_ignore_ascii_case(name))
    }

    pub fn quest(&self, id: &str) -> Option<&QuestInfo> {
        self.quests.iter().find(|q| q.id == id)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegexPreset {
    pub name: String,
    pub regex: String,
}

#[derive(Debug, Clone)]
pub struct Chord {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: String,
}

pub fn parse_chord(raw: &str) -> Chord {
    let mut chord = Chord {
        ctrl: false,
        alt: false,
        shift: false,
        key: String::new(),
    };
    for part in raw.split('+') {
        match part.trim().to_ascii_lowercase().as_str() {
            "ctrl" | "control" => chord.ctrl = true,
            "alt" => chord.alt = true,
            "shift" => chord.shift = true,
            "" => {}
            other => chord.key = other.to_string(),
        }
    }
    chord
}

pub fn gem_icon_file(gem_name: &str) -> String {
    let mut out = String::new();
    for ch in gem_name.chars() {
        if ch == ' ' {
            out.push('_');
        } else if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            out.push(ch);
        }
    }
    format!("{out}_inventory_icon.png")
}
