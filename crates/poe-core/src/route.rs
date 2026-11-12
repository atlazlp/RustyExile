use crate::model::{
    AreaInfo, BanditChoice, CampaignEvent, Catalog, CharacterRoute, EventGroup, EventKind,
    GemSource, QuestGem, TriggerKind,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
struct RawStep {
    text: String,
    detail: Vec<String>,
    icon: String,
    area_id: Option<String>,
    quest_ids: Vec<String>,
    act: u8,
    logout: bool,
}

pub fn build_route(
    catalog: &Catalog,
    class_name: &str,
    gem_filter: Option<&HashSet<String>>,
    league_start: bool,
) -> CharacterRoute {
    build_route_with(
        catalog,
        class_name,
        gem_filter,
        league_start,
        BanditChoice::KillAll,
    )
}

pub fn build_route_with(
    catalog: &Catalog,
    class_name: &str,
    gem_filter: Option<&HashSet<String>>,
    league_start: bool,
    bandit: BanditChoice,
) -> CharacterRoute {
    let mut steps = Vec::new();
    for (act, text) in &catalog.routes {
        steps.extend(parse_act(catalog, *act, text, league_start, bandit));
    }
    assign_leading_area(&mut steps, catalog);
    let mut carry: Option<String> = None;
    let mut back_in_town = false;
    for step in &mut steps {
        if step.area_id.is_none() {
            step.area_id = if back_in_town {
                act_town(catalog, step.act).or_else(|| carry.clone())
            } else {
                carry.clone()
            };
        } else {
            carry = step.area_id.clone();
            back_in_town = false;
        }
        if step.logout {
            back_in_town = true;
        }
    }

    let mut route = CharacterRoute {
        bandit: bandit.as_str().to_string(),
        ..CharacterRoute::default()
    };
    let mut index = 0usize;
    while index < steps.len() {
        let area = steps[index].area_id.clone();
        let mut end = index + 1;
        while end < steps.len() && steps[end].area_id == area && !steps[end - 1].logout {
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
                cost_icon: String::new(),
            });
            if let Some((quest_id, offers)) = step.quest_ids.split_first() {
                push_quest_gems(
                    catalog,
                    &mut route,
                    &mut ids,
                    quest_id,
                    offers,
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
    if let Some(filter) = gem_filter {
        add_lilly_and_trials(&mut route, catalog, class_name, filter);
    }
    route
}

fn push_quest_gems(
    catalog: &Catalog,
    route: &mut CharacterRoute,
    ids: &mut Vec<String>,
    quest_id: &str,
    offer_filter: &[String],
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
    if class_name.is_empty() || gem_filter.is_none() {
        return;
    }
    let gems: Vec<&QuestGem> = quest
        .gems
        .iter()
        .filter(|gem| {
            gem_wanted(
                catalog,
                gem,
                quest_id,
                offer_filter,
                class_name,
                gem_filter,
            )
        })
        .collect();
    let mut vendor_of: HashMap<&str, &QuestGem> = HashMap::new();
    for gem in &gems {
        if gem.source == GemSource::Vendor {
            vendor_of.entry(gem.gem_id.as_str()).or_insert(gem);
        }
    }
    let mut offer_order = Vec::new();
    let mut rewards: HashMap<&str, Vec<&QuestGem>> = HashMap::new();
    for gem in &gems {
        if gem.source != GemSource::Reward {
            continue;
        }
        let list = rewards.entry(gem.offer.as_str()).or_default();
        if list.iter().any(|have| have.gem_id == gem.gem_id) {
            continue;
        }
        if list.is_empty() {
            offer_order.push(gem.offer.as_str());
        }
        list.push(*gem);
    }
    let mut emitted: HashSet<String> = HashSet::new();
    for offer in offer_order {
        let list = &rewards[offer];
        let chosen = gem_filter.map(|_| reward_index(list, &vendor_of, catalog));
        for (index, gem) in list.iter().enumerate() {
            if emitted.contains(gem.gem_id.as_str()) {
                continue;
            }
            if chosen.is_none_or(|pick| index == pick) {
                push_gem_event(
                    catalog, route, ids, quest_id, gem, quest.npc.as_str(), GemSource::Reward,
                    act, zone_name, start, end,
                );
                emitted.insert(gem.gem_id.clone());
            } else if let Some(vendor) = vendor_of.get(gem.gem_id.as_str()) {
                push_gem_event(
                    catalog, route, ids, quest_id, vendor, quest.npc.as_str(), GemSource::Vendor,
                    act, zone_name, start, end,
                );
                emitted.insert(gem.gem_id.clone());
            }
        }
    }
    for gem in &gems {
        if gem.source != GemSource::Vendor || emitted.contains(gem.gem_id.as_str()) {
            continue;
        }
        push_gem_event(
            catalog, route, ids, quest_id, gem, quest.npc.as_str(), GemSource::Vendor, act,
            zone_name, start, end,
        );
        emitted.insert(gem.gem_id.clone());
    }
}

fn gem_wanted(
    catalog: &Catalog,
    gem: &QuestGem,
    quest_id: &str,
    offer_filter: &[String],
    class_name: &str,
    gem_filter: Option<&HashSet<String>>,
) -> bool {
    if quest_id == "a6q4" && gem.source == GemSource::Vendor {
        return false;
    }
    if !offer_filter.is_empty() && !offer_filter.iter().any(|id| id == &gem.offer) {
        return false;
    }
    if !gem.classes.is_empty()
        && !gem
            .classes
            .iter()
            .any(|class| class.eq_ignore_ascii_case(class_name))
    {
        return false;
    }
    let Some(info) = catalog.gem(&gem.gem_id) else {
        return false;
    };
    gem_filter.is_none_or(|filter| filter.iter().any(|name| names_match(name, &info.name)))
}

fn reward_index(
    rewards: &[&QuestGem],
    vendor_of: &HashMap<&str, &QuestGem>,
    catalog: &Catalog,
) -> usize {
    rewards
        .iter()
        .enumerate()
        .max_by_key(|(_, gem)| {
            let exclusive = !vendor_of.contains_key(gem.gem_id.as_str());
            let level = catalog
                .gem(&gem.gem_id)
                .map(|info| info.required_level)
                .unwrap_or(0);
            (exclusive, level, std::cmp::Reverse(gem.gem_id.as_str()))
        })
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn push_gem_event(
    catalog: &Catalog,
    route: &mut CharacterRoute,
    ids: &mut Vec<String>,
    quest_id: &str,
    gem: &QuestGem,
    fallback_npc: &str,
    source: GemSource,
    act: u8,
    zone_name: &str,
    start: &[TriggerKind],
    end: &[TriggerKind],
) {
    let Some(info) = catalog.gem(&gem.gem_id) else {
        return;
    };
    let kind = match source {
        GemSource::Reward => EventKind::GemPick,
        GemSource::Vendor => EventKind::GemBuy,
    };
    let verb = match source {
        GemSource::Reward => "Pick",
        GemSource::Vendor => "Buy",
    };
    let npc = if gem.npc.is_empty() {
        fallback_npc.to_string()
    } else {
        gem.npc.clone()
    };
    let id = format!("gem-{}-{}", quest_id, info.id.replace('/', "_"));
    if route.events.iter().any(|event| event.id == id) {
        if !ids.iter().any(|existing| existing == &id) {
            ids.push(id);
        }
        return;
    }
    ids.push(id.clone());
    let place = gem_location(catalog, &npc, act, zone_name);
    route.events.push(CampaignEvent {
        id,
        act,
        text: format!(
            "{verb} {} from {npc} in {}",
            gem_color(&info.primary_attribute, &info.name),
            loc(&place)
        ),
        detail: String::new(),
        icon: format!("gem:{}", info.name),
        kind,
        start: start.to_vec(),
        end: end.to_vec(),
        gem_id: Some(info.id.clone()),
        gem_name: Some(info.name.clone()),
        npc: Some(npc),
        zone_name: Some(zone_name.to_string()),
        cost_icon: buy_cost(kind, info.required_level),
    });
}

fn act_town(catalog: &Catalog, act: u8) -> Option<String> {
    catalog
        .areas
        .iter()
        .find(|area| area.act == act && area.is_town)
        .map(|area| area.id.clone())
}

fn names_match(pob: &str, catalog_name: &str) -> bool {
    if pob.eq_ignore_ascii_case(catalog_name) {
        return true;
    }
    format!("{} Support", strip_support(pob)).eq_ignore_ascii_case(catalog_name)
}

fn strip_support(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    if let Some(stripped) = lower.strip_suffix(" support") {
        &name[..stripped.len()]
    } else {
        name
    }
}

fn is_transfigured(catalog: &Catalog, name: &str) -> bool {
    if catalog
        .gems
        .iter()
        .any(|gem| gem.name.eq_ignore_ascii_case(name))
    {
        return false;
    }
    let Some((base, rest)) = name.split_once(" of ") else {
        return false;
    };
    !rest.is_empty()
        && catalog
            .gems
            .iter()
            .any(|gem| gem.name.eq_ignore_ascii_case(base))
}

fn add_lilly_and_trials(
    route: &mut CharacterRoute,
    catalog: &Catalog,
    class_name: &str,
    filter: &HashSet<String>,
) {
    let Some(quest) = catalog.quest("a6q4") else {
        for name in filter {
            if is_transfigured(catalog, name) {
                push_trial(route, catalog, name);
            }
        }
        return;
    };
    for name in filter {
        if is_transfigured(catalog, name) {
            push_trial(route, catalog, name);
            continue;
        }
        let Some(info) = catalog.gems.iter().find(|gem| names_match(name, &gem.name)) else {
            continue;
        };
        if route.events.iter().any(|event| {
            !event.id.starts_with("gem-lilly-")
                && event
                    .gem_name
                    .as_deref()
                    .is_some_and(|gem| names_match(name, gem))
        }) {
            continue;
        }
        let offered = quest.gems.iter().any(|gem| {
            gem.source == GemSource::Vendor
                && gem.gem_id == info.id
                && (gem.classes.is_empty()
                    || gem
                        .classes
                        .iter()
                        .any(|c| c.eq_ignore_ascii_case(class_name)))
        });
        if !offered {
            continue;
        }
        let id = format!("gem-lilly-{}", info.id.replace('/', "_"));
        if route.events.iter().any(|event| event.id == id) {
            continue;
        }
        route.events.push(CampaignEvent {
            id,
            act: 6,
            text: format!(
                "Buy {} from Lilly Roth",
                gem_color(&info.primary_attribute, &info.name)
            ),
            detail: String::new(),
            icon: format!("gem:{}", info.name),
            kind: EventKind::GemBuy,
            start: Vec::new(),
            end: Vec::new(),
            gem_id: Some(info.id.clone()),
            gem_name: Some(info.name.clone()),
            npc: Some("Lilly Roth".into()),
            zone_name: Some("Lioneye's Watch".into()),
            cost_icon: buy_cost(EventKind::GemBuy, info.required_level),
        });
    }
}

fn gem_location(catalog: &Catalog, npc: &str, act: u8, zone_name: &str) -> String {
    if npc.eq_ignore_ascii_case("Siosa") {
        return catalog
            .area("1_3_17_1")
            .map(|area| area.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "The Library".to_string());
    }
    catalog
        .areas
        .iter()
        .find(|area| area.act == act && area.is_town)
        .map(|area| area.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| zone_name.to_string())
}

fn loc(name: &str) -> String {
    tag("loc", name)
}

fn item(name: &str) -> String {
    tag("item", name)
}

fn tag(kind: &str, name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        String::new()
    } else {
        format!("{{{kind}}}{name}{{/}}")
    }
}

fn gem_color(attribute: &str, name: &str) -> String {
    let tag = match attribute {
        "strength" => "red",
        "dexterity" => "green",
        "intelligence" => "blue",
        _ => return name.to_string(),
    };
    format!("{{{tag}}}{name}{{/}}")
}

fn gem_attribute<'a>(catalog: &'a Catalog, name: &str) -> &'a str {
    if let Some(gem) = catalog
        .gems
        .iter()
        .find(|gem| gem.name.eq_ignore_ascii_case(name))
    {
        return gem.primary_attribute.as_str();
    }
    if let Some((base, _)) = name.split_once(" of ") {
        if let Some(gem) = catalog
            .gems
            .iter()
            .find(|gem| gem.name.eq_ignore_ascii_case(base))
        {
            return gem.primary_attribute.as_str();
        }
    }
    "none"
}

fn push_trial(route: &mut CharacterRoute, catalog: &Catalog, name: &str) {
    let id = format!(
        "gem-trial-{}",
        name.replace(|ch: char| !ch.is_ascii_alphanumeric(), "_")
    );
    if route.events.iter().any(|event| event.id == id) {
        return;
    }
    route.events.push(CampaignEvent {
        id,
        act: 3,
        text: format!(
            "Take {} from {}",
            gem_color(gem_attribute(catalog, name), name),
            loc("the Divine Font")
        ),
        detail: String::new(),
        icon: format!("gem:{name}"),
        kind: EventKind::GemPick,
        start: Vec::new(),
        end: Vec::new(),
        gem_id: None,
        gem_name: Some(name.to_string()),
        npc: None,
        zone_name: Some("Aspirant's Trial".into()),
        cost_icon: String::new(),
    });
}

fn buy_cost(kind: EventKind, level: u32) -> String {
    if kind == EventKind::GemBuy {
        vendor_cost_icon(level).to_string()
    } else {
        String::new()
    }
}

pub fn vendor_cost_icon(required_level: u32) -> &'static str {
    if required_level < 8 {
        "wisdom"
    } else if required_level < 16 {
        "transmutation"
    } else if required_level < 28 {
        "alteration"
    } else if required_level < 38 {
        "chance"
    } else {
        "alchemy"
    }
}

pub fn fill_vendor_costs(file: &mut crate::model::CharacterFile, catalog: &Catalog) -> bool {
    let mut changed = false;
    for event in &mut file.route.events {
        if event.kind != EventKind::GemBuy || !event.cost_icon.is_empty() {
            continue;
        }
        let level = event
            .gem_id
            .as_deref()
            .and_then(|id| catalog.gem(id))
            .or_else(|| {
                event
                    .gem_name
                    .as_deref()
                    .and_then(|name| catalog.gem_by_name(name))
            })
            .map(|gem| gem.required_level)
            .unwrap_or(1);
        event.cost_icon = vendor_cost_icon(level).to_string();
        changed = true;
    }
    changed
}

fn assign_leading_area(steps: &mut [RawStep], catalog: &Catalog) {
    let Some(prelude) = steps
        .iter()
        .find_map(|step| step.area_id.as_deref())
        .and_then(|next| area_before(catalog, next))
    else {
        return;
    };
    for step in steps {
        if step.area_id.is_some() {
            break;
        }
        step.area_id = Some(prelude.clone());
    }
}

fn area_before(catalog: &Catalog, next_id: &str) -> Option<String> {
    if next_id == "1_1_town" && catalog.area("1_1_1").is_some() {
        Some("1_1_1".into())
    } else {
        None
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

fn parse_act(
    catalog: &Catalog,
    act: u8,
    text: &str,
    league_start: bool,
    bandit: BanditChoice,
) -> Vec<RawStep> {
    let filtered = apply_conditionals(text, league_start, bandit);
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
        let text = if !label.is_empty() && body.contains("{waypoint|") {
            format!("Waypoint to {}", loc(&label))
        } else if !label.is_empty() && body.contains("{quest|") && !body.contains("{enter|") {
            format!("Hand in {label}")
        } else if enter_only(body) {
            let name = if label.is_empty() {
                rendered.trim().trim_start_matches("Enter ").trim()
            } else {
                label.as_str()
            };
            let name = crate::plain_text(name);
            format!("Enter {}", loc(name.trim()))
        } else if rendered.trim().is_empty() {
            label
        } else {
            rendered
        };
        if text.trim().is_empty() {
            continue;
        }
        let mut detail = Vec::new();
        if text.starts_with("Hand in ") {
            if let Some(place) = hand_in_place(catalog, &quest_ids, act) {
                detail.push(place);
            }
        }
        steps.push(RawStep {
            text: text.trim().to_string(),
            detail,
            icon,
            area_id,
            quest_ids,
            act,
            logout: body.contains("{logout}"),
        });
    }
    steps
}

fn hand_in_place(catalog: &Catalog, quest_ids: &[String], act: u8) -> Option<String> {
    let (quest_id, offers) = quest_ids.split_first()?;
    let quest = catalog.quest(quest_id)?;
    let offered = offers.iter().find_map(|offer| {
        quest
            .gems
            .iter()
            .find(|gem| gem.offer == *offer && !gem.npc.is_empty())
            .map(|gem| gem.npc.clone())
    });
    let npc = offered.or_else(|| {
        if quest.npc.is_empty() {
            None
        } else {
            Some(quest.npc.clone())
        }
    })?;
    let town = catalog
        .areas
        .iter()
        .find(|area| area.act == act && area.is_town && !area.name.is_empty())?
        .name
        .clone();
    Some(format!("{npc} in {town}"))
}

fn enter_only(body: &str) -> bool {
    if !body.contains("{enter|") && !body.contains("{area|") {
        return false;
    }
    let mut cursor = body;
    let mut plain = String::new();
    while let Some(start) = cursor.find('{') {
        plain.push_str(&cursor[..start]);
        let after = &cursor[start + 1..];
        let Some(end) = after.find('}') else {
            return false;
        };
        let kind = after[..end].split('|').next().unwrap_or("");
        if kind != "enter" && kind != "area" {
            return false;
        }
        cursor = &after[end + 1..];
    }
    plain.push_str(cursor);
    plain.replace('➞', " ").split_whitespace().next().is_none()
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

fn render_token(catalog: &Catalog, token: &str) -> (String, String, Option<String>, Vec<String>) {
    let mut parts = token.split('|');
    let kind = parts.next().unwrap_or("");
    let args: Vec<&str> = parts.collect();
    let first = args.first().copied().unwrap_or("");
    match kind {
        "enter" | "area" => {
            let area = catalog.area(first);
            let name = area
                .map(|a| a.name.clone())
                .unwrap_or_else(|| first.to_string());
            let icon = if area.is_some_and(|a| a.is_town) {
                "town"
            } else {
                "portal"
            };
            let label = if kind == "area" {
                loc(&name)
            } else {
                format!("Enter {}", loc(&name))
            };
            (label, icon.into(), Some(first.to_string()), Vec::new())
        }
        "waypoint" => {
            let name = catalog
                .area(first)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| first.to_string());
            (
                format!("Waypoint to {}", loc(&name)),
                "waypoint".into(),
                Some(first.to_string()),
                Vec::new(),
            )
        }
        "waypoint_get" => (loc("the Waypoint"), "waypoint".into(), None, Vec::new()),
        "kill" => (first.to_string(), "quest".into(), None, Vec::new()),
        "quest" => {
            let ids: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
            let name = ids
                .first()
                .and_then(|id| catalog.quest(id).map(|q| q.name.clone()))
                .unwrap_or_else(|| first.to_string());
            (name, "quest".into(), None, ids)
        }
        "quest_text" => (item(first), "quest".into(), None, Vec::new()),
        "generic" => (first.to_string(), "quest".into(), None, Vec::new()),
        "arena" => (loc(first), "quest".into(), None, Vec::new()),
        "logout" => (
            "Logout or Portal to Town".into(),
            "portalscroll".into(),
            None,
            Vec::new(),
        ),
        "trial" => (loc("Trial of Ascendancy"), "trial".into(), None, Vec::new()),
        "crafting" => (
            "Crafting recipe".into(),
            "crafting".into(),
            None,
            Vec::new(),
        ),
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

fn flag_enabled(flag: &str, league_start: bool, bandit: BanditChoice) -> bool {
    match flag.trim() {
        "LEAGUE_START" => league_start,
        "BANDIT_KILL" => bandit == BanditChoice::KillAll,
        "BANDIT_ALIRA" => bandit == BanditChoice::Alira,
        "BANDIT_KRAITYN" => bandit == BanditChoice::Kraityn,
        "BANDIT_OAK" => bandit == BanditChoice::Oak,
        _ => false,
    }
}

fn apply_conditionals(src: &str, league_start: bool, bandit: BanditChoice) -> String {
    let mut out = String::new();
    let mut stack = vec![true];
    for line in src.lines() {
        let t = line.trim();
        if let Some(flag) = t.strip_prefix("#ifdef ") {
            let ok = flag_enabled(flag, league_start, bandit);
            stack.push(*stack.last().unwrap_or(&true) && ok);
            continue;
        }
        if let Some(flag) = t.strip_prefix("#ifndef ") {
            let ok = !flag_enabled(flag, league_start, bandit);
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

pub fn refresh_generated_route(
    file: &mut crate::model::CharacterFile,
    catalog: &Catalog,
    league_start: bool,
) {
    if file.user_edited {
        return;
    }
    let filter: HashSet<String> = file.route.pob_gems.iter().cloned().collect();
    let filtered = if filter.is_empty() {
        None
    } else {
        Some(&filter)
    };
    let bandit = BanditChoice::parse(&file.route.bandit);
    let mut fresh = build_route_with(catalog, &file.class_name, filtered, league_start, bandit);
    fresh.pob_notes = std::mem::take(&mut file.route.pob_notes);
    fresh.pob_url = std::mem::take(&mut file.route.pob_url);
    fresh.pob_gems = std::mem::take(&mut file.route.pob_gems);
    fresh.tree_specs = std::mem::take(&mut file.route.tree_specs);
    file.route = fresh;
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
    let fresh = build_route_with(
        catalog,
        &file.class_name,
        Some(&filter),
        league_start,
        build.bandit,
    );
    if file.user_edited {
        file.route.events.retain(|event| !event.kind.is_gem());
        for group in &mut file.route.groups {
            group
                .event_ids
                .retain(|id| file.route.events.iter().any(|event| &event.id == id));
        }
        for event in fresh.events.into_iter().filter(|event| event.kind.is_gem()) {
            let zone = event.zone_name.clone();
            let id = event.id.clone();
            file.route.events.push(event);
            if let Some(group) = file
                .route
                .groups
                .iter_mut()
                .find(|group| Some(group.name.as_str()) == zone.as_deref())
            {
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
    gem.classes
        .iter()
        .any(|c| c.eq_ignore_ascii_case(class_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GemInfo, QuestInfo};

    #[test]
    fn vendor_cost_follows_the_gem_drop_level() {
        assert_eq!(vendor_cost_icon(1), "wisdom");
        assert_eq!(vendor_cost_icon(4), "wisdom");
        assert_eq!(vendor_cost_icon(8), "transmutation");
        assert_eq!(vendor_cost_icon(12), "transmutation");
        assert_eq!(vendor_cost_icon(16), "alteration");
        assert_eq!(vendor_cost_icon(24), "alteration");
        assert_eq!(vendor_cost_icon(28), "chance");
        assert_eq!(vendor_cost_icon(34), "chance");
        assert_eq!(vendor_cost_icon(38), "alchemy");
        assert_eq!(vendor_cost_icon(72), "alchemy");
    }

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
                AreaInfo {
                    id: "1_1_4_1".into(),
                    name: "The Submerged Passage".into(),
                    act: 1,
                    is_town: false,
                    has_waypoint: true,
                },
                AreaInfo {
                    id: "1_1_2a".into(),
                    name: "The Tidal Island".into(),
                    act: 1,
                    is_town: false,
                    has_waypoint: false,
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
                    offer: String::new(),
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
➞ {enter|1_1_4_1} #The Submerged Passage
{waypoint|1_1_2} #The Coast
➞ {enter|1_1_2a} #The Tidal Island
"
                .into(),
            )],
        }
    }

    #[test]
    fn builds_zone_steps_and_class_gems() {
        let bare = build_route(&catalog(), "Witch", None, true);
        assert!(bare.events.iter().all(|event| !event.kind.is_gem()));
        let filter: HashSet<String> = ["Fireball"].into_iter().map(str::to_string).collect();
        let route = build_route(&catalog(), "Witch", Some(&filter), true);
        assert!(route
            .events
            .iter()
            .any(|e| e.text == "Hand in Enemy at the Gate"));
        assert!(route
            .events
            .iter()
            .any(|e| e.text == "Enter {loc}The Coast{/}"));
        assert!(route
            .events
            .iter()
            .any(|e| e.text == "Get {loc}the Waypoint{/}"));
        assert!(route
            .events
            .iter()
            .any(|e| e.icon == "portal" && e.text == "Enter {loc}The Coast{/}"));
        assert!(route
            .events
            .iter()
            .any(|e| e.text.contains("{blue}Fireball{/}")));
        assert!(route
            .events
            .iter()
            .any(|e| e.detail.contains("Tarkleigh in Lioneye's Watch")));
        let coast = route
            .events
            .iter()
            .find(|e| e.text == "Enter {loc}The Coast{/}")
            .unwrap();
        let coast_group = route.groups.iter().find(|g| g.name == "The Coast").unwrap();
        assert!(coast_group.event_ids.iter().any(|id| id == &coast.id));
        let waypoint = route
            .events
            .iter()
            .find(|e| e.text == "Waypoint to {loc}The Coast{/}")
            .unwrap();
        assert!(route.groups.iter().any(|group| {
            group.name == "The Coast" && group.event_ids.iter().any(|id| id == &waypoint.id)
        }));
        assert!(route.groups.iter().any(|g| g.name == "The Tidal Island"));
        assert!(matches!(
            coast.start[0],
            TriggerKind::ZoneEnter { ref area_id } if area_id == "1_1_2"
        ));
    }

    #[test]
    fn hillock_is_shown_on_the_twilight_strand() {
        let mut data = catalog();
        data.areas.insert(
            0,
            AreaInfo {
                id: "1_1_1".into(),
                name: "The Twilight Strand".into(),
                act: 1,
                is_town: false,
                has_waypoint: false,
            },
        );
        data.routes[0].1 = format!("Find and kill {{kill|Hillock}}\n{}", data.routes[0].1);
        let route = build_route(&data, "Duelist", None, true);
        let group = route
            .groups
            .iter()
            .find(|g| g.name == "The Twilight Strand")
            .unwrap();
        assert!(matches!(
            group.start[0],
            TriggerKind::ZoneEnter { ref area_id } if area_id == "1_1_1"
        ));
        let hillock = route
            .events
            .iter()
            .find(|e| e.text == "Find and kill Hillock")
            .unwrap();
        assert!(group.event_ids.iter().any(|id| id == &hillock.id));
        assert!(matches!(
            hillock.end[0],
            TriggerKind::ZoneEnter { ref area_id } if area_id == "1_1_town"
        ));
    }

    #[test]
    fn support_lilly_and_transfigured_gems() {
        let mut data = catalog();
        data.gems.push(GemInfo {
            id: "void".into(),
            name: "Void Manipulation Support".into(),
            primary_attribute: "intelligence".into(),
            required_level: 8,
            is_support: true,
        });
        data.gems.push(GemInfo {
            id: "added".into(),
            name: "Added Fire Damage Support".into(),
            primary_attribute: "strength".into(),
            required_level: 8,
            is_support: true,
        });
        data.gems.push(GemInfo {
            id: "frost".into(),
            name: "Frostblink".into(),
            primary_attribute: "dexterity".into(),
            required_level: 4,
            is_support: false,
        });
        data.quests[0].gems.push(QuestGem {
            gem_id: "void".into(),
            classes: Vec::new(),
            npc: "Nessa".into(),
            source: GemSource::Vendor,
            offer: String::new(),
        });
        data.quests.push(QuestInfo {
            id: "a6q4".into(),
            name: "Fallen from Grace".into(),
            act: 6,
            npc: "Lilly Roth".into(),
            gems: vec![
                QuestGem {
                    gem_id: "void".into(),
                    classes: Vec::new(),
                    npc: "Lilly Roth".into(),
                    source: GemSource::Vendor,
                    offer: String::new(),
                },
                QuestGem {
                    gem_id: "added".into(),
                    classes: Vec::new(),
                    npc: "Lilly Roth".into(),
                    source: GemSource::Vendor,
                    offer: String::new(),
                },
            ],
        });
        let filter: HashSet<String> = [
            "Void Manipulation",
            "Added Fire Damage",
            "Frostblink of Wintry Blast",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        let route = build_route(&data, "Witch", Some(&filter), true);
        assert!(route.events.iter().any(|event| {
            event.text.contains("Void Manipulation") && !event.id.starts_with("gem-lilly-")
        }));
        assert!(route.events.iter().any(|event| {
            event.id.starts_with("gem-lilly-") && event.text.contains("Added Fire Damage")
        }));
        assert!(!route.events.iter().any(|event| {
            event.id.starts_with("gem-lilly-") && event.text.contains("Void Manipulation")
        }));
        assert!(route.events.iter().any(|event| {
            event.id.starts_with("gem-trial-") && event.text.contains("Frostblink of Wintry Blast")
        }));
        assert!(route.groups.iter().all(|group| {
            group
                .event_ids
                .iter()
                .all(|id| !id.starts_with("gem-lilly-") && !id.starts_with("gem-trial-"))
        }));
    }

    #[test]
    fn gem_rewards_name_the_act_town_except_siosa() {
        let mut data = catalog();
        data.areas.push(AreaInfo {
            id: "1_1_7_1".into(),
            name: "The Lower Prison".into(),
            act: 1,
            is_town: false,
            has_waypoint: true,
        });
        data.areas.push(AreaInfo {
            id: "1_3_town".into(),
            name: "The Sarn Encampment".into(),
            act: 3,
            is_town: true,
            has_waypoint: true,
        });
        data.areas.push(AreaInfo {
            id: "1_3_17_1".into(),
            name: "The Library".into(),
            act: 3,
            is_town: false,
            has_waypoint: true,
        });
        data.gems.push(GemInfo {
            id: "frostblink".into(),
            name: "Frostblink".into(),
            primary_attribute: "dexterity".into(),
            required_level: 4,
            is_support: false,
        });
        data.quests.push(QuestInfo {
            id: "a1q2".into(),
            name: "The Caged Brute".into(),
            act: 1,
            npc: "Tarkleigh".into(),
            gems: vec![QuestGem {
                gem_id: "Metadata/Items/Gems/SkillGemFireball".into(),
                classes: vec!["Witch".into()],
                npc: "Tarkleigh".into(),
                source: GemSource::Vendor,
                offer: String::new(),
            }],
        });
        data.quests.push(QuestInfo {
            id: "a3q12".into(),
            name: "A Fixture of Fate".into(),
            act: 3,
            npc: "Siosa".into(),
            gems: vec![QuestGem {
                gem_id: "frostblink".into(),
                classes: vec!["Witch".into()],
                npc: "Siosa".into(),
                source: GemSource::Vendor,
                offer: String::new(),
            }],
        });
        data.routes[0].1 = "\
➞ {enter|1_1_7_1} #The Lower Prison
Hand in {quest|a1q2} #The Caged Brute
"
        .into();
        data.routes.push((
            3,
            "\
➞ {enter|1_3_17_1} #The Library
Hand in {quest|a3q12} #A Fixture of Fate
"
            .into(),
        ));
        let filter: HashSet<String> = ["Fireball", "Frostblink"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let route = build_route(&data, "Witch", Some(&filter), true);
        let tarkleigh = route
            .events
            .iter()
            .find(|event| event.text.contains("Fireball"))
            .unwrap();
        assert!(
            tarkleigh.text.contains("{loc}Lioneye's Watch{/}"),
            "{}",
            tarkleigh.text
        );
        assert!(
            !tarkleigh.text.contains("Lower Prison"),
            "{}",
            tarkleigh.text
        );
        let siosa = route
            .events
            .iter()
            .find(|event| event.npc.as_deref() == Some("Siosa"))
            .unwrap();
        assert!(siosa.text.contains("{loc}The Library{/}"), "{}", siosa.text);
        assert!(!siosa.text.contains("Sarn"), "{}", siosa.text);
    }

    #[test]
    fn one_reward_per_offer_and_logout_returns_to_town() {
        let mut data = catalog();
        data.gems.push(GemInfo {
            id: "pulse".into(),
            name: "Freezing Pulse".into(),
            primary_attribute: "intelligence".into(),
            required_level: 1,
            is_support: false,
        });
        data.gems.push(GemInfo {
            id: "blink".into(),
            name: "Frostblink".into(),
            primary_attribute: "dexterity".into(),
            required_level: 4,
            is_support: false,
        });
        data.quests[0].gems = vec![
            QuestGem {
                gem_id: "Metadata/Items/Gems/SkillGemFireball".into(),
                classes: vec!["Witch".into()],
                npc: "Tarkleigh".into(),
                source: GemSource::Reward,
                offer: "a1q1".into(),
            },
            QuestGem {
                gem_id: "pulse".into(),
                classes: vec!["Witch".into()],
                npc: "Nessa".into(),
                source: GemSource::Vendor,
                offer: "a1q1".into(),
            },
            QuestGem {
                gem_id: "pulse".into(),
                classes: vec!["Witch".into()],
                npc: "Tarkleigh".into(),
                source: GemSource::Reward,
                offer: "a1q1".into(),
            },
            QuestGem {
                gem_id: "Metadata/Items/Gems/SkillGemFireball".into(),
                classes: vec!["Witch".into()],
                npc: "Nessa".into(),
                source: GemSource::Vendor,
                offer: "a1q1".into(),
            },
            QuestGem {
                gem_id: "blink".into(),
                classes: vec!["Witch".into()],
                npc: "Tarkleigh".into(),
                source: GemSource::Reward,
                offer: "a1q1b".into(),
            },
        ];
        data.quests.push(QuestInfo {
            id: "a1q4".into(),
            name: "Breaking Some Eggs".into(),
            act: 1,
            npc: "Tarkleigh".into(),
            gems: Vec::new(),
        });
        data.routes[0].1 = "\
➞ {enter|1_1_2a} #The Tidal Island
Find and kill {kill|Hailrake}, take {quest_text|Medicine Chest}
{logout}
Hand in {quest|a1q1} #Enemy at the Gate
Hand in {quest|a1q4} #Breaking Some Eggs
{waypoint|1_1_2} #The Coast
"
        .into();
        let filter: HashSet<String> = ["Fireball", "Freezing Pulse", "Frostblink"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let route = build_route(&data, "Witch", Some(&filter), true);
        let picks: Vec<_> = route
            .events
            .iter()
            .filter(|event| event.kind == EventKind::GemPick)
            .collect();
        let buys: Vec<_> = route
            .events
            .iter()
            .filter(|event| event.kind == EventKind::GemBuy)
            .collect();
        assert_eq!(picks.len(), 2, "{picks:?}");
        assert_eq!(buys.len(), 1, "{buys:?}");
        assert!(buys[0].text.contains("Nessa"), "{}", buys[0].text);
        assert!(picks.iter().any(|event| event.text.contains("Frostblink")));
        let tidal = route
            .groups
            .iter()
            .find(|group| group.name == "The Tidal Island")
            .unwrap();
        let last = route
            .events
            .iter()
            .find(|event| event.id == *tidal.event_ids.last().unwrap())
            .unwrap();
        assert_eq!(last.text, "Logout or Portal to Town");
        let eggs = route
            .events
            .iter()
            .find(|event| event.text == "Hand in Breaking Some Eggs")
            .unwrap();
        let town = route
            .groups
            .iter()
            .find(|group| group.event_ids.iter().any(|id| id == &eggs.id))
            .unwrap();
        assert_eq!(town.name, "Lioneye's Watch");
        assert!(tidal.event_ids.iter().all(|id| id != &eggs.id));
        let coast = route
            .groups
            .iter()
            .find(|group| group.name == "The Coast")
            .unwrap();
        let tidal_at = route.groups.iter().position(|group| group.id == tidal.id).unwrap();
        let town_at = route.groups.iter().position(|group| group.id == town.id).unwrap();
        let coast_at = route.groups.iter().position(|group| group.id == coast.id).unwrap();
        assert!(tidal_at < town_at && town_at < coast_at);
    }
}
