use image::RgbaImage;
use poe_core::{AreaInfo, Catalog, GemInfo, GemSource, QuestGem, QuestInfo, RegexPreset};
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
    }
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
                for offer in offers.values() {
                    if npc.is_empty() {
                        if let Some(quest_npc) = offer.get("quest_npc").and_then(|v| v.as_str()) {
                            npc = quest_npc.to_string();
                        }
                    }
                    collect_gems(offer.get("quest"), GemSource::Reward, &npc, &mut gems);
                    collect_gems(offer.get("vendor"), GemSource::Vendor, &npc, &mut gems);
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
    fn act_one_route_contains_the_coast() {
        let catalog = load_catalog();
        assert!(catalog.areas.len() > 100);
        assert!(catalog.gems.len() > 100);
        let route = build_route(&catalog, "Templar", None, true);
        assert!(route.events.iter().any(|e| e.text.contains("The Coast")));
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
        assert!(shown.contains("Fellshrine"), "{shown}");
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
