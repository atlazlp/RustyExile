use crate::model::{
    AreaInfo, CampaignEvent, Catalog, CharacterRoute, EventGroup, EventKind, GemSource, QuestGem,
    TriggerKind,
};
use std::collections::HashSet;

#[derive(Debug)]
struct RawStep {
    text: String,
    detail: Vec<String>,
    icon: String,
    area_id: Option<String>,
    quest_ids: Vec<String>,
    act: u8,
}

pub fn build_route(
    catalog: &Catalog,
    class_name: &str,
    gem_filter: Option<&HashSet<String>>,
    league_start: bool,
) -> CharacterRoute {
    let mut steps = Vec::new();
    for (act, text) in &catalog.routes {
        steps.extend(parse_act(catalog, *act, text, league_start));
    }
    let mut carry: Option<String> = None;
    for step in &mut steps {
        if step.area_id.is_none() {
            step.area_id = carry.clone();
        } else {
            carry = step.area_id.clone();
        }
    }

    let mut route = CharacterRoute::default();
    let mut index = 0usize;
    while index < steps.len() {
        let area = steps[index].area_id.clone();
        let mut end = index + 1;
        while end < steps.len() && steps[end].area_id == area {
            end += 1;
        }
        let end_area = steps.get(end).and_then(|s| s.area_id.clone());
        let act = steps[index].act;
        let zone_name = area
            .as_ref()
            .and_then(|id| catalog.area(id).map(|a| a.name.clone()))
            .unwrap_or_else(|| steps[index].text.clone());
        let group_id = format!("g{act}-{index}");
        let start = triggers_for(area.as_deref(), &zone_name);
        let end_triggers = end_area
            .as_ref()
            .map(|id| {
                let name = catalog.area(id).map(|a| a.name.clone()).unwrap_or_default();
                triggers_for(Some(id), &name)
            })
            .unwrap_or_default();
        let mut ids = Vec::new();
        for step in &steps[index..end] {
            let id = format!("a{}-{}", step.act, route.events.len());
            ids.push(id.clone());
            route.events.push(CampaignEvent {
                id,
                act: step.act,
                text: step.text.clone(),
                detail: step.detail.join("\n"),
                icon: step.icon.clone(),
                kind: EventKind::Step,
                start: start.clone(),
                end: end_triggers.clone(),
                gem_id: None,
                gem_name: None,
                npc: None,
                zone_name: Some(zone_name.clone()),
            });
            for quest_id in &step.quest_ids {
                push_quest_gems(
                    catalog,
                    &mut route,
                    &mut ids,
                    quest_id,
                    class_name,
                    gem_filter,
                    step.act,
                    &zone_name,
                    &start,
                    &end_triggers,
                );
            }
        }
        if area.is_some() {
            route.groups.push(EventGroup {
                id: group_id,
                act,
                name: zone_name,
                start,
                event_ids: ids,
            });
        }
        index = end;
    }
    route
}

fn push_quest_gems(
    catalog: &Catalog,
    route: &mut CharacterRoute,
    ids: &mut Vec<String>,
    quest_id: &str,
    class_name: &str,
    gem_filter: Option<&HashSet<String>>,
    act: u8,
    zone_name: &str,
    start: &[TriggerKind],
    end: &[TriggerKind],
) {
    let Some(quest) = catalog.quest(quest_id) else {
        return;
    };
    if class_name.is_empty() {
        return;
    }
    for gem in &quest.gems {
        if !gem.classes.iter().any(|c| c.eq_ignore_ascii_case(class_name)) {
            continue;
        }
        let Some(info) = catalog.gem(&gem.gem_id) else {
            continue;
        };
        if let Some(filter) = gem_filter {
            if !filter.iter().any(|n| n.eq_ignore_ascii_case(&info.name)) {
                continue;
            }
        }
        let kind = match gem.source {
            GemSource::Reward => EventKind::GemPick,
            GemSource::Vendor => EventKind::GemBuy,
        };
        let verb = match gem.source {
            GemSource::Reward => "Pick",
            GemSource::Vendor => "Buy",
        };
        let npc = if gem.npc.is_empty() {
            quest.npc.clone()
        } else {
            gem.npc.clone()
        };
        let id = format!("gem-{}-{}", quest_id, info.id.replace('/', "_"));
        if route.events.iter().any(|e| e.id == id) {
            if !ids.iter().any(|existing| existing == &id) {
                ids.push(id);
            }
            continue;
        }
        ids.push(id.clone());
        route.events.push(CampaignEvent {
            id,
            act,
            text: format!("{verb} {} from {npc} in {zone_name}", info.name),
            detail: String::new(),
            icon: format!("gem:{}", info.name),
            kind,
            start: start.to_vec(),
            end: end.to_vec(),
            gem_id: Some(info.id.clone()),
            gem_name: Some(info.name.clone()),
            npc: Some(npc),
            zone_name: Some(zone_name.to_string()),
        });
    }
}

fn triggers_for(area_id: Option<&str>, name: &str) -> Vec<TriggerKind> {
    if let Some(id) = area_id {
        vec![TriggerKind::ZoneEnter {
            area_id: id.to_string(),
        }]
    } else if !name.is_empty() {
        vec![TriggerKind::ZoneName {
            name: name.to_string(),
        }]
    } else {
        Vec::new()
    }
}

fn parse_act(catalog: &Catalog, act: u8, text: &str, league_start: bool) -> Vec<RawStep> {
    let filtered = apply_conditionals(text, league_start);
    let mut steps: Vec<RawStep> = Vec::new();
    for line in filtered.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("#sub") {
            if let Some(last) = steps.last_mut() {
                let tip = rest.trim().trim_start_matches('#').trim();
                if !tip.is_empty() {
                    last.detail.push(render_line(catalog, tip).0);
                }
            }
            continue;
        }
        if trimmed.starts_with('#') {
            continue;
        }
        let (body, label) = split_label(trimmed);
        let (rendered, icon, area_id, quest_ids) = render_line(catalog, body);
        let text = if !label.is_empty() && is_movement(body) {
            if body.contains("{waypoint") {
                format!("Waypoint to {label}")
            } else if body.contains("{quest|") {
                format!("Hand in {label}")
            } else {
                label
            }
        } else if rendered.trim().is_empty() {
            label
        } else {
            rendered
        };
        if text.trim().is_empty() {
            continue;
        }
        steps.push(RawStep {
            text: text.trim().to_string(),
            detail: Vec::new(),
            icon,
            area_id,
            quest_ids,
            act,
        });
    }
    steps
}

fn is_movement(body: &str) -> bool {
    body.contains("{enter|") || body.contains("{waypoint|") || body.contains("{waypoint_get}")
}

fn split_label(line: &str) -> (&str, String) {
    let mut in_token = false;
    for (i, ch) in line.char_indices() {
        match ch {
            '{' => in_token = true,
            '}' => in_token = false,
            '#' if !in_token => {
                return (line[..i].trim(), line[i + 1..].trim().to_string());
            }
            _ => {}
        }
    }
    (line, String::new())
}

fn render_line(catalog: &Catalog, body: &str) -> (String, String, Option<String>, Vec<String>) {
    let mut text = String::new();
    let mut icon = "quest".to_string();
    let mut area_id = None;
    let mut quest_ids = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find('{') {
        text.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            text.push_str(rest);
            break;
        };
        let token = &after[..end];
        let (piece, token_icon, token_area, token_quests) = render_token(catalog, token);
        text.push_str(&piece);
        if token_icon != "quest" || icon == "quest" {
            icon = token_icon;
        }
        if token_area.is_some() {
            area_id = token_area;
        }
        quest_ids.extend(token_quests);
        rest = &after[end + 1..];
    }
    if !rest.contains('{') {
        text.push_str(rest);
    }
    let cleaned = text
        .replace('➞', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    (cleaned, icon, area_id, quest_ids)
}

fn render_token(
    catalog: &Catalog,
    token: &str,
) -> (String, String, Option<String>, Vec<String>) {
    let mut parts = token.split('|');
    let kind = parts.next().unwrap_or("");
    let args: Vec<&str> = parts.collect();
    let first = args.first().copied().unwrap_or("");
    match kind {
        "enter" | "area" | "waypoint" => {
            let area = catalog.area(first);
            let name = area.map(|a| a.name.clone()).unwrap_or_else(|| first.to_string());
            let icon = if kind == "waypoint" {
                "waypoint"
            } else if area.is_some_and(|a| a.is_town) {
                "town"
            } else {
                "portal"
            };
            let label = if kind == "waypoint" {
                format!("Waypoint to {name}")
            } else {
                name
            };
            (label, icon.into(), Some(first.to_string()), Vec::new())
        }
        "waypoint_get" => ("Waypoint".into(), "waypoint".into(), None, Vec::new()),
        "kill" => (first.to_string(), "quest".into(), None, Vec::new()),
        "quest" => {
            let ids: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
            let name = ids
                .first()
                .and_then(|id| catalog.quest(id).map(|q| q.name.clone()))
                .unwrap_or_else(|| first.to_string());
            (format!("Hand in {name}"), "quest".into(), None, ids)
        }
        "quest_text" | "generic" | "arena" => (first.to_string(), "quest".into(), None, Vec::new()),
        "logout" => ("Logout".into(), "portalscroll".into(), None, Vec::new()),
        "trial" => (
            "Trial of Ascendancy".into(),
            "trial".into(),
            None,
            Vec::new(),
        ),
        "crafting" => ("Crafting recipe".into(), "crafting".into(), None, Vec::new()),
        "portal" => (
            if first == "use" {
                "Take town portal".into()
            } else {
                "Town portal".into()
            },
            "portalscroll".into(),
            None,
            Vec::new(),
        ),
        "ascend" => (
            format!("Ascend ({first})"),
            "trial".into(),
            None,
            Vec::new(),
        ),
        "dir" => (direction(first).into(), "quest".into(), None, Vec::new()),
        _ => (first.to_string(), "quest".into(), None, Vec::new()),
    }
}

fn direction(raw: &str) -> &'static str {
    match raw.parse::<i32>().unwrap_or(0).rem_euclid(360) {
        0 => "east",
        45 => "north-east",
        90 => "north",
        135 => "north-west",
        180 => "west",
        225 => "south-west",
        270 => "south",
        315 => "south-east",
        _ => "ahead",
    }
}

fn apply_conditionals(src: &str, league_start: bool) -> String {
    let mut out = String::new();
    let mut stack = vec![true];
    for line in src.lines() {
        let t = line.trim();
        if let Some(flag) = t.strip_prefix("#ifdef ") {
            let ok = flag.trim() == "LEAGUE_START" && league_start;
            stack.push(*stack.last().unwrap_or(&true) && ok);
            continue;
        }
        if let Some(flag) = t.strip_prefix("#ifndef ") {
            let ok = !(flag.trim() == "LEAGUE_START" && league_start);
            stack.push(*stack.last().unwrap_or(&true) && ok);
            continue;
        }
        if t == "#endif" {
            if stack.len() > 1 {
                stack.pop();
            }
            continue;
        }
        if *stack.last().unwrap_or(&true) {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

pub fn install_build(
    file: &mut crate::model::CharacterFile,
    catalog: &Catalog,
    build: &crate::pob::PobBuild,
    league_start: bool,
) {
    if !build.class_name.is_empty() {
        file.class_name = build.class_name.clone();
        file.progress.class_name = build.class_name.clone();
    }
    let filter: HashSet<String> = build.gems.iter().cloned().collect();
    let fresh = build_route(catalog, &file.class_name, Some(&filter), league_start);
    if file.user_edited {
        file.route.events.retain(|event| !event.kind.is_gem());
        for group in &mut file.route.groups {
            group.event_ids.retain(|id| file.route.events.iter().any(|event| &event.id == id));
        }
        for event in fresh.events.into_iter().filter(|event| event.kind.is_gem()) {
            let zone = event.zone_name.clone();
            let id = event.id.clone();
            file.route.events.push(event);
            if let Some(group) = file.route.groups.iter_mut().find(|group| Some(group.name.as_str()) == zone.as_deref()) {
                group.event_ids.push(id);
            }
        }
    } else {
        file.route = fresh;
    }
    crate::pob::apply_pob(&mut file.route, build);
}

pub fn area_named(areas: &[AreaInfo]) -> impl Fn(&str) -> Option<u8> + '_ {
    move |id| areas.iter().find(|a| a.id == id).map(|a| a.act)
}

pub fn class_matches(gem: &QuestGem, class_name: &str) -> bool {
    gem.classes.iter().any(|c| c.eq_ignore_ascii_case(class_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GemInfo, QuestInfo};

    fn catalog() -> Catalog {
        Catalog {
            areas: vec![
                AreaInfo {
                    id: "1_1_town".into(),
                    name: "Lioneye's Watch".into(),
                    act: 1,
                    is_town: true,
                    has_waypoint: true,
                },
                AreaInfo {
                    id: "1_1_2".into(),
                    name: "The Coast".into(),
                    act: 1,
                    is_town: false,
                    has_waypoint: true,
                },
            ],
            gems: vec![GemInfo {
                id: "Metadata/Items/Gems/SkillGemFireball".into(),
                name: "Fireball".into(),
                primary_attribute: "intelligence".into(),
                required_level: 1,
                is_support: false,
            }],
            quests: vec![QuestInfo {
                id: "a1q1".into(),
                name: "Enemy at the Gate".into(),
                act: 1,
                npc: "Tarkleigh".into(),
                gems: vec![QuestGem {
                    gem_id: "Metadata/Items/Gems/SkillGemFireball".into(),
                    classes: vec!["Witch".into()],
                    npc: "Tarkleigh".into(),
                    source: GemSource::Reward,
                }],
            }],
            routes: vec![(
                1,
                "\
#section Act 1
➞ {enter|1_1_town} #Lioneye's Watch
Hand in {quest|a1q1} #Enemy at the Gate
#sub Talk to Tarkleigh
➞ {enter|1_1_2} #The Coast
#ifdef LEAGUE_START
Get {waypoint_get}
#endif
"
                .into(),
            )],
        }
    }

    #[test]
    fn builds_zone_steps_and_class_gems() {
        let route = build_route(&catalog(), "Witch", None, true);
        assert!(route.events.iter().any(|e| e.text == "The Coast"));
        assert!(route.events.iter().any(|e| e.text.contains("Fireball")));
        assert!(route.events.iter().any(|e| e.detail.contains("Tarkleigh")));
        let coast = route.events.iter().find(|e| e.text == "The Coast").unwrap();
        assert!(matches!(
            coast.start[0],
            TriggerKind::ZoneEnter { ref area_id } if area_id == "1_1_2"
        ));
    }
}
