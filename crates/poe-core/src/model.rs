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
                "Zone tips, the act list, and landmarks stay on. You can change them later in the configuration."
            }
            Self::Intermediate => {
                "Zone tips start off. The act list and landmarks stay on. You can change them later in the configuration."
            }
            Self::Advanced => {
                "Zone tips and the act list start off. Gems from the build stay on, with their triggers, when the character is added. Landmarks stay on."
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
    Custom,
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
pub struct ChatCommand {
    pub id: String,
    pub label: String,
    pub text: String,
    pub hotkey: String,
}

impl ChatCommand {
    pub fn new(
        label: impl Into<String>,
        text: impl Into<String>,
        hotkey: impl Into<String>,
    ) -> Self {
        Self {
            id: format!("chat-{}", chat_id()),
            label: label.into(),
            text: text.into(),
            hotkey: hotkey.into(),
        }
    }
}

fn chat_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let tick = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    tick.wrapping_add(NEXT.fetch_add(1, Ordering::Relaxed))
}

pub fn default_chat_commands() -> Vec<ChatCommand> {
    [
        ("hideout", "Hideout", "/hideout", "ctrl+alt+h"),
        ("oos", "Resync", "/oos", "ctrl+alt+o"),
        ("remaining", "Remaining", "/remaining", "ctrl+alt+r"),
        ("exit", "Exit to login", "/exit", "ctrl+alt+x"),
        ("passives", "Passives", "/passives", "ctrl+alt+p"),
        ("deaths", "Deaths", "/deaths", "ctrl+alt+d"),
        ("leave", "Leave party", "/leave", "ctrl+alt+l"),
        ("dnd", "Do not disturb", "/dnd", "ctrl+alt+n"),
        ("menagerie", "Menagerie", "/menagerie", "ctrl+alt+m"),
        ("delve", "Delve", "/delve", "ctrl+alt+e"),
        ("kingsmarch", "Kingsmarch", "/kingsmarch", "ctrl+alt+k"),
        ("afk", "AFK", "/afk", "ctrl+alt+a"),
    ]
    .into_iter()
    .map(|(id, label, text, hotkey)| ChatCommand {
        id: id.into(),
        label: label.into(),
        text: text.into(),
        hotkey: hotkey.into(),
    })
    .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hotkeys {
    pub toggle_hud: String,
    pub open_config: String,
    pub paste_regex: String,
    pub clear_modifiers: String,
    #[serde(default = "default_events_back")]
    pub events_back: String,
    #[serde(default = "default_events_forward")]
    pub events_forward: String,
    #[serde(default = "default_group_back")]
    pub group_back: String,
    #[serde(default = "default_group_forward")]
    pub group_forward: String,
    #[serde(default = "default_chat_commands")]
    pub chat: Vec<ChatCommand>,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            toggle_hud: "ctrl+shift+h".into(),
            open_config: "ctrl+shift+o".into(),
            paste_regex: "ctrl+shift+v".into(),
            clear_modifiers: "ctrl+alt".into(),
            events_back: default_events_back(),
            events_forward: default_events_forward(),
            group_back: default_group_back(),
            group_forward: default_group_forward(),
            chat: default_chat_commands(),
        }
    }
}

fn default_events_back() -> String {
    "ctrl+shift+left".into()
}

fn default_events_forward() -> String {
    "ctrl+shift+right".into()
}

fn default_group_back() -> String {
    "ctrl+,".into()
}

fn default_group_forward() -> String {
    "ctrl+.".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default)]
    pub custom_x: Option<i32>,
    #[serde(default)]
    pub custom_y: Option<i32>,
    #[serde(default = "default_show_config_button")]
    pub show_config_button: bool,
    pub theme: Theme,
    pub hotkeys: Hotkeys,
    pub log_path_override: Option<String>,
    pub league_start: bool,
    pub install: Option<SavedInstall>,
    pub active_character: Option<String>,
    pub active_regex: usize,
    pub poe_leveling_url: String,
    #[serde(default = "default_show_landmarks")]
    pub show_landmarks: bool,
    #[serde(default = "default_auto_update")]
    pub auto_update: bool,
    #[serde(default)]
    pub landmarks: Vec<crate::landmarks::Landmark>,
}

fn default_show_landmarks() -> bool {
    true
}

fn default_auto_update() -> bool {
    true
}

fn default_show_config_button() -> bool {
    true
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
            custom_x: None,
            custom_y: None,
            show_config_button: true,
            theme: Theme::default(),
            hotkeys: Hotkeys::default(),
            log_path_override: None,
            league_start: true,
            install: None,
            active_character: None,
            active_regex: crate::regexes::default_regex_index(),
            poe_leveling_url: "https://poe-leveling.com/".into(),
            show_landmarks: true,
            auto_update: true,
            landmarks: crate::landmarks::default_landmarks(),
        }
    }
}

impl Settings {
    pub fn apply_experience(&mut self, experience: Experience) {
        self.experience = experience;
        self.experience_chosen = true;
        self.show_landmarks = true;
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
                self.gems = true;
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
    #[serde(default)]
    pub bandit: String,
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
    #[serde(default)]
    pub trial_gems: bool,
    #[serde(default)]
    pub skipped_groups: Vec<String>,
    #[serde(default)]
    pub cleared_groups: Vec<String>,
    #[serde(default)]
    pub anchor_group: Option<String>,
    #[serde(default)]
    pub visited_groups: Vec<String>,
    #[serde(default)]
    pub view_delta: i32,
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
            "," | "comma" => chord.key = "comma".into(),
            "." | "dot" | "period" => chord.key = "dot".into(),
            other => chord.key = other.to_string(),
        }
    }
    chord
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BanditChoice {
    #[default]
    KillAll,
    Alira,
    Kraityn,
    Oak,
}

impl BanditChoice {
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "alira" => Self::Alira,
            "kraityn" | "kraitlyn" | "kraitlin" => Self::Kraityn,
            "oak" => Self::Oak,
            _ => Self::KillAll,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::KillAll => "kill",
            Self::Alira => "alira",
            Self::Kraityn => "kraityn",
            Self::Oak => "oak",
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advanced_keeps_build_gems_on() {
        let mut settings = Settings::default();
        settings.apply_experience(Experience::Advanced);
        assert!(!settings.tips);
        assert!(!settings.acts);
        assert!(settings.gems);
        assert!(settings.show_landmarks);
        assert!(settings.auto_update);
        assert!(settings.experience_chosen);
    }

    #[test]
    fn landmarks_stay_on_for_every_profile() {
        for experience in [
            Experience::Beginner,
            Experience::Intermediate,
            Experience::Advanced,
        ] {
            let mut settings = Settings::default();
            settings.show_landmarks = false;
            settings.apply_experience(experience);
            assert!(settings.show_landmarks);
        }
    }

    #[test]
    fn saved_settings_without_the_landmarks_switch_stay_on() {
        let mut value = serde_json::to_value(Settings::default()).unwrap();
        value.as_object_mut().unwrap().remove("show_landmarks");
        value.as_object_mut().unwrap().remove("auto_update");
        let settings: Settings = serde_json::from_value(value).unwrap();
        assert!(settings.show_landmarks);
        assert!(settings.auto_update);
    }

    #[test]
    fn saved_hotkeys_without_chat_gain_the_defaults() {
        let hotkeys: Hotkeys = serde_json::from_str(
            r#"{"toggle_hud":"ctrl+shift+h","open_config":"ctrl+shift+o","paste_regex":"ctrl+shift+v","clear_modifiers":"ctrl+alt"}"#,
        )
        .unwrap();
        assert!(hotkeys.chat.iter().any(|cmd| cmd.text == "/hideout"));
    }
}
