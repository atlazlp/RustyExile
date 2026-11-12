use std::collections::HashSet;

use crate::logparse::ParsedLog;
use crate::model::{
    CampaignEvent, CharacterFile, CharacterProgress, CharacterRoute, EventGroup, EventKind,
    TriggerKind, VisibleLine,
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
    for index in 0..route.groups.len() {
        if !group_passed(route, index, progress) {
            break;
        }
        current = Some(index);
    }
    current
}

fn group_passed(route: &CharacterRoute, index: usize, progress: &CharacterProgress) -> bool {
    let group = &route.groups[index];
    if listed(&progress.skipped_groups, &group.id)
        || listed(&progress.cleared_groups, &group.id)
        || listed(&progress.visited_groups, &group.id)
    {
        return true;
    }
    let Some(key) = start_key(group) else {
        return group
            .start
            .iter()
            .any(|trigger| trigger_fired(trigger, progress));
    };
    let open = route.groups.iter().enumerate().find(|(_, candidate)| {
        start_key(candidate).as_ref() == Some(&key)
            && !listed(&progress.cleared_groups, &candidate.id)
            && !listed(&progress.skipped_groups, &candidate.id)
    });
    open.is_some_and(|(open_index, _)| open_index == index)
        && group
            .start
            .iter()
            .any(|trigger| trigger_fired(trigger, progress))
}

fn listed(ids: &[String], id: &str) -> bool {
    ids.iter().any(|candidate| candidate == id)
}

fn view_base(route: &CharacterRoute, progress: &CharacterProgress) -> Option<usize> {
    anchor_index(route, progress).or_else(|| frontier_index(route, progress))
}

fn viewed_index(route: &CharacterRoute, progress: &CharacterProgress) -> Option<usize> {
    let base = view_base(route, progress)?;
    if route.groups.is_empty() {
        return None;
    }
    let max = route.groups.len() as i32 - 1;
    Some((base as i32 + progress.view_delta).clamp(0, max) as usize)
}

pub fn nudge_events(file: &mut CharacterFile, delta: i32) {
    let Some(base) = view_base(&file.route, &file.progress) else {
        return;
    };
    if file.route.groups.is_empty() {
        return;
    }
    let max = file.route.groups.len() as i32 - 1;
    let next = (base as i32 + file.progress.view_delta + delta).clamp(0, max);
    file.progress.view_delta = next - base as i32;
}

pub fn nudge_groups(file: &mut CharacterFile, delta: i32) {
    let Some(current) = viewed_index(&file.route, &file.progress) else {
        return;
    };
    if file.route.groups.is_empty() {
        return;
    }
    let max = file.route.groups.len() as i32 - 1;
    let target = (current as i32 + delta).clamp(0, max) as usize;
    let base = view_base(&file.route, &file.progress).unwrap_or(0);
    if target > current {
        for group in &file.route.groups[current..target] {
            if !listed(&file.progress.cleared_groups, &group.id) {
                file.progress.cleared_groups.push(group.id.clone());
            }
        }
    } else if target < current {
        for group in &file.route.groups[target..current] {
            file.progress.cleared_groups.retain(|id| id != &group.id);
        }
    }
    file.progress.view_delta = target as i32 - base as i32;
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
        if group
            .start
            .iter()
            .any(|trigger| trigger_fired(trigger, progress))
        {
            furthest = Some(index);
        }
    }
    furthest
}

pub fn sync_skipped(file: &mut CharacterFile) {
    let route = &file.route;
    let area_now = file.progress.area_id.clone();
    let still_skipped: Vec<String> = file
        .progress
        .skipped_groups
        .iter()
        .filter(|id| {
            let Some((index, group)) = route
                .groups
                .iter()
                .enumerate()
                .find(|(_, group)| &&group.id == id)
            else {
                return false;
            };
            let Some(area) = zone_area(group) else {
                return !group
                    .start
                    .iter()
                    .any(|trigger| trigger_fired(trigger, &file.progress));
            };
            if area_now != area {
                return true;
            }
            let first_skipped = route.groups.iter().enumerate().find(|(_, candidate)| {
                zone_area(candidate) == Some(area)
                    && file
                        .progress
                        .skipped_groups
                        .iter()
                        .any(|skipped| skipped == &candidate.id)
            });
            match first_skipped {
                Some((skipped_index, _)) => skipped_index != index,
                None => true,
            }
        })
        .cloned()
        .collect();
    file.progress.skipped_groups = still_skipped;
    let Some(target) = furthest_reached(route, &file.progress) else {
        return;
    };
    let mut gap = None;
    for index in 0..target {
        if !group_passed(route, index, &file.progress) {
            gap = Some(index);
            break;
        }
    }
    let Some(gap) = gap else {
        return;
    };
    for index in gap..target {
        if group_passed(route, index, &file.progress) {
            continue;
        }
        file.progress
            .skipped_groups
            .push(route.groups[index].id.clone());
    }
}

fn zone_area(group: &crate::model::EventGroup) -> Option<&str> {
    group.start.iter().find_map(|trigger| match trigger {
        TriggerKind::ZoneEnter { area_id } => Some(area_id.as_str()),
        _ => None,
    })
}

fn anchor_index(route: &CharacterRoute, progress: &CharacterProgress) -> Option<usize> {
    let id = progress.anchor_group.as_deref()?;
    route.groups.iter().position(|group| group.id == id)
}

fn resolve_area_id(
    route: &CharacterRoute,
    progress: &CharacterProgress,
    given: Option<&str>,
    name: &str,
) -> Option<String> {
    if let Some(id) = given {
        if route
            .groups
            .iter()
            .any(|group| zone_area(group) == Some(id) && group.name.eq_ignore_ascii_case(name))
        {
            return Some(id.to_string());
        }
        let trimmed = id.trim_end_matches('_');
        if let Some(group) = route.groups.iter().find(|group| {
            zone_area(group).is_some_and(|area| area.trim_end_matches('_') == trimmed)
                && group.name.eq_ignore_ascii_case(name)
        }) {
            return zone_area(group).map(|area| area.to_string());
        }
        let known = route.groups.iter().any(|group| {
            zone_area(group).is_some_and(|area| area == id || area.trim_end_matches('_') == trimmed)
        });
        if !known {
            return Some(id.to_string());
        }
    }
    let anchor = anchor_index(route, progress)
        .or_else(|| frontier_index(route, progress))
        .unwrap_or(0);
    let matches: Vec<usize> = route
        .groups
        .iter()
        .enumerate()
        .filter(|(_, group)| group.name.eq_ignore_ascii_case(name) && zone_area(group).is_some())
        .map(|(index, _)| index)
        .collect();
    let index = matches
        .iter()
        .copied()
        .find(|index| *index >= anchor)
        .or_else(|| matches.last().copied())?;
    zone_area(&route.groups[index]).map(|area| area.to_string())
}

fn claim_zone(file: &mut CharacterFile, area_id: &str) {
    let slots: Vec<usize> = file
        .route
        .groups
        .iter()
        .enumerate()
        .filter(|(_, group)| zone_area(group) == Some(area_id))
        .map(|(index, _)| index)
        .collect();
    if slots.is_empty() {
        return;
    }
    let anchor = anchor_index(&file.route, &file.progress);
    let shortcut_at = |index: usize, file: &CharacterFile| {
        listed(&file.progress.cleared_groups, &file.route.groups[index].id)
    };
    let skipped_at = |index: usize, file: &CharacterFile| {
        listed(&file.progress.skipped_groups, &file.route.groups[index].id)
    };
    if let Some(hit) = slots
        .iter()
        .copied()
        .find(|index| skipped_at(*index, file) && !shortcut_at(*index, file))
    {
        let id = file.route.groups[hit].id.clone();
        file.progress
            .skipped_groups
            .retain(|skipped_id| skipped_id != &id);
        set_anchor(file, hit);
        return;
    }
    let progress_consumed = |index: usize| anchor.is_some_and(|current| index < current);
    let candidate = slots
        .iter()
        .copied()
        .find(|index| !shortcut_at(*index, file) && !progress_consumed(*index));
    let Some(candidate) = candidate else {
        return;
    };
    let Some(anchor) = anchor else {
        set_anchor(file, candidate);
        return;
    };
    if candidate <= anchor {
        return;
    }
    let between_clear =
        (anchor + 1..candidate).all(|index| group_passed(&file.route, index, &file.progress));
    let earlier: Vec<usize> = slots
        .iter()
        .copied()
        .filter(|index| *index < candidate)
        .collect();
    let earlier_shortcut =
        !earlier.is_empty() && earlier.iter().all(|index| shortcut_at(*index, file));
    if between_clear || earlier_shortcut || slots.first().copied() == Some(candidate) {
        set_anchor(file, candidate);
    }
}

fn set_anchor(file: &mut CharacterFile, index: usize) {
    let id = file.route.groups[index].id.clone();
    if file.progress.anchor_group.as_deref() == Some(id.as_str()) {
        return;
    }
    if let Some(previous) = file.progress.anchor_group.clone() {
        if !listed(&file.progress.visited_groups, &previous) {
            file.progress.visited_groups.push(previous);
        }
    }
    file.progress.anchor_group = Some(id);
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
            .filter(|event| {
                event.id.starts_with("gem-lilly-") && !progress.completed.contains(&event.id)
            })
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
            .filter(|event| {
                event.id.starts_with("gem-trial-") && !progress.completed.contains(&event.id)
            })
            .filter(|_| progress.trial_gems)
            .cloned()
            .collect();
        if !trials.is_empty() {
            lines.push(title_line(
                "trial",
                "Aspirant's Trial",
                progress.highest_act,
            ));
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
    if event.kind != EventKind::Step {
        return false;
    }
    let plain = crate::plain_text(&event.text);
    plain.eq_ignore_ascii_case(&format!("Enter {}", group.name))
        || plain.eq_ignore_ascii_case(&format!("Waypoint to {}", group.name))
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
        cost_icon: String::new(),
    }
}

fn line_from(event: &CampaignEvent, show_tips: bool) -> VisibleLine {
    VisibleLine {
        event_id: event.id.clone(),
        group_name: None,
        text: event.text.clone(),
        detail: if show_tips {
            event.detail.clone()
        } else {
            String::new()
        },
        icon: event.icon.clone(),
        gem_name: event.gem_name.clone(),
        act: event.act,
        kind: event.kind,
        is_header: false,
        cost_icon: event.kind.shown_cost(&event.cost_icon).to_string(),
    }
}

pub(crate) fn place_is_town(area_id: &str, name: &str) -> bool {
    area_id.ends_with("_town")
        || area_id.to_ascii_lowercase().contains("hideout")
        || name.to_ascii_lowercase().contains("hideout")
}

pub fn note_zone_kind(progress: &mut CharacterProgress, name: &str, area_id: &str) {
    let trial = name.eq_ignore_ascii_case("Aspirant's Trial") || area_id.contains("Labyrinth_boss");
    let town = place_is_town(area_id, name);
    if trial {
        progress.trial_gems = true;
    } else if town {
        progress.trial_gems = false;
    }
}

fn arrive(file: &mut CharacterFile, name: &str, given: Option<&str>, act: Option<u8>) -> bool {
    let resolved = resolve_area_id(&file.route, &file.progress, given, name);
    let area_key = resolved
        .clone()
        .unwrap_or_else(|| given.unwrap_or("").to_string());
    if file.progress.zone_name == name && file.progress.area_id == area_key {
        return false;
    }
    file.progress.zone_name = name.to_string();
    file.progress.entered_names.insert(name.to_string());
    if !area_key.is_empty() {
        file.progress.area_id = area_key.clone();
        file.progress.entered_areas.insert(area_key.clone());
    }
    if let Some(act) = act {
        file.progress.highest_act = file.progress.highest_act.max(act);
    }
    file.progress.view_delta = 0;
    if !area_key.is_empty() {
        claim_zone(file, &area_key);
    }
    sync_skipped(file);
    let area_now = file.progress.area_id.clone();
    note_zone_kind(&mut file.progress, name, &area_now);
    true
}

pub fn apply_parsed(file: &mut CharacterFile, parsed: &ParsedLog, area_act: Option<u8>) -> bool {
    match parsed {
        ParsedLog::Entered { name, area_id } => arrive(file, name, area_id.as_deref(), area_act),
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

pub fn note_current_zone(
    file: &mut CharacterFile,
    name: &str,
    area_id: Option<&str>,
    act: Option<u8>,
) {
    let _ = arrive(file, name, area_id, act);
}

pub fn restart_at_beach(file: &mut CharacterFile) {
    let class_name = if !file.class_name.is_empty() {
        file.class_name.clone()
    } else {
        file.progress.class_name.clone()
    };
    file.class_name = class_name.clone();
    ensure_act1_beach(file);
    let anchor = file
        .route
        .groups
        .iter()
        .find(|group| zone_area(group) == Some("1_1_1"))
        .map(|group| group.id.clone());
    file.progress = CharacterProgress {
        class_name,
        level: 1,
        zone_name: "The Twilight Strand".into(),
        area_id: "1_1_1".into(),
        highest_act: 1,
        anchor_group: anchor,
        entered_areas: ["1_1_1".to_string()].into_iter().collect(),
        entered_names: ["The Twilight Strand".to_string()].into_iter().collect(),
        ..CharacterProgress::default()
    };
    note_zone_kind(
        &mut file.progress,
        "The Twilight Strand",
        "1_1_1",
    );
}

fn ensure_act1_beach(file: &mut CharacterFile) {
    if let Some(index) = file
        .route
        .groups
        .iter()
        .position(|group| zone_area(group) == Some("1_1_1"))
    {
        if index != 0 {
            let group = file.route.groups.remove(index);
            file.route.groups.insert(0, group);
        }
        return;
    }
    let event_id = beach_event(file);
    for group in &mut file.route.groups {
        group.event_ids.retain(|id| id != &event_id);
    }
    file.route
        .groups
        .retain(|group| !group.event_ids.is_empty());
    let mut id = "g1-beach".to_string();
    let mut n = 2u32;
    while file.route.groups.iter().any(|group| group.id == id) {
        id = format!("g1-beach-{n}");
        n += 1;
    }
    file.route.groups.insert(
        0,
        EventGroup {
            id,
            act: 1,
            name: "The Twilight Strand".into(),
            start: vec![TriggerKind::ZoneEnter {
                area_id: "1_1_1".into(),
            }],
            event_ids: vec![event_id],
        },
    );
}

fn beach_event(file: &mut CharacterFile) -> String {
    if let Some(event) = file.route.events.iter_mut().find(|event| {
        event.act == 1
            && crate::plain_text(&event.text)
                .to_ascii_lowercase()
                .contains("hillock")
    }) {
        event.zone_name = Some("The Twilight Strand".into());
        event.start = vec![TriggerKind::ZoneEnter {
            area_id: "1_1_1".into(),
        }];
        if event.end.is_empty() {
            event.end = vec![TriggerKind::ZoneEnter {
                area_id: "1_1_town".into(),
            }];
        }
        return event.id.clone();
    }
    let id = "a1-beach".to_string();
    file.route.events.insert(
        0,
        CampaignEvent {
            id: id.clone(),
            act: 1,
            text: "Find and kill Hillock".into(),
            detail: String::new(),
            icon: "quest".into(),
            kind: EventKind::Step,
            start: vec![TriggerKind::ZoneEnter {
                area_id: "1_1_1".into(),
            }],
            end: vec![TriggerKind::ZoneEnter {
                area_id: "1_1_town".into(),
            }],
            gem_id: None,
            gem_name: None,
            npc: None,
            zone_name: Some("The Twilight Strand".into()),
            cost_icon: String::new(),
        },
    );
    id
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
            cost_icon: String::new(),
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
        route
            .groups
            .push(group("g-return", "1_1_town", &["dweller"]));
        route.events.push(step("mud", "1_1_3", "1_1_4_1"));
        let mut progress = CharacterProgress::default();
        progress.entered_areas.insert("1_1_town".into());
        progress.entered_areas.insert("1_1_2".into());
        let lines = visible_lines(&route, &progress, true, true, false);
        assert!(lines
            .iter()
            .any(|line| line.event_id == "coast" && !line.is_header));
        assert!(lines.iter().any(|line| line.event_id == "mud"));
        assert!(!lines
            .iter()
            .any(|line| line.event_id == "gate" || line.event_id == "dweller"));
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
        assert!(lines
            .iter()
            .any(|line| line.is_header && line.text == "The Ledge"));
        assert!(!lines
            .iter()
            .any(|line| line.event_id == "tidal" || line.event_id == "flood"));
        apply_parsed(&mut file, &enter("Lioneye's Watch", "1_1_town"), Some(1));
        assert_eq!(file.progress.skipped_groups, vec!["g-flood".to_string()]);
        let lines = visible_lines(&file.route, &file.progress, true, true, false);
        assert!(lines
            .iter()
            .any(|line| line.is_header && line.text == "The Ledge"));
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
        route
            .groups
            .push(group("g-mud", "1_1_3", &["enter-mud", "glyph"]));
        route.groups[1].name = "The Mud Flats".into();
        let mut progress = CharacterProgress::default();
        progress.entered_areas.insert("1_1_2".into());
        let lines = visible_lines(&route, &progress, true, true, false);
        assert!(lines
            .iter()
            .any(|line| line.text == "Enter {loc}The Mud Flats{/}"));
        progress.entered_areas.insert("1_1_3".into());
        let lines = visible_lines(&route, &progress, true, true, false);
        assert!(lines.iter().any(|line| line.text == "Find 3xGlyph"));
        assert!(lines
            .iter()
            .any(|line| line.is_header && line.text == "The Mud Flats"));
        assert!(!lines.iter().any(|line| line.event_id == "enter-mud"));
    }

    #[test]
    fn restart_at_beach_clears_progress_and_shows_the_first_mission() {
        let mut route = CharacterRoute::default();
        let mut hillock = step("hillock", "1_1_1", "1_1_town");
        hillock.text = "Find and kill Hillock".into();
        let mut town = step("town", "1_1_town", "1_1_2");
        town.text = "Enter Lioneye's Watch".into();
        route.events.extend([hillock, town]);
        route.groups.push(group("g-strand", "1_1_1", &["hillock"]));
        route.groups.push(group("g-town", "1_1_town", &["town"]));
        route.groups[0].name = "The Twilight Strand".into();
        route.groups[1].name = "Lioneye's Watch".into();
        let mut file = character(route);
        file.class_name = "Witch".into();
        file.progress.class_name = "Witch".into();
        file.progress.level = 40;
        file.progress.area_id = "1_3_1".into();
        file.progress.zone_name = "The City of Sarn".into();
        file.progress.entered_areas.insert("1_3_1".into());
        file.progress.completed.insert("old".into());
        file.progress.highest_act = 3;
        file.progress.skipped_groups.push("g-town".into());
        restart_at_beach(&mut file);
        assert_eq!(file.class_name, "Witch");
        assert_eq!(file.progress.class_name, "Witch");
        assert_eq!(file.progress.level, 1);
        assert_eq!(file.progress.area_id, "1_1_1");
        assert_eq!(file.progress.zone_name, "The Twilight Strand");
        assert_eq!(file.progress.highest_act, 1);
        assert!(file.progress.completed.is_empty());
        assert!(file.progress.skipped_groups.is_empty());
        assert_eq!(
            file.progress.entered_areas.iter().collect::<Vec<_>>(),
            vec!["1_1_1"]
        );
        let lines = visible_lines(&file.route, &file.progress, true, true, false);
        assert!(lines
            .iter()
            .any(|line| line.is_header && line.text == "The Twilight Strand"));
        assert!(lines.iter().any(|line| line.event_id == "hillock"));
    }

    #[test]
    fn restart_from_the_act_six_strand_opens_act_one() {
        let mut route = CharacterRoute::default();
        let mut hillock = step("hillock", "1_1_town", "1_1_2");
        hillock.text = "Find and kill Hillock".into();
        hillock.act = 1;
        hillock.start = vec![TriggerKind::ZoneName {
            name: "Find and kill Hillock".into(),
        }];
        hillock.zone_name = Some("Find and kill Hillock".into());
        let mut town = step("town", "1_1_town", "1_1_2");
        town.text = "Enter Lioneye's Watch".into();
        let mut fallen = step("fallen", "2_6_1", "2_6_2");
        fallen.act = 6;
        fallen.text = "Hand in Fallen from Grace".into();
        route.events.extend([hillock, town, fallen]);
        route.groups.push(group("g-town", "1_1_town", &["town"]));
        let mut strand = group("g6-strand", "2_6_1", &["fallen"]);
        strand.act = 6;
        strand.name = "The Twilight Strand".into();
        route.groups.push(strand);
        let mut file = character(route);
        file.user_edited = true;
        file.progress.area_id = "2_6_1".into();
        file.progress.zone_name = "The Twilight Strand".into();
        file.progress.highest_act = 6;
        file.progress.level = 48;
        file.progress.entered_areas.insert("2_6_1".into());
        file.progress
            .entered_names
            .insert("The Twilight Strand".into());
        file.progress.anchor_group = Some("g6-strand".into());
        file.progress.skipped_groups.push("g-town".into());
        file.progress.completed.insert("old".into());
        restart_at_beach(&mut file);
        assert_eq!(file.progress.area_id, "1_1_1");
        assert_eq!(file.progress.zone_name, "The Twilight Strand");
        assert_eq!(file.progress.highest_act, 1);
        assert_eq!(file.progress.level, 1);
        assert!(file.progress.completed.is_empty());
        assert!(file.progress.skipped_groups.is_empty());
        assert!(!file.progress.entered_areas.contains("2_6_1"));
        let anchor = file.progress.anchor_group.as_deref().unwrap();
        let group = file
            .route
            .groups
            .iter()
            .find(|group| group.id == anchor)
            .unwrap();
        assert_eq!(group.act, 1);
        assert_eq!(group.name, "The Twilight Strand");
        assert_eq!(file.route.groups[0].id, anchor);
        let lines = visible_lines(&file.route, &file.progress, true, true, false);
        assert!(lines.iter().any(|line| line.event_id == "hillock"));
        assert!(!lines.iter().any(|line| line.event_id == "fallen"));
        assert!(!lines.iter().any(|line| line.text.contains("Fallen from Grace")));
    }

    fn enter(name: &str, area: &str) -> ParsedLog {
        ParsedLog::Entered {
            name: name.into(),
            area_id: Some(area.into()),
        }
    }

    #[test]
    fn repeated_zone_follows_visit_order() {
        let mut route = CharacterRoute::default();
        route.events.push(step("chamber-go", "1_2_3", "1_2_6_1"));
        route.events.push(step("fell-go", "1_2_3", "1_2_15"));
        route
            .groups
            .push(group("g-cross-1", "1_2_3", &["chamber-go"]));
        route
            .groups
            .push(group("g-chamber", "1_2_6_1", &["chamber"]));
        route.groups.push(group("g-cross-2", "1_2_3", &["fell-go"]));
        route.groups[0].name = "The Crossroads".into();
        route.groups[2].name = "The Crossroads".into();
        let mut file = character(route);
        apply_parsed(&mut file, &enter("The Crossroads", "1_2_3"), Some(2));
        let lines = visible_lines(&file.route, &file.progress, true, false, false);
        assert!(lines.iter().any(|line| line.event_id == "chamber-go"));
        assert!(!lines.iter().any(|line| line.event_id == "fell-go"));
        apply_parsed(
            &mut file,
            &enter("The Chamber of Sins Level 1", "1_2_6_1"),
            Some(2),
        );
        apply_parsed(&mut file, &enter("The Crossroads", "1_2_3"), Some(2));
        let lines = visible_lines(&file.route, &file.progress, true, false, false);
        assert!(lines.iter().any(|line| line.event_id == "fell-go"));
        assert!(!lines.iter().any(|line| line.event_id == "chamber-go"));
    }

    #[test]
    fn group_shortcut_clears_without_skipping_and_zone_entry_cancels_the_scroll() {
        let mut route = CharacterRoute::default();
        route.events.push(step("chamber-go", "1_2_3", "1_2_6_1"));
        route.events.push(step("fell-go", "1_2_3", "1_2_15"));
        route
            .groups
            .push(group("g-cross-1", "1_2_3", &["chamber-go"]));
        route
            .groups
            .push(group("g-chamber", "1_2_6_1", &["chamber"]));
        route.groups.push(group("g-cross-2", "1_2_3", &["fell-go"]));
        let mut file = character(route);
        apply_parsed(&mut file, &enter("The Crossroads", "1_2_3"), Some(2));
        nudge_groups(&mut file, 1);
        assert_eq!(file.progress.cleared_groups, vec!["g-cross-1".to_string()]);
        assert!(file.progress.skipped_groups.is_empty());
        assert_ne!(file.progress.view_delta, 0);
        nudge_groups(&mut file, -1);
        assert!(file.progress.cleared_groups.is_empty());
        nudge_groups(&mut file, 1);
        apply_parsed(
            &mut file,
            &enter("The Forest Encampment", "1_2_town"),
            Some(2),
        );
        assert_eq!(file.progress.view_delta, 0);
        apply_parsed(&mut file, &enter("The Crossroads", "1_2_3"), Some(2));
        let lines = visible_lines(&file.route, &file.progress, true, false, false);
        assert!(lines.iter().any(|line| line.event_id == "fell-go"));
        assert!(file.progress.skipped_groups.is_empty());
    }

    #[test]
    fn entered_name_resolves_the_area_when_the_log_id_is_missing() {
        let mut route = CharacterRoute::default();
        route.events.push(step("river", "1_2_7", "1_2_9"));
        route.events.push(step("forest", "1_2_9", "1_2_10"));
        route.groups.push(group("g-river", "1_2_7", &["river"]));
        route.groups.push(group("g-forest", "1_2_9", &["forest"]));
        route.groups[0].name = "The Riverways".into();
        route.groups[1].name = "The Western Forest".into();
        let mut file = character(route);
        apply_parsed(&mut file, &enter("The Riverways", "1_2_7"), Some(2));
        apply_parsed(
            &mut file,
            &ParsedLog::Entered {
                name: "The Western Forest".into(),
                area_id: None,
            },
            Some(2),
        );
        assert_eq!(file.progress.area_id, "1_2_9");
        let lines = visible_lines(&file.route, &file.progress, true, false, false);
        assert!(lines
            .iter()
            .any(|line| line.is_header && line.text == "The Western Forest"));
        apply_parsed(&mut file, &enter("The Riverways", "1_2_7"), Some(2));
        apply_parsed(
            &mut file,
            &ParsedLog::Entered {
                name: "The Western Forest".into(),
                area_id: Some("1_2_7".into()),
            },
            Some(2),
        );
        assert_eq!(file.progress.area_id, "1_2_9");
    }

    #[test]
    fn an_unknown_area_id_is_not_replaced_by_a_later_zone_of_the_same_name() {
        let mut route = CharacterRoute::default();
        route.events.push(step("fallen", "2_6_1", "2_6_2"));
        let mut strand = group("g6", "2_6_1", &["fallen"]);
        strand.name = "The Twilight Strand".into();
        route.groups.push(strand);
        let mut file = character(route);
        note_current_zone(&mut file, "The Twilight Strand", Some("1_1_1"), Some(1));
        assert_eq!(file.progress.area_id, "1_1_1");
        assert!(file.progress.anchor_group.is_none());
    }

    fn character(route: CharacterRoute) -> CharacterFile {
        CharacterFile {
            name: "Test".into(),
            class_name: "Scion".into(),
            route,
            progress: CharacterProgress::default(),
            user_edited: false,
        }
    }

    #[test]
    fn a_hideout_counts_as_town() {
        assert!(place_is_town("1_1_town", "Lioneye's Watch"));
        assert!(place_is_town("HideoutShore", "Coastal Hideout"));
        assert!(place_is_town("2_6_town", "Hideout"));
        assert!(!place_is_town("1_1_2", "The Coast"));
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
