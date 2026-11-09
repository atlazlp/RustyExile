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

fn started(event: &CampaignEvent, route: &CharacterRoute, progress: &CharacterProgress) -> bool {
    let own = event.start.iter().any(|t| trigger_fired(t, progress));
    let grouped = route.groups.iter().any(|group| {
        group.event_ids.iter().any(|id| id == &event.id)
            && group.start.iter().any(|t| trigger_fired(t, progress))
    });
    own || grouped
}

fn ended(event: &CampaignEvent, progress: &CharacterProgress) -> bool {
    progress.completed.contains(&event.id) || event.end.iter().any(|t| trigger_fired(t, progress))
}

pub fn event_visible(
    event: &CampaignEvent,
    route: &CharacterRoute,
    progress: &CharacterProgress,
) -> bool {
    started(event, route, progress) && !ended(event, progress)
}

pub fn visible_lines(
    route: &CharacterRoute,
    progress: &CharacterProgress,
    show_acts: bool,
    show_gems: bool,
    show_tips: bool,
) -> Vec<VisibleLine> {
    let mut lines = Vec::new();
    let mut header_done = std::collections::BTreeSet::new();
    for event in &route.events {
        if event.kind.is_gem() && !show_gems {
            continue;
        }
        if !event.kind.is_gem() && !show_acts {
            continue;
        }
        if !event_visible(event, route, progress) {
            continue;
        }
        if let Some(group) = route
            .groups
            .iter()
            .find(|g| g.event_ids.iter().any(|id| id == &event.id))
        {
            if header_done.insert(group.id.clone()) {
                let any = group.event_ids.iter().any(|id| {
                    route
                        .events
                        .iter()
                        .find(|e| &e.id == id)
                        .is_some_and(|e| {
                            (e.kind.is_gem() && show_gems || !e.kind.is_gem() && show_acts)
                                && event_visible(e, route, progress)
                        })
                });
                if any {
                    lines.push(VisibleLine {
                        event_id: group.id.clone(),
                        group_name: Some(group.name.clone()),
                        text: group.name.clone(),
                        detail: String::new(),
                        icon: "waypoint".into(),
                        gem_name: None,
                        act: group.act,
                        kind: EventKind::Step,
                        is_header: true,
                    });
                }
            }
        }
        lines.push(VisibleLine {
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
        });
    }
    lines
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
        route.groups.push(EventGroup {
            id: "g-coast".into(),
            act: 1,
            name: "The Coast".into(),
            start: vec![TriggerKind::ZoneEnter {
                area_id: "1_1_2".into(),
            }],
            event_ids: vec!["coast".into()],
        });
        let mut progress = CharacterProgress::default();
        progress.entered_areas.insert("1_1_2".into());
        let lines = visible_lines(&route, &progress, true, true, true);
        assert!(lines.iter().any(|l| l.event_id == "coast"));
        assert!(!lines.iter().any(|l| l.event_id == "mud"));

        progress.entered_areas.insert("1_1_3".into());
        let lines = visible_lines(&route, &progress, true, true, true);
        assert!(!lines.iter().any(|l| l.event_id == "coast"));
        assert!(lines.iter().any(|l| l.event_id == "mud"));
    }

    #[test]
    fn manual_clear_removes_a_task_before_its_end_zone() {
        let mut route = CharacterRoute::default();
        route.events.push(step("coast", "1_1_2", "1_1_3"));
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
        let mut progress = CharacterProgress::default();
        progress.entered_areas.insert("1_1_town".into());
        progress.entered_areas.insert("1_1_2".into());
        assert_eq!(visible_lines(&route, &progress, true, true, false).len(), 1);
    }
}
