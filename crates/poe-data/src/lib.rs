use image::RgbaImage;
use poe_core::{
    AreaInfo, Catalog, ClassGems, GemInfo, GemSource, QuestGem, QuestInfo, RegexPreset,
};
use std::path::{Path, PathBuf};

pub fn assets_root() -> PathBuf {
    if let Ok(path) = std::env::var("RUSTY_EXILE_ASSETS") {
        return PathBuf::from(path);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside = dir.join("assets");
            if beside.join("data/gems.json").is_file() {
                return beside;
            }
        }
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

pub fn load_catalog() -> Catalog {
    let root = assets_root();
    let areas = load_areas(&root.join("data/areas.json"));
    let gems = load_gems(&root.join("data/gems.json"));
    let quests = load_quests(&root.join("data/quests.json"));
    let starters = load_starters(&root.join("data/characters.json"));
    let mut routes = Vec::new();
    for act in 1..=10 {
        let path = root.join(format!("data/routes/act-{act}.txt"));
        if let Ok(text) = std::fs::read_to_string(path) {
            routes.push((act, text));
        }
    }
    Catalog {
        areas,
        gems,
        quests,
        routes,
        starters,
    }
}

fn load_starters(path: &Path) -> Vec<ClassGems> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(raw) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&text) else {
        return Vec::new();
    };
    raw.into_iter()
        .filter_map(|(class_name, value)| {
            Some(ClassGems {
                class_name,
                start_gem_id: value.get("start_gem_id")?.as_str()?.to_string(),
                chest_gem_id: value.get("chest_gem_id")?.as_str()?.to_string(),
            })
        })
        .collect()
}

pub fn load_regex_defaults() -> Vec<RegexPreset> {
    let path = assets_root().join("regex/presets.json");
    if let Ok(text) = std::fs::read_to_string(path) {
        if let Ok(presets) = serde_json::from_str(&text) {
            return presets;
        }
    }
    poe_core::default_presets()
}

fn load_areas(path: &Path) -> Vec<AreaInfo> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(raw) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&text) else {
        return Vec::new();
    };
    raw.into_values()
        .filter_map(|value| {
            Some(AreaInfo {
                id: value.get("id")?.as_str()?.to_string(),
                name: value.get("name")?.as_str()?.to_string(),
                act: value.get("act")?.as_u64()? as u8,
                is_town: value.get("is_town_area")?.as_bool().unwrap_or(false),
                has_waypoint: value.get("has_waypoint")?.as_bool().unwrap_or(false),
            })
        })
        .collect()
}

fn load_gems(path: &Path) -> Vec<GemInfo> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(raw) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&text) else {
        return Vec::new();
    };
    raw.into_values()
        .filter_map(|value| {
            Some(GemInfo {
                id: value.get("id")?.as_str()?.to_string(),
                name: value.get("name")?.as_str()?.to_string(),
                primary_attribute: value
                    .get("primary_attribute")
                    .and_then(|v| v.as_str())
                    .unwrap_or("none")
                    .to_string(),
                required_level: value
                    .get("required_level")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as u32,
                is_support: value
                    .get("is_support")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
            })
        })
        .collect()
}

fn load_quests(path: &Path) -> Vec<QuestInfo> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(raw) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&text) else {
        return Vec::new();
    };
    raw.into_values()
        .filter_map(|value| {
            let id = value.get("id")?.as_str()?.to_string();
            let name = value.get("name")?.as_str()?.to_string();
            let act = value
                .get("act")
                .and_then(|v| v.as_str())
                .unwrap_or("1")
                .parse()
                .unwrap_or(1);
            let mut npc = String::new();
            let mut gems = Vec::new();
            if let Some(offers) = value.get("reward_offers").and_then(|v| v.as_object()) {
                for (offer_id, offer) in offers {
                    let offer_npc = offer
                        .get("quest_npc")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if npc.is_empty() && !offer_npc.is_empty() {
                        npc = offer_npc.to_string();
                    }
                    collect_gems(
                        offer.get("quest"),
                        GemSource::Reward,
                        offer_npc,
                        offer_id,
                        &mut gems,
                    );
                    collect_gems(
                        offer.get("vendor"),
                        GemSource::Vendor,
                        offer_npc,
                        offer_id,
                        &mut gems,
                    );
                }
            }
            Some(QuestInfo {
                id,
                name,
                act,
                npc,
                gems,
            })
        })
        .collect()
}

fn collect_gems(
    bucket: Option<&serde_json::Value>,
    source: GemSource,
    fallback_npc: &str,
    offer: &str,
    out: &mut Vec<QuestGem>,
) {
    let Some(map) = bucket.and_then(|v| v.as_object()) else {
        return;
    };
    for (gem_id, info) in map {
        let classes = info
            .get("classes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let npc = info
            .get("npc")
            .and_then(|v| v.as_str())
            .unwrap_or(fallback_npc)
            .to_string();
        out.push(QuestGem {
            gem_id: gem_id.clone(),
            classes,
            npc,
            source,
            offer: offer.to_string(),
        });
    }
}

pub fn tray_icon_argb(size: u32) -> Option<(i32, i32, Vec<u8>)> {
    let (width, height, rgba) = window_icon_rgba(size)?;
    let mut data = vec![0u8; rgba.len()];
    for (src, dst) in rgba.chunks_exact(4).zip(data.chunks_exact_mut(4)) {
        dst[0] = src[3];
        dst[1] = src[0];
        dst[2] = src[1];
        dst[3] = src[2];
    }
    Some((width as i32, height as i32, data))
}

pub fn window_icon_rgba(size: u32) -> Option<(u32, u32, Vec<u8>)> {
    let src = load_icon("portal").or_else(embedded_portal_icon)?;
    let size = size.max(1).max(16);
    let resized = image::imageops::resize(&src, size, size, image::imageops::FilterType::Lanczos3);
    let mut out = image::RgbaImage::from_pixel(size, size, image::Rgba([12, 24, 48, 255]));
    let margin = (size / 16).max(1);
    let inner = size.saturating_sub(margin * 2).max(1);
    let portal = image::imageops::resize(
        &resized,
        inner,
        inner,
        image::imageops::FilterType::Lanczos3,
    );
    image::imageops::overlay(&mut out, &portal, margin as i64, margin as i64);
    let (width, height) = out.dimensions();
    Some((width, height, out.into_raw()))
}

fn embedded_portal_icon() -> Option<RgbaImage> {
    image::load_from_memory(include_bytes!("../../../assets/icons/ui/portal.png"))
        .ok()
        .map(|image| image.to_rgba8())
}

pub fn load_icon(key: &str) -> Option<RgbaImage> {
    let root = assets_root().join("icons");
    let path = if let Some(name) = key.strip_prefix("gem:") {
        root.join("gems").join(poe_core::gem_icon_file(name))
    } else {
        let file = match key {
            "waypoint" => "waypoint.png",
            "portal" => "portal.png",
            "quest" => "quest.png",
            "town" => "town.png",
            "trial" => "trial.png",
            "crafting" => "crafting.png",
            "portalscroll" => "portalscroll.png",
            "wisdom" => "wisdom.png",
            "transmutation" => "transmutation.png",
            "chance" => "chance.png",
            "alteration" => "alteration.png",
            "alchemy" => "alchemy.png",
            other => {
                if Path::new(other).extension().is_some() {
                    return image::open(root.join(other)).ok().map(|img| img.to_rgba8());
                }
                return None;
            }
        };
        root.join("ui").join(file)
    };
    image::open(path).ok().map(|img| img.to_rgba8())
}

#[cfg(test)]
mod tests {
    use super::*;
    use poe_core::build_route;

    #[test]
    fn vendor_buys_use_the_currency_that_gem_costs() {
        let catalog = load_catalog();
        let route = build_route(&catalog, "Witch", Some(&every_gem(&catalog)), true);
        let mut wisdom = false;
        let mut other = false;
        let mut buys = 0;
        for event in route.events.iter().filter(|event| event.kind == poe_core::EventKind::GemBuy) {
            buys += 1;
            let gem = catalog
                .gem(event.gem_id.as_deref().expect("vendor gem has an id"))
                .unwrap_or_else(|| panic!("missing {}", event.gem_id.as_deref().unwrap_or("")));
            let cost = poe_core::vendor_cost_icon(gem.required_level);
            assert_eq!(event.cost_icon, cost, "{}", gem.name);
            if cost == "wisdom" {
                wisdom = true;
            } else {
                other = true;
            }
        }
        assert!(buys > 0);
        assert!(wisdom, "a low-level vendor gem should cost a Scroll of Wisdom");
        assert!(other, "a higher-level vendor gem should cost another currency");
        assert!(route.events.iter().any(|event| {
            event.kind == poe_core::EventKind::GemPick && event.cost_icon.is_empty()
        }));
    }

    #[test]
    fn act_one_route_contains_the_coast() {
        let catalog = load_catalog();
        assert!(catalog.areas.len() > 100);
        assert!(catalog.gems.len() > 100);
        let route = build_route(&catalog, "Templar", None, true);
        assert!(route.events.iter().any(|e| e.text.contains("The Coast")));
        assert!(route.events.iter().all(|event| !event.kind.is_gem()));
        let route = build_route(&catalog, "Templar", Some(&every_gem(&catalog)), true);
        assert!(route.events.iter().any(
            |e| e.kind == poe_core::EventKind::GemPick || e.kind == poe_core::EventKind::GemBuy
        ));
        assert!(route.events.iter().any(|event| {
            event.kind.is_gem()
                && event.npc.as_deref() == Some("Tarkleigh")
                && event.text.contains("{loc}Lioneye's Watch{/}")
        }));
        for event in route.events.iter().filter(|event| event.kind.is_gem()) {
            assert!(!event.text.contains("Lower Prison"), "{}", event.text);
            let Some(npc) = event.npc.as_deref() else {
                continue;
            };
            if npc.eq_ignore_ascii_case("Siosa") {
                assert!(event.text.contains("{loc}The Library{/}"), "{}", event.text);
                continue;
            }
            if !event.text.contains(" in ") {
                continue;
            }
            let town = catalog
                .areas
                .iter()
                .find(|area| area.act == event.act && area.is_town)
                .unwrap_or_else(|| panic!("act {} has no town", event.act));
            assert!(
                event.text.contains(&format!("{{loc}}{}{{/}}", town.name)),
                "{}",
                event.text
            );
        }
    }

    #[test]
    fn hand_in_names_the_npc_and_a_portal_is_not_an_enter() {
        let catalog = load_catalog();
        let route = build_route(&catalog, "Templar", None, true);
        assert!(route.events.iter().any(|event| {
            event.text.contains("Hand in Enemy at the Gate")
                && event.detail.contains("Tarkleigh in Lioneye's Watch")
        }));
        let blank: Vec<_> = route
            .events
            .iter()
            .filter(|event| event.text.starts_with("Hand in ") && event.detail.is_empty())
            .map(|event| event.text.clone())
            .collect();
        assert!(blank.is_empty(), "{blank:?}");
        let den = route
            .events
            .iter()
            .find(|event| poe_core::plain_text(&event.text).contains("The Den"))
            .unwrap();
        let den_text = poe_core::plain_text(&den.text);
        assert!(den_text.contains("Find The Den"), "{den_text}");
        assert!(!den_text.contains("Enter"), "{den_text}");
    }

    #[test]
    fn crossroads_opens_chamber_before_fellshrine() {
        let catalog = load_catalog();
        let mut file = poe_core::CharacterFile {
            name: "Tester".into(),
            class_name: "Templar".into(),
            route: build_route(&catalog, "Templar", None, false),
            progress: Default::default(),
            user_edited: false,
        };
        for (name, id) in [
            ("The Southern Forest", "1_2_1"),
            ("The Forest Encampment", "1_2_town"),
            ("The Old Fields", "1_2_2"),
            ("The Crossroads", "1_2_3"),
        ] {
            poe_core::apply_parsed(
                &mut file,
                &poe_core::ParsedLog::Entered {
                    name: name.into(),
                    area_id: Some(id.into()),
                },
                Some(2),
            );
        }
        let shown = shown_text(&file);
        assert!(shown.contains("Chamber of Sins"), "{shown}");
        assert!(!shown.contains("Fellshrine"), "{shown}");
        for (name, id) in [
            ("The Chamber of Sins Level 1", "1_2_6_1"),
            ("The Chamber of Sins Level 2", "1_2_6_2"),
        ] {
            poe_core::apply_parsed(
                &mut file,
                &poe_core::ParsedLog::Entered {
                    name: name.into(),
                    area_id: Some(id.into()),
                },
                Some(2),
            );
        }
        poe_core::apply_parsed(
            &mut file,
            &poe_core::ParsedLog::Entered {
                name: "The Forest Encampment".into(),
                area_id: Some("1_2_town".into()),
            },
            Some(2),
        );
        let shown = shown_text(&file);
        assert!(shown.contains("Intruders in Black"), "{shown}");
        assert!(!shown.contains("Fellshrine"), "{shown}");
        poe_core::apply_parsed(
            &mut file,
            &poe_core::ParsedLog::Entered {
                name: "The Crossroads".into(),
                area_id: Some("1_2_3".into()),
            },
            Some(2),
        );
        let shown = shown_text(&file);
        assert!(shown.contains("Fellshrine"), "{shown}");
    }

    #[test]
    fn coast_after_submerged_passage_points_at_tidal_island() {
        let catalog = load_catalog();
        let mut file = poe_core::CharacterFile {
            name: "Tester".into(),
            class_name: "Duelist".into(),
            route: build_route(&catalog, "Duelist", None, true),
            progress: Default::default(),
            user_edited: false,
        };
        for (name, id) in [
            ("The Twilight Strand", "1_1_1"),
            ("Lioneye's Watch", "1_1_town"),
            ("The Coast", "1_1_2"),
            ("The Mud Flats", "1_1_3"),
            ("The Submerged Passage", "1_1_4_1"),
        ] {
            poe_core::apply_parsed(
                &mut file,
                &poe_core::ParsedLog::Entered {
                    name: name.into(),
                    area_id: Some(id.into()),
                },
                Some(1),
            );
        }
        let shown = shown_text(&file);
        assert!(shown.contains("Waypoint to The Coast"), "{shown}");
        poe_core::apply_parsed(
            &mut file,
            &poe_core::ParsedLog::Entered {
                name: "The Coast".into(),
                area_id: Some("1_1_2".into()),
            },
            Some(1),
        );
        let shown = shown_text(&file);
        assert!(shown.starts_with("The Coast"), "{shown}");
        assert!(shown.contains("Enter The Tidal Island"), "{shown}");
        assert!(
            !file.progress.skipped_groups.iter().any(|id| id == "g1-8"),
            "{:?}",
            file.progress.skipped_groups
        );
        poe_core::apply_parsed(
            &mut file,
            &poe_core::ParsedLog::Entered {
                name: "The Tidal Island".into(),
                area_id: Some("1_1_2a".into()),
            },
            Some(1),
        );
        assert!(
            !file.progress.skipped_groups.iter().any(|id| id == "g1-8"),
            "{:?}",
            file.progress.skipped_groups
        );
        let shown = shown_text(&file);
        assert!(shown.contains("Hailrake"), "{shown}");
    }

    #[test]
    fn relevel_returns_to_hillock_on_the_twilight_strand() {
        let catalog = load_catalog();
        let route = build_route(&catalog, "Witch", None, true);
        let events = route.events.len();
        let mut file = poe_core::CharacterFile {
            name: "Witch".into(),
            class_name: "Witch".into(),
            route,
            progress: Default::default(),
            user_edited: false,
        };
        file.route.pob_url = "keep".into();
        for (name, id) in [
            ("The Twilight Strand", "1_1_1"),
            ("Lioneye's Watch", "1_1_town"),
            ("The Coast", "1_1_2"),
            ("The Mud Flats", "1_1_3"),
        ] {
            poe_core::apply_parsed(
                &mut file,
                &poe_core::ParsedLog::Entered {
                    name: name.into(),
                    area_id: Some(id.into()),
                },
                Some(1),
            );
        }
        file.progress.level = 12;
        file.progress.completed.insert("already-done".into());
        poe_core::restart_at_beach(&mut file);
        assert_eq!(file.route.events.len(), events);
        assert_eq!(file.route.pob_url, "keep");
        assert_eq!(file.class_name, "Witch");
        assert_eq!(file.progress.level, 1);
        assert_eq!(file.progress.area_id, "1_1_1");
        assert_eq!(file.progress.zone_name, "The Twilight Strand");
        assert!(file.progress.completed.is_empty());
        assert!(file.progress.skipped_groups.is_empty());
        let shown = shown_text(&file);
        assert!(shown.contains("The Twilight Strand"), "{shown}");
        assert!(shown.contains("Hillock"), "{shown}");
        assert!(!shown.contains("The Mud Flats"), "{shown}");
    }

    #[test]
    fn relevel_from_act_six_strand_returns_to_act_one() {
        let catalog = load_catalog();
        let route = build_route(&catalog, "Scion", None, true);
        let mut file = poe_core::CharacterFile {
            name: "Scion".into(),
            class_name: "Scion".into(),
            route,
            progress: Default::default(),
            user_edited: true,
        };
        let beach_ids: Vec<String> = file
            .route
            .groups
            .iter()
            .filter(|group| {
                group.start.iter().any(|trigger| {
                    matches!(
                        trigger,
                        poe_core::TriggerKind::ZoneEnter { area_id } if area_id == "1_1_1"
                    )
                })
            })
            .map(|group| group.id.clone())
            .collect();
        assert!(!beach_ids.is_empty());
        let mut orphan = Vec::new();
        file.route.groups.retain(|group| {
            if beach_ids.iter().any(|id| id == &group.id) {
                orphan.extend(group.event_ids.clone());
                false
            } else {
                true
            }
        });
        for event in &mut file.route.events {
            if orphan.iter().any(|id| id == &event.id) {
                event.start = vec![poe_core::TriggerKind::ZoneName {
                    name: "Find and kill Hillock".into(),
                }];
            }
        }
        poe_core::note_current_zone(&mut file, "The Twilight Strand", Some("2_6_1"), Some(6));
        assert_eq!(file.progress.area_id, "2_6_1");
        poe_core::restart_at_beach(&mut file);
        assert_eq!(file.progress.area_id, "1_1_1");
        assert_eq!(file.progress.highest_act, 1);
        assert!(!file.progress.entered_areas.contains("2_6_1"));
        assert!(file.progress.skipped_groups.is_empty());
        let anchor = file.progress.anchor_group.as_deref().unwrap();
        let group = file
            .route
            .groups
            .iter()
            .find(|group| group.id == anchor)
            .unwrap();
        assert_eq!(group.act, 1);
        assert_eq!(group.name, "The Twilight Strand");
        let shown = shown_text(&file);
        assert!(shown.contains("Hillock"), "{shown}");
        assert!(!shown.contains("Fallen from Grace"), "{shown}");
    }

    #[test]
    fn pob_bandit_selects_the_act_two_route() {
        let catalog = load_catalog();
        let kill = build_route(&catalog, "Ranger", None, false);
        let kill_text = event_text(&kill);
        assert!(kill_text
            .iter()
            .any(|line| line.contains("Deal with the Bandits")));
        let alira_at = kill_text
            .iter()
            .position(|line| line.contains("Alira Darktongue"))
            .unwrap();
        let weaver_at = kill_text
            .iter()
            .position(|line| line.contains("The Weaver"))
            .unwrap();
        let kraityn_at = kill_text
            .iter()
            .position(|line| line.contains("Kraityn"))
            .unwrap();
        let oak_at = kill_text
            .iter()
            .position(|line| line.contains("Oak"))
            .unwrap();
        let hand_at = kill_text
            .iter()
            .position(|line| line.contains("Deal with the Bandits"))
            .unwrap();
        assert!(
            alira_at < weaver_at
                && weaver_at < kraityn_at
                && kraityn_at < oak_at
                && oak_at < hand_at
        );

        let mut file = poe_core::CharacterFile {
            name: "Ranger".into(),
            class_name: "Ranger".into(),
            route: kill,
            progress: Default::default(),
            user_edited: false,
        };
        let build = poe_core::PobBuild {
            class_name: "Ranger".into(),
            ascendancy: String::new(),
            notes: String::new(),
            gems: Vec::new(),
            specs: Vec::new(),
            bandit: poe_core::BanditChoice::Alira,
        };
        poe_core::install_build(&mut file, &catalog, &build, false);
        let help = event_text(&file.route);
        assert!(!help
            .iter()
            .any(|line| line.contains("Deal with the Bandits")));
        let kraityn_at = help
            .iter()
            .position(|line| line.contains("Kraityn"))
            .unwrap();
        let oak_at = help.iter().position(|line| line.contains("Oak")).unwrap();
        let help_at = help
            .iter()
            .position(|line| line.contains("Help Alira"))
            .unwrap();
        assert!(kraityn_at < oak_at && oak_at < help_at);

        let mut build = build;
        for (choice, earlier, middle, helped) in [
            (
                poe_core::BanditChoice::Kraityn,
                "Alira Darktongue",
                "Oak",
                "Help Kraityn",
            ),
            (
                poe_core::BanditChoice::Oak,
                "Kraityn",
                "Alira Darktongue",
                "help Oak",
            ),
        ] {
            build.bandit = choice;
            poe_core::install_build(&mut file, &catalog, &build, false);
            let lines = event_text(&file.route);
            assert!(!lines
                .iter()
                .any(|line| line.contains("Deal with the Bandits")));
            let earlier_at = lines
                .iter()
                .position(|line| line.contains(earlier))
                .unwrap();
            let middle_at = lines.iter().position(|line| line.contains(middle)).unwrap();
            let helped_at = lines.iter().position(|line| line.contains(helped)).unwrap();
            assert!(
                earlier_at < middle_at && middle_at < helped_at,
                "{choice:?}"
            );
        }
    }

    #[test]
    fn an_imported_scion_does_not_schedule_its_free_gems() {
        let catalog = load_catalog();
        let mut file = poe_core::CharacterFile {
            name: "Scion".into(),
            class_name: "Scion".into(),
            route: build_route(&catalog, "Scion", None, true),
            progress: Default::default(),
            user_edited: false,
        };
        let build = poe_core::PobBuild {
            class_name: "Scion".into(),
            ascendancy: String::new(),
            notes: String::new(),
            gems: vec![
                "Spectral Throw".into(),
                "Prismatic Burst".into(),
                "Fireball".into(),
            ],
            specs: Vec::new(),
            bandit: Default::default(),
        };
        poe_core::install_build(&mut file, &catalog, &build, true);
        assert!(!file.route.pob_gems.iter().any(|gem| gem == "Spectral Throw"));
        assert!(!file
            .route
            .pob_gems
            .iter()
            .any(|gem| gem == "Prismatic Burst"));
        assert!(file.route.pob_gems.iter().any(|gem| gem == "Fireball"));
        assert!(file.route.events.iter().all(|event| {
            event.gem_name.as_deref() != Some("Spectral Throw")
                && event.gem_name.as_deref() != Some("Prismatic Burst Support")
        }));
        assert!(file
            .route
            .events
            .iter()
            .any(|event| event.gem_name.as_deref() == Some("Fireball")));
        let spectral: std::collections::HashSet<String> =
            ["Spectral Throw"].into_iter().map(str::to_string).collect();
        let duelist = build_route(&catalog, "Duelist", Some(&spectral), true);
        assert!(duelist
            .events
            .iter()
            .any(|event| event.gem_name.as_deref() == Some("Spectral Throw")));
    }

    #[test]
    fn logout_ends_in_town_and_a_reward_window_gives_one_gem() {
        let catalog = load_catalog();
        let route = build_route(&catalog, "Witch", None, true);
        let mut logouts = 0;
        for (index, group) in route.groups.iter().enumerate() {
            let events: Vec<_> = group
                .event_ids
                .iter()
                .filter_map(|id| route.events.iter().find(|event| &event.id == id))
                .collect();
            let town = catalog
                .areas
                .iter()
                .find(|area| area.act == group.act && area.is_town)
                .unwrap_or_else(|| panic!("act {} has no town", group.act));
            let Some(pos) = events.iter().position(|event| {
                event.text == "Logout or Portal to Town"
                    || ((event.text == "Take town portal" || event.text == "Town portal")
                        && group.name != town.name)
            }) else {
                continue;
            };
            logouts += 1;
            assert_eq!(
                pos,
                events.len() - 1,
                "{} keeps a step after logout",
                group.name
            );
            let Some(next) = route.groups.get(index + 1) else {
                continue;
            };
            let first = next.event_ids.first().and_then(|id| {
                route.events.iter().find(|event| &event.id == id)
            });
            let Some(first) = first else { continue };
            let plain = poe_core::plain_text(&first.text);
            if plain.starts_with("Waypoint") || plain.starts_with("Enter") {
                continue;
            }
            assert_eq!(
                next.name, town.name,
                "after logout in {}, '{plain}' is still in {}",
                group.name, next.name
            );
        }
        assert!(logouts > 10, "logouts across the acts: {logouts}");
        let brutes: Vec<_> = route
            .events
            .iter()
            .filter(|event| event.text.contains("Hand in The Caged Brute"))
            .collect();
        assert_eq!(brutes.len(), 2);
        assert!(brutes[0].detail.contains("Nessa"), "{}", brutes[0].detail);
        assert!(
            brutes[1].detail.contains("Tarkleigh"),
            "{}",
            brutes[1].detail
        );

        let filter: std::collections::HashSet<String> = [
            "Freezing Pulse",
            "Rolling Magma",
            "Frost Bomb",
            "Frostblink",
            "Contagion",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        let route = build_route(&catalog, "Witch", Some(&filter), true);
        let enemy: Vec<_> = route
            .events
            .iter()
            .filter(|event| event.id.starts_with("gem-a1q1-"))
            .collect();
        assert_eq!(enemy.iter().filter(|event| event.kind == poe_core::EventKind::GemPick).count(), 1);
        assert_eq!(enemy.iter().filter(|event| event.kind == poe_core::EventKind::GemBuy).count(), 1);
        assert!(enemy.iter().any(|event| event.kind == poe_core::EventKind::GemBuy && event.text.contains("Nessa")));
        let eggs: Vec<_> = route
            .events
            .iter()
            .filter(|event| event.id.starts_with("gem-a1q4-"))
            .collect();
        assert_eq!(eggs.iter().filter(|event| event.kind == poe_core::EventKind::GemPick).count(), 2);
        assert_eq!(eggs.iter().filter(|event| event.kind == poe_core::EventKind::GemBuy).count(), 1);
        assert!(eggs.iter().filter(|event| event.kind == poe_core::EventKind::GemPick).all(|event| event.text.contains("Tarkleigh")));
    }

    fn every_gem(catalog: &poe_core::Catalog) -> std::collections::HashSet<String> {
        catalog.gems.iter().map(|gem| gem.name.clone()).collect()
    }

    fn shown_text(file: &poe_core::CharacterFile) -> String {
        poe_core::visible_lines(&file.route, &file.progress, true, false, true)
            .into_iter()
            .map(|line| poe_core::plain_text(&line.text))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn event_text(route: &poe_core::CharacterRoute) -> Vec<String> {
        route
            .events
            .iter()
            .map(|event| poe_core::plain_text(&event.text))
            .collect()
    }
}
