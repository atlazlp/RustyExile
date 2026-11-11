use std::collections::HashSet;

use crate::logparse::ParsedLog;
use crate::model::{
    CampaignEvent, CharacterFile, CharacterProgress, CharacterRoute, EventKind, TriggerKind,
    VisibleLine,
};

pub fn trigger_fired(trigger: &TriggerKind, progress: &CharacterProgress) -> bool {
    match trigger {
        TriggerKind::ZoneEnter { area_id } => progress.entered_areas.contains(area_id),
        TriggerKind::ZoneName { name } => progress
            .entered_names
            .iter()
            .any(|n| n.eq_ignore_ascii_case(name)),
        TriggerKind::ActStart { act } => progress.highest_act >= *act,
        TriggerKind::ActEnd { act } => progress.highest_act > *act,
        TriggerKind::LevelAtLeast { level } => progress.level >= *level,
        TriggerKind::PassiveAllocated { id } => progress.passives.contains(id),
        TriggerKind::Manual => false,
    }
}

fn frontier_index(route: &CharacterRoute, progress: &CharacterProgress) -> Option<usize> {
    let mut current = None;
    for (index, group) in route.groups.iter().enumerate() {
        if !group_passed(group, progress) {
            break;
        }
        current = Some(index);
    }
    current
}

fn group_passed(group: &crate::model::EventGroup, progress: &CharacterProgress) -> bool {
    progress.skipped_groups.iter().any(|id| id == &group.id)
        || group.start.iter().any(|trigger| trigger_fired(trigger, progress))
}

fn viewed_index(route: &CharacterRoute, progress: &CharacterProgress) -> Option<usize> {
    let base = frontier_index(route, progress)?;
    if route.groups.is_empty() {
        return None;
    }
    let max = route.groups.len() as i32 - 1;
    Some((base as i32 + progress.view_delta).clamp(0, max) as usize)
}

pub fn nudge_events(file: &mut CharacterFile, delta: i32) {
    let Some(base) = frontier_index(&file.route, &file.progress) else {
        return;
    };
    if file.route.groups.is_empty() {
        return;
    }
    let max = file.route.groups.len() as i32 - 1;
    let next = (base as i32 + file.progress.view_delta + delta).clamp(0, max);
    file.progress.view_delta = next - base as i32;
}

fn start_key(group: &crate::model::EventGroup) -> Option<String> {
    group.start.iter().find_map(|trigger| match trigger {
        TriggerKind::ZoneEnter { area_id } => Some(format!("id:{area_id}")),
        TriggerKind::ZoneName { name } => Some(format!("name:{}", name.to_ascii_lowercase())),
        _ => None,
    })
}

fn furthest_reached(route: &CharacterRoute, progress: &CharacterProgress) -> Option<usize> {
    let mut seen = HashSet::new();
    let mut furthest = None;
    for (index, group) in route.groups.iter().enumerate() {
        let Some(key) = start_key(group) else {
            continue;
        };
        if !seen.insert(key) {
            continue;
        }
        if group.start.iter().any(|trigger| trigger_fired(trigger, progress)) {
            furthest = Some(index);
        }
    }
    furthest
}

pub fn sync_skipped(file: &mut CharacterFile) {
    let route = &file.route;
    let progress = &mut file.progress;
    let still_skipped: Vec<String> = progress
        .skipped_groups
        .iter()
        .filter(|id| match route.groups.iter().find(|group| &&group.id == id) {
            Some(group) => !group.start.iter().any(|trigger| trigger_fired(trigger, progress)),
            None => false,
        })
        .cloned()
        .collect();
    progress.skipped_groups = still_skipped;
    let Some(target) = furthest_reached(route, progress) else {
        return;
    };
    let mut gap = None;
    for (index, group) in route.groups.iter().enumerate() {
        if index >= target {
            break;
        }
        if !group_passed(group, progress) {
            gap = Some(index);
            break;
        }
    }
    let Some(gap) = gap else {
        return;
    };
    for group in &route.groups[gap..target] {
        if group_passed(group, progress) {
            continue;
        }
        progress.skipped_groups.push(group.id.clone());
    }
}

pub fn event_visible(
    event: &CampaignEvent,
    route: &CharacterRoute,
    progress: &CharacterProgress,
) -> bool {
    if progress.completed.contains(&event.id) {
        return false;
    }
    let Some(index) = frontier_index(route, progress) else {
        return false;
    };
    let group = &route.groups[index];
    if group.event_ids.iter().any(|id| id == &event.id) {
        return true;
    }
    route
        .groups
        .get(index + 1)
        .and_then(|next| next.event_ids.first())
        .is_some_and(|id| id == &event.id)
}

pub fn visible_lines(
    route: &CharacterRoute,
    progress: &CharacterProgress,
    show_acts: bool,
    show_gems: bool,
    show_tips: bool,
) -> Vec<VisibleLine> {
    let mut lines = Vec::new();
    if show_acts || show_gems {
        if let Some(index) = viewed_index(route, progress) {
            let group = &route.groups[index];
            let mut body = Vec::new();
            for id in &group.event_ids {
                let Some(event) = route.events.iter().find(|event| &event.id == id) else {
                    continue;
                };
                if !shown(event, show_acts, show_gems) || progress.completed.contains(&event.id) {
                    continue;
                }
                if is_zone_enter(event, group) {
                    continue;
                }
                body.push(line_from(event, show_tips));
            }
            let preview = route.groups.get(index + 1).and_then(|next| {
                let id = next.event_ids.first()?;
                let event = route.events.iter().find(|event| &event.id == id)?;
                if !shown(event, show_acts, show_gems) || progress.completed.contains(&event.id) {
                    return None;
                }
                Some(line_from(event, show_tips))
            });
            if !body.is_empty() || preview.is_some() {
                lines.push(title_line(&group.id, &group.name, group.act));
                lines.extend(body);
                if let Some(preview) = preview {
                    lines.push(preview);
                }
            }
        }
    }
    if show_gems {
        let lilly: Vec<_> = route
            .events
            .iter()
            .filter(|event| event.id.starts_with("gem-lilly-") && !progress.completed.contains(&event.id))
            .filter(|_| progress.area_id == "2_6_town" && progress.entered_areas.contains("2_6_1"))
            .cloned()
            .collect();
        if !lilly.is_empty() {
            lines.push(title_line("lilly", "Lilly Roth", 6));
            lines.extend(lilly.iter().map(|event| line_from(event, false)));
        }
        let trials: Vec<_> = route
            .events
            .iter()
            .filter(|event| event.id.starts_with("gem-trial-") && !progress.completed.contains(&event.id))
            .filter(|_| progress.trial_gems)
            .cloned()
            .collect();
        if !trials.is_empty() {
            lines.push(title_line("trial", "Aspirant's Trial", progress.highest_act));
            lines.extend(trials.iter().map(|event| line_from(event, false)));
        }
    }
    lines
}

fn shown(event: &CampaignEvent, show_acts: bool, show_gems: bool) -> bool {
    if event.id.starts_with("gem-lilly-") || event.id.starts_with("gem-trial-") {
        return false;
    }
    if event.kind.is_gem() {
        show_gems
    } else {
        show_acts
    }
}

fn is_zone_enter(event: &CampaignEvent, group: &crate::model::EventGroup) -> bool {
    event.kind == EventKind::Step
        && crate::plain_text(&event.text).eq_ignore_ascii_case(&format!("Enter {}", group.name))
}

fn title_line(id: &str, name: &str, act: u8) -> VisibleLine {
    VisibleLine {
        event_id: format!("title-{id}"),
        group_name: Some(name.to_string()),
        text: name.to_string(),
        detail: String::new(),
        icon: String::new(),
        gem_name: None,
        act,
        kind: EventKind::Step,
        is_header: true,
    }
}

fn line_from(event: &CampaignEvent, show_tips: bool) -> VisibleLine {
    VisibleLine {
        event_id: event.id.clone(),
        group_name: None,
        text: event.text.clone(),
        detail: if show_tips { event.detail.clone() } else { String::new() },
        icon: event.icon.clone(),
        gem_name: event.gem_name.clone(),
        act: event.act,
        kind: event.kind,
        is_header: false,
    }
}

pub fn note_zone_kind(progress: &mut CharacterProgress, name: &str, area_id: &str) {
    let trial = name.eq_ignore_ascii_case("Aspirant's Trial") || area_id.contains("Labyrinth_boss");
    let town = area_id.ends_with("_town")
        || area_id.to_ascii_lowercase().contains("hideout")
        || name.to_ascii_lowercase().contains("hideout");
    if trial {
        progress.trial_gems = true;
    } else if town {
        progress.trial_gems = false;
    }
}

pub fn apply_parsed(file: &mut CharacterFile, parsed: &ParsedLog, area_act: Option<u8>) -> bool {
    match parsed {
        ParsedLog::Entered { name, area_id } => {
            if file.progress.zone_name == *name
                && file.progress.area_id == area_id.clone().unwrap_or_default()
            {
                return false;
            }
            file.progress.zone_name = name.clone();
            file.progress.entered_names.insert(name.clone());
            if let Some(id) = area_id {
                file.progress.area_id = id.clone();
                file.progress.entered_areas.insert(id.clone());
            }
            if let Some(act) = area_act {
                file.progress.highest_act = file.progress.highest_act.max(act);
            }
            file.progress.view_delta = 0;
            sync_skipped(file);
            note_zone_kind(&mut file.progress, name, area_id.as_deref().unwrap_or(""));
            true
        }
        ParsedLog::LevelUp {
            name,
            class_name,
            level,
        } => {
            if !file.name.eq_ignore_ascii_case(name) {
                return false;
            }
            file.class_name = class_name.clone();
            file.progress.class_name = class_name.clone();
            file.progress.level = file.progress.level.max(*level);
            true
        }
        ParsedLog::Slain { name } => file.name.eq_ignore_ascii_case(name),
        ParsedLog::Passive { id, .. } => {
            file.progress.passives.insert(id.clone());
            true
        }
        ParsedLog::Connected => false,
    }
}

pub fn note_current_zone(progress: &mut CharacterProgress, name: &str, area_id: Option<&str>, act: Option<u8>) {
    progress.zone_name = name.to_string();
    progress.entered_names.insert(name.to_string());
    if let Some(id) = area_id {
        progress.area_id = id.to_string();
        progress.entered_areas.insert(id.to_string());
    }
    if let Some(act) = act {
        progress.highest_act = progress.highest_act.max(act);
    }
    note_zone_kind(progress, name, area_id.unwrap_or(""));
}

pub fn force_complete(progress: &mut CharacterProgress, event_id: &str) {
    progress.completed.insert(event_id.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CampaignEvent, EventGroup, EventKind, TriggerKind};

    fn step(id: &str, area: &str, end: &str) -> CampaignEvent {
        CampaignEvent {
            id: id.into(),
            act: 1,
            text: id.into(),
            detail: "Look for the nests".into(),
            icon: "portal".into(),
            kind: EventKind::Step,
            start: vec![TriggerKind::ZoneEnter {
                area_id: area.into(),
            }],
            end: vec![TriggerKind::ZoneEnter {
                area_id: end.into(),
            }],
            gem_id: None,
            gem_name: None,
            npc: None,
            zone_name: Some(area.into()),
        }
    }

    #[test]
    fn zone_advance_hides_the_step_and_stacks_the_next() {
        let mut route = CharacterRoute::default();
        route.events.push(step("coast", "1_1_2", "1_1_3"));
        route.events.push(step("mud", "1_1_3", "1_1_4_1"));
        route.groups.push(group("g-coast", "1_1_2", &["coast"]));
        route.groups.push(group("g-mud", "1_1_3", &["mud"]));
        let mut progress = CharacterProgress::default();
        progress.entered_areas.insert("1_1_2".into());
        let lines = visible_lines(&route, &progress, true, true, true);
        assert!(lines.iter().any(|l| l.event_id == "coast" && !l.is_header));
        assert!(lines.iter().any(|l| l.event_id == "mud"));
        assert!(lines.iter().any(|l| l.is_header && l.text == "g-coast"));

        progress.entered_areas.insert("1_1_3".into());
        let lines = visible_lines(&route, &progress, true, true, true);
        assert!(!lines.iter().any(|l| l.event_id == "coast"));
        assert!(lines.iter().any(|l| l.event_id == "mud"));
    }

    #[test]
    fn manual_clear_removes_a_task_before_its_end_zone() {
        let mut route = CharacterRoute::default();
        route.events.push(step("coast", "1_1_2", "1_1_3"));
        route.groups.push(group("g-coast", "1_1_2", &["coast"]));
        let mut progress = CharacterProgress::default();
        progress.entered_areas.insert("1_1_2".into());
        force_complete(&mut progress, "coast");
        assert!(visible_lines(&route, &progress, true, true, false).is_empty());
    }

    #[test]
    fn missing_end_trigger_stays_up() {
        let mut route = CharacterRoute::default();
        let mut event = step("stay", "1_1_town", "unused");
        event.end.clear();
        route.events.push(event);
        route.groups.push(group("g-town", "1_1_town", &["stay"]));
        let mut progress = CharacterProgress::default();
        progress.entered_areas.insert("1_1_town".into());
        progress.entered_areas.insert("1_1_2".into());
        let lines = visible_lines(&route, &progress, true, true, false);
        assert!(lines.iter().any(|line| line.event_id == "stay"));
    }

    #[test]
    fn town_visit_does_not_open_later_hand_ins() {
        let mut route = CharacterRoute::default();
        route.events.push(step("gate", "1_1_town", "1_1_2"));
        route.events.push(step("coast", "1_1_2", "1_1_3"));
        route.events.push(step("dweller", "1_1_town", "1_1_7_1"));
        route.groups.push(group("g-town", "1_1_town", &["gate"]));
        route.groups.push(group("g-coast", "1_1_2", &["coast"]));
        route.groups.push(group("g-mud", "1_1_3", &["mud"]));
        route.groups.push(group("g-return", "1_1_town", &["dweller"]));
        route.events.push(step("mud", "1_1_3", "1_1_4_1"));
        let mut progress = CharacterProgress::default();
        progress.entered_areas.insert("1_1_town".into());
        progress.entered_areas.insert("1_1_2".into());
        let lines = visible_lines(&route, &progress, true, true, false);
        assert!(lines.iter().any(|line| line.event_id == "coast" && !line.is_header));
        assert!(lines.iter().any(|line| line.event_id == "mud"));
        assert!(!lines.iter().any(|line| line.event_id == "gate" || line.event_id == "dweller"));
        assert!(lines.iter().any(|line| line.is_header));
    }

    #[test]
    fn skipping_a_zone_shows_the_one_you_entered() {
        let mut route = CharacterRoute::default();
        route.events.push(step("town", "1_1_town", "1_1_2a"));
        route.events.push(step("tidal", "1_1_2a", "1_1_4_0"));
        route.events.push(step("flood", "1_1_4_0", "1_1_5"));
        route.events.push(step("ledge", "1_1_5", "1_1_6"));
        route.groups.push(group("g-town", "1_1_town", &["town"]));
        route.groups.push(group("g-tidal", "1_1_2a", &["tidal"]));
        route.groups.push(group("g-flood", "1_1_4_0", &["flood"]));
        route.groups.push(group("g-ledge", "1_1_5", &["ledge"]));
        route.groups[3].name = "The Ledge".into();
        let mut file = CharacterFile {
            name: "Test".into(),
            class_name: "Scion".into(),
            route,
            progress: CharacterProgress::default(),
            user_edited: false,
        };
        apply_parsed(&mut file, &enter("Lioneye's Watch", "1_1_town"), Some(1));
        apply_parsed(&mut file, &enter("The Tidal Island", "1_1_2a"), Some(1));
        apply_parsed(&mut file, &enter("The Ledge", "1_1_5"), Some(1));
        assert_eq!(file.progress.skipped_groups, vec!["g-flood".to_string()]);
        let lines = visible_lines(&file.route, &file.progress, true, true, false);
        assert!(lines.iter().any(|line| line.is_header && line.text == "The Ledge"));
        assert!(!lines.iter().any(|line| line.event_id == "tidal" || line.event_id == "flood"));
        apply_parsed(&mut file, &enter("Lioneye's Watch", "1_1_town"), Some(1));
        assert_eq!(file.progress.skipped_groups, vec!["g-flood".to_string()]);
        let lines = visible_lines(&file.route, &file.progress, true, true, false);
        assert!(lines.iter().any(|line| line.is_header && line.text == "The Ledge"));
        apply_parsed(&mut file, &enter("The Flooded Depths", "1_1_4_0"), Some(1));
        assert!(file.progress.skipped_groups.is_empty());
    }

    #[test]
    fn returning_to_town_does_not_skip_the_route() {
        let mut route = CharacterRoute::default();
        route.events.push(step("town", "1_1_town", "1_1_2"));
        route.events.push(step("coast", "1_1_2", "1_1_3"));
        route.events.push(step("mud", "1_1_3", "1_1_town"));
        route.events.push(step("later", "1_1_town", "1_1_4_1"));
        route.groups.push(group("g-town", "1_1_town", &["town"]));
        route.groups.push(group("g-coast", "1_1_2", &["coast"]));
        route.groups.push(group("g-mud", "1_1_3", &["mud"]));
        route.groups.push(group("g-later", "1_1_town", &["later"]));
        let mut file = CharacterFile {
            name: "Test".into(),
            class_name: "Scion".into(),
            route,
            progress: CharacterProgress::default(),
            user_edited: false,
        };
        apply_parsed(&mut file, &enter("Lioneye's Watch", "1_1_town"), Some(1));
        apply_parsed(&mut file, &enter("The Coast", "1_1_2"), Some(1));
        apply_parsed(&mut file, &enter("Lioneye's Watch", "1_1_town"), Some(1));
        assert!(file.progress.skipped_groups.is_empty());
        let lines = visible_lines(&file.route, &file.progress, true, true, false);
        assert!(lines.iter().any(|line| line.event_id == "coast"));
        assert!(!lines.iter().any(|line| line.event_id == "later"));
    }

    #[test]
    fn enter_line_shows_in_the_previous_zone_only() {
        let mut route = CharacterRoute::default();
        let mut coast = step("coast", "1_1_2", "1_1_3");
        coast.text = "Get the Waypoint".into();
        let mut enter = step("enter-mud", "1_1_3", "1_1_4_1");
        enter.text = "Enter {loc}The Mud Flats{/}".into();
        let mut glyph = step("glyph", "1_1_3", "1_1_4_1");
        glyph.text = "Find 3xGlyph".into();
        route.events.extend([coast, enter, glyph]);
        route.groups.push(group("g-coast", "1_1_2", &["coast"]));
        route.groups.push(group("g-mud", "1_1_3", &["enter-mud", "glyph"]));
        route.groups[1].name = "The Mud Flats".into();
        let mut progress = CharacterProgress::default();
        progress.entered_areas.insert("1_1_2".into());
        let lines = visible_lines(&route, &progress, true, true, false);
        assert!(lines.iter().any(|line| line.text == "Enter {loc}The Mud Flats{/}"));
        progress.entered_areas.insert("1_1_3".into());
        let lines = visible_lines(&route, &progress, true, true, false);
        assert!(lines.iter().any(|line| line.text == "Find 3xGlyph"));
        assert!(lines.iter().any(|line| line.is_header && line.text == "The Mud Flats"));
        assert!(!lines.iter().any(|line| line.event_id == "enter-mud"));
    }

    fn enter(name: &str, area: &str) -> ParsedLog {
        ParsedLog::Entered {
            name: name.into(),
            area_id: Some(area.into()),
        }
    }

    fn group(id: &str, area: &str, events: &[&str]) -> EventGroup {
        EventGroup {
            id: id.into(),
            act: 1,
            name: id.into(),
            start: vec![TriggerKind::ZoneEnter {
                area_id: area.into(),
            }],
            event_ids: events.iter().map(|event| (*event).to_string()).collect(),
        }
    }
}
