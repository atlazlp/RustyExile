use crate::model::{CharacterProgress, EventKind, VisibleLine};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Landmark {
    pub id: String,
    pub act: u8,
    pub areas: String,
    pub area_ids: Vec<String>,
    pub text: String,
    pub enabled: bool,
}

pub fn default_landmarks() -> Vec<Landmark> {
    vec![
        mark("flooded-depths", 1, "The Flooded Depths", &["1_1_4_0"], true, "Dweller of the Deep is stuck in an Essence. The boat behind it sails toward the far exit."),
        mark("climb-legion", 1, "The Climb", &["1_1_6"], false, "Legion monolith by Lioneye's Standard. Karui versus Eternal Empire."),
        mark("upper-prison-chemist", 1, "The Upper Prison", &["1_1_7_2"], true, "Switch on the column by the crucified skeleton. Hidden Chemist's Strongbox."),
        mark("ship-graveyard", 1, "The Ship Graveyard", &["1_1_9"], false, "Pirate chest. The Drunken Corsair and two tormented spirits."),
        mark("sins-gems", 2, "The Chamber of Sins Level 2", &["1_2_6_2"], false, "Side room with a handful of level 1 gems."),
        mark("broken-bridge", 2, "The Broken Bridge", &["1_2_4"], false, "Three neutral exiles by the waypoint. The jewellery cache turns them hostile."),
        mark("fellshrine", 2, "The Fellshrine Ruins", &["1_2_15"], false, "Three devotional statues. Using them spawns the Forgotten Daughter near the Crypt."),
        mark("vaal-ruins", 2, "The Vaal Ruins", &["1_2_11"], false, "Corruption altar. Works like a Vaal Orb."),
        mark("ritual-omen", 2, "The Northern Forest or The Dread Thicket", &["1_2_8", "1_2_13"], false, "Ritual with a guaranteed Omen. Later visits keep the monsters, not the altars."),
        mark("caverns-recipe", 2, "The Caverns", &["1_2_14_2"], false, "Two pillars in the rubble room on the pyramid floor open a crafting recipe."),
        mark("blight", 3, "The Sewers or The Imperial Gardens", &["1_3_10_1", "1_3_15"], false, "Blight encounter in this area."),
        mark("docks-essence", 3, "The Docks", &["1_3_9"], false, "The Shipyard Terror is locked in a tier 3 Essence."),
        mark("archives-recipe", 3, "The Archives", &["1_3_17_2"], false, "Click the candle on the long wall. Hidden crafting recipe."),
        mark("dried-lake-legion", 4, "The Dried Lake", &["1_4_2"], false, "Legion monolith beside Nightwane. Longer duration, always includes Eternal."),
        mark("heart-corruption", 4, "The Belly of the Beast Level 1", &["1_4_6_1"], false, "Cankerous Sore. Heart of Corruption drops a Vaal skill gem."),
        mark("reliquary-keys", 5, "The Reliquary", &["1_5_7"], false, "Chests by the Torment items hold the Keys of Strength and Devotion. They open the vault by the waypoint."),
        mark("tidal-gold", 6, "The Tidal Island", &["2_6_3"], false, "Treasure piles in the water near Riptide drop gold."),
        mark("karui-tattoos", 6, "The Karui Fortress", &["2_6_5"], true, "Island across the red totem bridge. The Tukohama pack drops tattoos the first time you kill it."),
        mark("southern-forest", 6, "The Southern Forest", &["2_6_12"], false, "Forest's Treasure inside the ring of trees. Ursa pack, then a currency item."),
        mark("flesh-cradle", 7, "Maligaro's Sanctum", &["2_7_5_map"], false, "Second island. Flesh Cradle drops a rare ring."),
        mark("sins-quality-gem", 7, "The Chamber of Sins Level 2", &["2_7_5_2"], false, "Past the ascendancy plaque, a wall switch opens a quality gem chest."),
        mark("ultimatum", 7, "The Vaal City or The Temple of Decay Level 1", &["2_7_11", "2_7_12_1"], false, "Ultimatum encounter in this area."),
        mark("temple-red-room", 7, "The Temple of Decay Level 1", &["2_7_12_1"], false, "Side passage on the first floor. Red room with three corrupted item chests."),
        mark("ramparts-cache", 8, "The Sarn Ramparts", &["2_8_1"], false, "Switch at the base of the rampart, about halfway. Hidden tunnel with a Smuggler's Cache."),
        mark("underbelly", 8, "The Grain Gate, The Solaris Concourse, or The Bath House", &["2_8_9", "2_8_11", "2_8_5"], false, "Hidden grate into the Hidden Underbelly, linking these three areas."),
        mark("grain-chemist", 8, "The Grain Gate", &["2_8_9"], false, "Chemist's Strongbox somewhere in the Grain Gate. Extra flasks."),
        mark("twilight-room", 8, "The Grand Promenade, Solaris Temple, or Lunaris Temple", &["2_8_3", "2_8_7_1_", "2_8_12_1"], false, "Pressure plates in both temples unlock the mural room in the Grand Promenade. The chest drops a unique."),
        mark("bath-scouring", 8, "The Bath House", &["2_8_5"], false, "Break the wall in a corner. Chest with Orbs of Scouring."),
        mark("vastiri-jewels", 9, "The Vastiri Desert", &["2_9_3"], false, "Platform northeast of the Descent. Pick one statue for a rare jewel of that color."),
        mark("boiling-fossils", 9, "The Boiling Lake", &["2_9_6"], false, "Corner opposite the Gorgon, near the entrance. Two remains drop fossils."),
        mark("tunnel-gem", 9, "The Tunnel", &["2_9_7"], false, "Wall switch near the end of the trial. Hidden tunnel with a transfigured gem."),
        mark("refinery-heist", 9, "The Refinery", &["2_9_9"], false, "Grate into a small bunker. Crafting recipe inside."),
        mark("ossuary-lens", 10, "The Ossuary", &["2_10_9"], true, "In the trial, switch on the wall before the zig-zag. The passage opposite the lever holds a Facetor's Lens."),
    ]
}

fn mark(id: &str, act: u8, areas: &str, area_ids: &[&str], enabled: bool, text: &str) -> Landmark {
    Landmark {
        id: id.into(),
        act,
        areas: areas.into(),
        area_ids: area_ids.iter().map(|id| (*id).to_string()).collect(),
        text: text.into(),
        enabled,
    }
}

pub fn reconcile_landmarks(saved: &[Landmark]) -> Vec<Landmark> {
    default_landmarks()
        .into_iter()
        .map(|mut item| {
            if let Some(prev) = saved.iter().find(|saved| saved.id == item.id) {
                item.enabled = prev.enabled;
                if !prev.text.trim().is_empty() {
                    item.text = prev.text.clone();
                }
            }
            item
        })
        .collect()
}

pub fn visible_landmarks(landmarks: &[Landmark], progress: &CharacterProgress) -> Vec<VisibleLine> {
    landmarks
        .iter()
        .filter(|landmark| landmark.enabled && landmark.area_ids.iter().any(|id| id == &progress.area_id))
        .map(|landmark| VisibleLine {
            event_id: landmark.id.clone(),
            group_name: None,
            text: landmark.text.clone(),
            detail: String::new(),
            icon: "quest".into(),
            gem_name: None,
            act: landmark.act,
            kind: EventKind::Step,
            is_header: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_landmarks_start_enabled() {
        let on: Vec<_> = default_landmarks()
            .into_iter()
            .filter(|landmark| landmark.enabled)
            .map(|landmark| landmark.id)
            .collect();
        assert_eq!(
            on,
            vec!["flooded-depths", "upper-prison-chemist", "karui-tattoos", "ossuary-lens"]
        );
    }

    #[test]
    fn landmark_shows_only_inside_its_zone() {
        let landmarks = default_landmarks();
        let mut progress = CharacterProgress::default();
        assert!(visible_landmarks(&landmarks, &progress).is_empty());
        progress.area_id = "1_1_4_0".into();
        let lines = visible_landmarks(&landmarks, &progress);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].text.contains("Dweller"));
        progress.area_id = "1_1_7_2".into();
        let lines = visible_landmarks(&landmarks, &progress);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].text.contains("Chemist"));
        progress.area_id = "2_8_9".into();
        assert!(visible_landmarks(&landmarks, &progress).is_empty());
    }

    #[test]
    fn reconcile_keeps_text_and_resets_triggers() {
        let mut saved = default_landmarks();
        saved[2].text = "Custom flask hint".into();
        saved[2].enabled = false;
        saved[2].area_ids = vec!["nope".into()];
        let merged = reconcile_landmarks(&saved);
        let prison = merged.iter().find(|landmark| landmark.id == "upper-prison-chemist").unwrap();
        assert_eq!(prison.text, "Custom flask hint");
        assert!(!prison.enabled);
        assert_eq!(prison.area_ids, vec!["1_1_7_2"]);
    }
}
