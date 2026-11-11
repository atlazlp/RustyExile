use image::RgbaImage;
use poe_core::{
    AreaInfo, Catalog, GemInfo, GemSource, QuestGem, QuestInfo, RegexPreset,
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
                required_level: value.get("required_level").and_then(|v| v.as_u64()).unwrap_or(1) as u32,
                is_support: value.get("is_support").and_then(|v| v.as_bool()).unwrap_or(false),
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
            let act = value.get("act").and_then(|v| v.as_str()).unwrap_or("1").parse().unwrap_or(1);
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

fn collect_gems(bucket: Option<&serde_json::Value>, source: GemSource, fallback_npc: &str, out: &mut Vec<QuestGem>) {
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
        let resized =
            image::imageops::resize(&src, size, size, image::imageops::FilterType::Lanczos3);
        let mut out = image::RgbaImage::from_pixel(size, size, image::Rgba([12, 24, 48, 255]));
        let margin = (size / 16).max(1);
        let inner = size.saturating_sub(margin * 2).max(1);
        let portal =
            image::imageops::resize(&resized, inner, inner, image::imageops::FilterType::Lanczos3);
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
        assert!(route.events.iter().any(|e| e.kind == poe_core::EventKind::GemPick || e.kind == poe_core::EventKind::GemBuy));
    }
}
