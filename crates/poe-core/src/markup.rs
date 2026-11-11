#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextTone {
    Plain,
    Location,
    Item,
    Strength,
    Dexterity,
    Intelligence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextSpan {
    pub text: String,
    pub tone: TextTone,
}

pub fn parse_markup(input: &str) -> Vec<TextSpan> {
    let mut spans = Vec::new();
    let mut buf = String::new();
    let mut tone = TextTone::Plain;
    let mut rest = input;
    while let Some(start) = rest.find('{') {
        buf.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('}') else {
            buf.push_str(rest);
            rest = "";
            break;
        };
        let tag = &after[..end];
        if let Some(next) = tone_tag(tag) {
            push_span(&mut spans, &mut buf, tone);
            tone = next;
            rest = &after[end + 1..];
            continue;
        }
        if tag == "/" || tag.starts_with('/') {
            push_span(&mut spans, &mut buf, tone);
            tone = TextTone::Plain;
            rest = &after[end + 1..];
            continue;
        }
        buf.push('{');
        rest = after;
    }
    buf.push_str(rest);
    push_span(&mut spans, &mut buf, tone);
    spans
}

pub fn plain_text(input: &str) -> String {
    parse_markup(input)
        .into_iter()
        .map(|span| span.text)
        .collect()
}

pub fn tone_rgb(tone: TextTone) -> Option<[u8; 3]> {
    match tone {
        TextTone::Plain | TextTone::Location => None,
        TextTone::Item => Some([186, 220, 150]),
        TextTone::Strength => Some([214, 84, 72]),
        TextTone::Dexterity => Some([78, 176, 98]),
        TextTone::Intelligence => Some([112, 158, 228]),
    }
}

fn push_span(spans: &mut Vec<TextSpan>, buf: &mut String, tone: TextTone) {
    if buf.is_empty() {
        return;
    }
    spans.push(TextSpan {
        text: std::mem::take(buf),
        tone,
    });
}

fn tone_tag(tag: &str) -> Option<TextTone> {
    if tag.contains('|') || tag.contains(' ') || tag.len() > 12 {
        return None;
    }
    match tag {
        "loc" | "location" => Some(TextTone::Location),
        "item" => Some(TextTone::Item),
        "str" | "red" => Some(TextTone::Strength),
        "dex" | "green" => Some(TextTone::Dexterity),
        "int" | "blue" => Some(TextTone::Intelligence),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_color_a_span_and_plain_text_drops_them() {
        let spans = parse_markup("Enter {loc}The Coast{/} for {item}Glyph{/}");
        assert_eq!(
            spans,
            vec![
                TextSpan {
                    text: "Enter ".into(),
                    tone: TextTone::Plain
                },
                TextSpan {
                    text: "The Coast".into(),
                    tone: TextTone::Location
                },
                TextSpan {
                    text: " for ".into(),
                    tone: TextTone::Plain
                },
                TextSpan {
                    text: "Glyph".into(),
                    tone: TextTone::Item
                },
            ]
        );
        assert_eq!(plain_text("Enter {loc}The Coast{/}"), "Enter The Coast");
        assert_eq!(
            parse_markup("Buy {blue}Frostblink{/}").last().unwrap().tone,
            TextTone::Intelligence
        );
    }
}
