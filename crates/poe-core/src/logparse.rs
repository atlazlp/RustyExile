#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogEvent {
    Generating {
        area_id: String,
    },
    Entered {
        name: String,
    },
    LevelUp {
        name: String,
        class_name: String,
        level: u32,
    },
    Slain {
        name: String,
    },
    Passive {
        id: String,
        name: String,
    },
    Connected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedLog {
    Entered {
        name: String,
        area_id: Option<String>,
    },
    LevelUp {
        name: String,
        class_name: String,
        level: u32,
    },
    Slain {
        name: String,
    },
    Passive {
        id: String,
        name: String,
    },
    Connected,
}

#[derive(Debug, Default)]
pub struct LogParser {
    pending_area: Option<String>,
}

impl LogParser {
    pub fn push_line(&mut self, line: &str) -> Option<ParsedLog> {
        let event = parse_line(line)?;
        match event {
            LogEvent::Generating { area_id } => {
                self.pending_area = Some(area_id);
                None
            }
            LogEvent::Entered { name } => Some(ParsedLog::Entered {
                name,
                area_id: self.pending_area.take(),
            }),
            LogEvent::LevelUp {
                name,
                class_name,
                level,
            } => Some(ParsedLog::LevelUp {
                name,
                class_name,
                level,
            }),
            LogEvent::Slain { name } => Some(ParsedLog::Slain { name }),
            LogEvent::Passive { id, name } => Some(ParsedLog::Passive { id, name }),
            LogEvent::Connected => {
                self.pending_area = None;
                Some(ParsedLog::Connected)
            }
        }
    }
}

pub fn parse_line(line: &str) -> Option<LogEvent> {
    if let Some(id) =
        between(line, "Generating level ", " area \"").and_then(|_| between(line, " area \"", "\""))
    {
        return Some(LogEvent::Generating {
            area_id: id.to_string(),
        });
    }
    if let Some(rest) = line.split("] : You have entered ").nth(1) {
        let name = rest.trim().trim_end_matches('.').trim();
        if !name.is_empty() && !name.contains(':') {
            return Some(LogEvent::Entered {
                name: name.to_string(),
            });
        }
    }
    if let Some(rest) = line.split("] : ").nth(1) {
        if let Some(level_at) = rest.rfind(" is now level ") {
            let head = &rest[..level_at];
            if let Some(open) = head.rfind(" (") {
                if head.ends_with(')') {
                    let name = head[..open].trim();
                    let class_name = head[open + 2..head.len() - 1].trim();
                    let level: u32 = rest[level_at + " is now level ".len()..]
                        .trim()
                        .parse()
                        .ok()?;
                    if !name.is_empty() && !class_name.is_empty() {
                        return Some(LogEvent::LevelUp {
                            name: name.to_string(),
                            class_name: class_name.to_string(),
                            level,
                        });
                    }
                }
            }
        }
        if let Some(name) = rest.strip_suffix(" has been slain.") {
            let name = name.trim();
            if !name.is_empty() && !name.contains(' ')
                || name.chars().any(|c| c == '_')
                || !name.contains(':')
            {
                if !name.contains(':') {
                    return Some(LogEvent::Slain {
                        name: name.to_string(),
                    });
                }
            }
        }
    }
    if let Some(rest) = line
        .split("Successfully allocated passive skill id: ")
        .nth(1)
    {
        let mut parts = rest.split(", name: ");
        let id = parts.next()?.trim();
        let name = parts.next()?.trim();
        if !id.is_empty() {
            return Some(LogEvent::Passive {
                id: id.to_string(),
                name: name.to_string(),
            });
        }
    }
    if line.contains("Connected to ") && line.contains("login.pathofexile.com") {
        return Some(LogEvent::Connected);
    }
    None
}

fn between<'a>(line: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let i = line.find(start)? + start.len();
    let rest = &line[i..];
    let j = rest.find(end)?;
    Some(&rest[..j])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_client_lines() {
        let mut parser = LogParser::default();
        assert!(parser
            .push_line(
                r#"2026/06/25 19:02:48 743100 1186a886 [DEBUG Client 316] Generating level 1 area "1_1_1" with seed 1"#
            )
            .is_none());
        assert_eq!(
            parser.push_line(
                "2026/06/25 19:02:50 745281 cffb06dd [INFO Client 316] : You have entered The Twilight Strand."
            ),
            Some(ParsedLog::Entered {
                name: "The Twilight Strand".into(),
                area_id: Some("1_1_1".into()),
            })
        );
        assert_eq!(
            parser.push_line(
                "2026/06/25 19:11:15 1250604 cffb06dd [INFO Client 316] : ATZ_DontForgetToFRoSS (Templar) is now level 3"
            ),
            Some(ParsedLog::LevelUp {
                name: "ATZ_DontForgetToFRoSS".into(),
                class_name: "Templar".into(),
                level: 3,
            })
        );
        assert_eq!(
            parser.push_line(
                "2026/06/25 19:12:27 1322610 cffb06dd [INFO Client 316] : ATZ_DontForgetToFRoSS has been slain."
            ),
            Some(ParsedLog::Slain {
                name: "ATZ_DontForgetToFRoSS".into(),
            })
        );
        assert_eq!(
            parser.push_line(
                "2026/06/25 19:24:44 2059698 fb7ae327 [INFO Client 316] Successfully allocated passive skill id: spellcaster_notable1873, name: Retribution"
            ),
            Some(ParsedLog::Passive {
                id: "spellcaster_notable1873".into(),
                name: "Retribution".into(),
            })
        );
    }

    #[test]
    fn ignores_chat_that_mentions_gems() {
        assert!(parse_line(
            "2026/06/24 00:55:20 19078634 cffb06dd [INFO Client 868] #SuRyy: how link the gem in chat @@"
        )
        .is_none());
    }
}
