use crate::model::RegexPreset;

pub fn default_presets() -> Vec<RegexPreset> {
    vec![
        RegexPreset {
            name: "2H Melee (RRG + MS boots)".into(),
            regex: r"-\w-.-|r-r-|-r-r|r-.-r|g-g-|-g-g|g-.-g|Runn|rint".into(),
        },
        RegexPreset {
            name: "Caster (BBB + MS boots)".into(),
            regex: r"-\w-.-|b-b-|-b-b|b-.-b|Runn|rint".into(),
        },
        RegexPreset {
            name: "Bow / Dex (GGG + MS boots)".into(),
            regex: r"-\w-.-|g-g-|-g-g|g-.-g|Runn|rint".into(),
        },
        RegexPreset {
            name: "Spellcaster (BBG + MS boots)".into(),
            regex: r"-\w-.-|b-b-|-b-b|b-.-b|g-g-|-g-g|g-.-g|Runn|rint".into(),
        },
        RegexPreset {
            name: "Chromatic Recipe (RGB linked)".into(),
            regex: r":.*(?=\S*r)(?=\S*g)(?=\S*b)".into(),
        },
        RegexPreset {
            name: "+1 Spell Wand (vendor)".into(),
            regex: "ll g".into(),
        },
        RegexPreset {
            name: "Templar Early (2B/RB/RR + RG + MS + +1 Wand)".into(),
            regex: r":.*(?=\S*r)(?=\S*g)(?=\S*b)|r-r|b-b|g-r|r-g|Runn|ll g".into(),
        },
        RegexPreset {
            name: "Caster Mid (BBB + 4L + RGB + MS + +1 Wand)".into(),
            regex: r"-\w-.-|b-b-b|:.*(?=\S*r)(?=\S*g)(?=\S*b)|Runn|rint|ll g".into(),
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
    fn builds_vendor_search() {
        let regex = vendor_gem_regex(&["Molten Strike".into(), "Frostblink".into()]);
        assert_eq!(regex, "\"n:.*(molten strike|frostblink).*\"");
    }
}
