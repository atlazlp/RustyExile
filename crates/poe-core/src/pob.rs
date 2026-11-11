use crate::model::{BanditChoice, TreeSpec};
use base64::Engine;
use flate2::read::ZlibDecoder;
use std::io::Read;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct PobBuild {
    pub class_name: String,
    pub ascendancy: String,
    pub notes: String,
    pub gems: Vec<String>,
    pub specs: Vec<TreeSpec>,
    pub bandit: BanditChoice,
}

#[derive(Debug, Error)]
pub enum PobError {
    #[error("path of building code could not be inflated")]
    Inflate,
    #[error("path of building xml is invalid: {0}")]
    Xml(String),
}

pub fn fetch_pob(input: &str) -> Result<String, anyhow::Error> {
    let trimmed = input.trim();
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let url = raw_url(trimmed);
        let body = ureq::get(&url).call()?.into_string()?;
        Ok(extract_code(&body).unwrap_or(body))
    } else {
        Ok(trimmed.to_string())
    }
}

fn raw_url(url: &str) -> String {
    if let Some(id) = url.trim_end_matches('/').rsplit('/').next() {
        if url.contains("pastebin.com/") && !url.contains("/raw/") {
            return format!("https://pastebin.com/raw/{id}");
        }
        if url.contains("pobb.in/") && !url.contains("/pob/") {
            return format!("https://pobb.in/pob/{id}");
        }
    }
    url.to_string()
}

fn extract_code(body: &str) -> Option<String> {
    let trimmed = body.trim();
    if trimmed.starts_with('<') || looks_like_code(trimmed) {
        return Some(trimmed.to_string());
    }
    let mut best = String::new();
    let mut current = String::new();
    for ch in body.chars() {
        if ch.is_ascii_alphanumeric()
            || ch == '+'
            || ch == '/'
            || ch == '='
            || ch == '-'
            || ch == '_'
        {
            current.push(ch);
        } else if current.len() > best.len() {
            best = std::mem::take(&mut current);
        } else {
            current.clear();
        }
    }
    if current.len() > best.len() {
        best = current;
    }
    if best.len() > 80 {
        Some(best)
    } else {
        None
    }
}

fn looks_like_code(text: &str) -> bool {
    let head = text.trim();
    head.starts_with("eN") || head.starts_with("<?xml") || head.contains("<PathOfBuilding")
}

pub fn decode_pob(input: &str) -> Result<PobBuild, PobError> {
    let trimmed = input.trim();
    let xml = if trimmed.contains("<PathOfBuilding") || trimmed.contains("<Build") {
        trimmed.to_string()
    } else {
        inflate_code(trimmed)?
    };
    parse_xml(&xml)
}

fn inflate_code(code: &str) -> Result<String, PobError> {
    let cleaned: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let standard = cleaned.replace('-', "+").replace('_', "/");
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(standard.as_bytes())
        .or_else(|_| {
            let padded = match standard.len() % 4 {
                2 => format!("{standard}=="),
                3 => format!("{standard}="),
                _ => standard.clone(),
            };
            base64::engine::general_purpose::STANDARD.decode(padded.as_bytes())
        })
        .map_err(|_| PobError::Inflate)?;
    let mut decoder = ZlibDecoder::new(bytes.as_slice());
    let mut xml = String::new();
    decoder
        .read_to_string(&mut xml)
        .map_err(|_| PobError::Inflate)?;
    if xml.contains('<') {
        Ok(xml)
    } else {
        Err(PobError::Inflate)
    }
}

fn parse_xml(xml: &str) -> Result<PobBuild, PobError> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| PobError::Xml(e.to_string()))?;
    let mut build = PobBuild {
        class_name: String::new(),
        ascendancy: String::new(),
        notes: String::new(),
        gems: Vec::new(),
        specs: Vec::new(),
        bandit: BanditChoice::KillAll,
    };
    let mut active_spec = 0usize;
    let mut bandits = Vec::new();
    for node in doc.descendants() {
        if node.has_tag_name("Build") {
            if let Some(class_name) = node.attribute("className") {
                build.class_name = class_name.to_string();
            }
            if let Some(ascend) = node.attribute("ascendClassName") {
                build.ascendancy = ascend.to_string();
            }
        }
        if node.has_tag_name("Notes") {
            build.notes = node.text().unwrap_or("").to_string();
        }
        if node.has_tag_name("Gem") {
            if let Some(name) = node
                .attribute("nameSpec")
                .or_else(|| node.attribute("gemName"))
            {
                if !build.gems.iter().any(|g| g.eq_ignore_ascii_case(name)) {
                    build.gems.push(name.to_string());
                }
            }
        }
        if node.has_tag_name("Tree") {
            if let Some(active) = node
                .attribute("activeSpec")
                .and_then(|value| value.parse::<usize>().ok())
            {
                if active > 0 {
                    active_spec = active - 1;
                }
            }
        }
        if node.has_tag_name("Spec") {
            let nodes = node
                .attribute("nodes")
                .unwrap_or("")
                .split(',')
                .filter_map(|n| n.trim().parse::<u32>().ok())
                .collect();
            bandits.push(
                node.attribute("bandit")
                    .map(BanditChoice::parse)
                    .unwrap_or_default(),
            );
            build.specs.push(TreeSpec {
                title: node.attribute("title").unwrap_or("Default").to_string(),
                nodes,
                tree_version: node.attribute("treeVersion").unwrap_or("").to_string(),
                class_id: node.attribute("classId").and_then(|v| v.parse().ok()),
                ascend_class_id: node.attribute("ascendClassId").and_then(|v| v.parse().ok()),
            });
        }
    }
    build.bandit = bandits
        .get(active_spec)
        .copied()
        .or_else(|| bandits.first().copied())
        .unwrap_or_default();
    if build.class_name.is_empty() {
        if let Some(spec) = build.specs.first() {
            build.class_name = class_from_id(spec.class_id.unwrap_or(0)).to_string();
        }
    }
    Ok(build)
}

pub fn class_from_id(id: u32) -> &'static str {
    match id {
        0 => "Scion",
        1 => "Marauder",
        2 => "Ranger",
        3 => "Witch",
        4 => "Duelist",
        5 => "Templar",
        6 => "Shadow",
        _ => "",
    }
}

pub fn apply_pob(route: &mut crate::model::CharacterRoute, build: &PobBuild) {
    route.pob_notes = build.notes.clone();
    route.pob_gems = build.gems.clone();
    route.tree_specs = build.specs.clone();
    route.bandit = build.bandit.as_str().to_string();
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    #[test]
    fn decodes_zlib_code_and_lists_gems() {
        let xml = r#"<PathOfBuilding><Build className="Witch" ascendClassName="Necromancer"><Notes>act notes</Notes></Build><Skills><SkillSet><Skill enabled="true"><Gem nameSpec="Fireball" skillId="Fireball"/><Gem nameSpec="Arcane Surge Support"/></Skill></SkillSet></Skills><Tree><Spec title="Leveling" classId="3" nodes="1,2,3" treeVersion="3_26"/></Tree></PathOfBuilding>"#;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(xml.as_bytes()).unwrap();
        let code = base64::engine::general_purpose::STANDARD.encode(encoder.finish().unwrap());
        let build = decode_pob(&code).unwrap();
        assert_eq!(build.class_name, "Witch");
        assert_eq!(build.gems, vec!["Fireball", "Arcane Surge Support"]);
        assert_eq!(build.specs[0].nodes, vec![1, 2, 3]);
        assert!(build.notes.contains("act notes"));
        assert_eq!(build.bandit, BanditChoice::KillAll);
    }

    #[test]
    fn active_spec_selects_the_bandit() {
        let xml = r#"<PathOfBuilding><Build className="Ranger"></Build><Tree activeSpec="2"><Spec title="A" bandit="Oak" classId="2" nodes="1"/><Spec title="B" bandit="Alira" classId="2" nodes="2"/></Tree></PathOfBuilding>"#;
        let build = decode_pob(xml).unwrap();
        assert_eq!(build.bandit, BanditChoice::Alira);
        assert_eq!(build.specs.len(), 2);
    }
}
