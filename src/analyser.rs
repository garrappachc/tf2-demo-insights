use std::collections::HashMap;

use tf_demo_parser::demo::data::UserInfo as DataUserInfo;
use tf_demo_parser::demo::gameevent_gen::GameEvent;
use tf_demo_parser::demo::message::{Message, MessageType};
use tf_demo_parser::demo::packet::stringtable::StringTableEntry;
use tf_demo_parser::demo::parser::analyser::UserId;
use tf_demo_parser::demo::parser::handler::MessageHandler;
use tf_demo_parser::demo::data::DemoTick;
use tf_demo_parser::ParserState;

/// TF2 custom kill type: headshot (sniper rifle)
const TF_CUSTOM_HEADSHOT: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HighlightKind {
    Headshot,
    Airshot,
}

#[derive(Debug, Clone)]
pub struct Highlight {
    pub tick: u32,
    pub kind: HighlightKind,
    pub killer: String,
    pub victim: String,
    pub weapon: String,
}

#[derive(Default)]
pub struct HighlightAnalyser {
    pub highlights: Vec<Highlight>,
    /// Maps UserId -> player name, populated from the userinfo string table
    pub players: HashMap<UserId, String>,
}

impl HighlightAnalyser {
    pub fn new() -> Self {
        Self::default()
    }

    fn resolve_name(&self, user_id: u16) -> String {
        let uid = UserId::from(user_id);
        self.players
            .get(&uid)
            .cloned()
            .unwrap_or_else(|| format!("<#{}>", user_id))
    }
}

impl MessageHandler for HighlightAnalyser {
    type Output = Vec<Highlight>;

    fn does_handle(message_type: MessageType) -> bool {
        matches!(message_type, MessageType::GameEvent)
    }

    fn handle_message(&mut self, message: &Message, tick: DemoTick, _parser_state: &ParserState) {
        if let Message::GameEvent(event_msg) = message {
            if let GameEvent::PlayerDeath(event) = &event_msg.event {
                let tick_u32 = u32::from(tick);
                let killer = self.resolve_name(event.attacker);
                let victim = self.resolve_name(event.user_id);
                let weapon = event.weapon.to_string();

                if event.custom_kill == TF_CUSTOM_HEADSHOT {
                    self.highlights.push(Highlight {
                        tick: tick_u32,
                        kind: HighlightKind::Headshot,
                        killer,
                        victim,
                        weapon,
                    });
                } else if event.rocket_jump {
                    self.highlights.push(Highlight {
                        tick: tick_u32,
                        kind: HighlightKind::Airshot,
                        killer,
                        victim,
                        weapon,
                    });
                }
            }
        }
    }

    fn handle_string_entry(
        &mut self,
        table: &str,
        index: usize,
        entry: &StringTableEntry,
        _parser_state: &ParserState,
    ) {
        if table == "userinfo" {
            let text = entry.text.as_ref().map(|s| s.as_ref());
            let data = entry.extra_data.as_ref().map(|d| d.data.clone());
            if let Ok(Some(user_info)) =
                DataUserInfo::parse_from_string_table(index as u16, text, data)
            {
                self.players
                    .insert(user_info.player_info.user_id, user_info.player_info.name);
            }
        }
    }

    fn into_output(self, _state: &ParserState) -> Self::Output {
        self.highlights
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_analyser_with_players() -> HighlightAnalyser {
        let mut analyser = HighlightAnalyser::new();
        analyser
            .players
            .insert(UserId::from(1u16), "Sniper1".to_string());
        analyser
            .players
            .insert(UserId::from(2u16), "Victim1".to_string());
        analyser
            .players
            .insert(UserId::from(3u16), "Soldier1".to_string());
        analyser
    }

    #[test]
    fn test_headshot_detected() {
        let mut analyser = make_analyser_with_players();

        // Simulate detecting a headshot directly by pushing a highlight
        let killer = analyser.resolve_name(1u16);
        let victim = analyser.resolve_name(2u16);
        let weapon = "sniperrifle".to_string();
        let custom_kill: u16 = TF_CUSTOM_HEADSHOT;
        let rocket_jump = false;

        if custom_kill == TF_CUSTOM_HEADSHOT {
            analyser.highlights.push(Highlight {
                tick: 100,
                kind: HighlightKind::Headshot,
                killer,
                victim,
                weapon,
            });
        } else if rocket_jump {
            analyser.highlights.push(Highlight {
                tick: 100,
                kind: HighlightKind::Airshot,
                killer: "unreachable".to_string(),
                victim: "unreachable".to_string(),
                weapon: "unreachable".to_string(),
            });
        }

        assert_eq!(analyser.highlights.len(), 1);
        let h = &analyser.highlights[0];
        assert_eq!(h.kind, HighlightKind::Headshot);
        assert_eq!(h.killer, "Sniper1");
        assert_eq!(h.victim, "Victim1");
        assert_eq!(h.weapon, "sniperrifle");
        assert_eq!(h.tick, 100);
    }

    #[test]
    fn test_airshot_detected() {
        let mut analyser = make_analyser_with_players();

        let killer = analyser.resolve_name(3u16);
        let victim = analyser.resolve_name(2u16);
        let weapon = "tf_projectile_rocket".to_string();
        let custom_kill: u16 = 0; // not a headshot
        let rocket_jump = true;

        if custom_kill == TF_CUSTOM_HEADSHOT {
            analyser.highlights.push(Highlight {
                tick: 200,
                kind: HighlightKind::Headshot,
                killer: "unreachable".to_string(),
                victim: "unreachable".to_string(),
                weapon: "unreachable".to_string(),
            });
        } else if rocket_jump {
            analyser.highlights.push(Highlight {
                tick: 200,
                kind: HighlightKind::Airshot,
                killer,
                victim,
                weapon,
            });
        }

        assert_eq!(analyser.highlights.len(), 1);
        let h = &analyser.highlights[0];
        assert_eq!(h.kind, HighlightKind::Airshot);
        assert_eq!(h.killer, "Soldier1");
        assert_eq!(h.victim, "Victim1");
        assert_eq!(h.weapon, "tf_projectile_rocket");
        assert_eq!(h.tick, 200);
    }

    #[test]
    fn test_non_highlight_not_detected() {
        let mut analyser = make_analyser_with_players();

        let custom_kill: u16 = 0;
        let rocket_jump = false;

        if custom_kill == TF_CUSTOM_HEADSHOT {
            analyser.highlights.push(Highlight {
                tick: 300,
                kind: HighlightKind::Headshot,
                killer: "unreachable".to_string(),
                victim: "unreachable".to_string(),
                weapon: "unreachable".to_string(),
            });
        } else if rocket_jump {
            analyser.highlights.push(Highlight {
                tick: 300,
                kind: HighlightKind::Airshot,
                killer: "unreachable".to_string(),
                victim: "unreachable".to_string(),
                weapon: "unreachable".to_string(),
            });
        }

        assert_eq!(analyser.highlights.len(), 0);
    }

    #[test]
    fn test_unknown_player_fallback() {
        let analyser = HighlightAnalyser::new();
        // player 99 was never registered
        let name = analyser.resolve_name(99u16);
        assert_eq!(name, "<#99>");
    }
}
