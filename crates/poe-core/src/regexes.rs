use crate::model::RegexPreset;

pub fn default_regex_index() -> usize {
    default_presets()
        .iter()
        .position(|preset| preset.name == "Leveling (3-link, 4-link, move, +1)")
        .unwrap_or(0)
}

pub fn default_presets() -> Vec<RegexPreset> {
    vec![
        RegexPreset {
            name: "3-link (any colour)".into(),
            regex: r#"".-.-.""#.into(),
        },
        RegexPreset {
            name: "4-link (any colour)".into(),
            regex: r#"".-.-.-.""#.into(),
        },
        RegexPreset {
            name: "Movement speed".into(),
            regex: r#""movement speed""#.into(),
        },
        RegexPreset {
            name: "+1 spell gems".into(),
            regex: r#""ll g""#.into(),
        },
        RegexPreset {
            name: "Leveling (3-link, 4-link, move, +1)".into(),
            regex: r#"".-.-."|".-.-.-."|"movement speed"|"ll g""#.into(),
        },
        RegexPreset {
            name: "Life and resists".into(),
            regex: r#""maximum life"|"resistance""#.into(),
        },
        RegexPreset {
            name: "6-link".into(),
            regex: r#""(-.){5}""#.into(),
        },
        RegexPreset {
            name: "Chaos recipe".into(),
            regex: r#""rare" "item level: (6[0-9]|7[0-4])""#.into(),
        },
    ]
}

pub fn vendor_gem_regex(names: &[String]) -> String {
    let mut parts = Vec::new();
    for name in names {
        let mut escaped = String::new();
        for ch in name.to_ascii_lowercase().chars() {
            if matches!(ch, '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\' | '^' | '$') {
                escaped.push('\\');
            }
            escaped.push(ch);
        }
        if !escaped.is_empty() {
            parts.push(escaped);
        }
    }
    if parts.is_empty() {
        return String::new();
    }
    format!("\"n:.*({}).*\"", parts.join("|"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_selection_is_the_leveling_preset() {
        let presets = default_presets();
        let index = default_regex_index();
        assert_eq!(presets[index].name, "Leveling (3-link, 4-link, move, +1)");
        assert_eq!(crate::model::Settings::default().active_regex, index);
    }

    #[test]
    fn builds_vendor_search() {
        let regex = vendor_gem_regex(&["Molten Strike".into(), "Frostblink".into()]);
        assert_eq!(regex, "\"n:.*(molten strike|frostblink).*\"");
    }
}
