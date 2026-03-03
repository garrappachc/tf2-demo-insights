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

    fn detect(&mut self, tick: u32, attacker: u16, user_id: u16, weapon: &str, custom_kill: u16, rocket_jump: bool) {
        let is_headshot = custom_kill == TF_CUSTOM_HEADSHOT;
        let is_airshot = rocket_jump;

        if is_headshot || is_airshot {
            let killer = self.resolve_name(attacker);
            let victim = self.resolve_name(user_id);
            let weapon = weapon.to_string();

            if is_headshot {
                self.highlights.push(Highlight {
                    tick,
                    kind: HighlightKind::Headshot,
                    killer: killer.clone(),
                    victim: victim.clone(),
                    weapon: weapon.clone(),
                });
            }
            if is_airshot {
                self.highlights.push(Highlight {
                    tick,
                    kind: HighlightKind::Airshot,
                    killer,
                    victim,
                    weapon,
                });
            }
        }
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
                self.detect(
                    u32::from(tick),
                    event.attacker,
                    event.user_id,
                    event.weapon.as_ref(),
                    event.custom_kill,
                    event.rocket_jump,
                );
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

    #[test]
    fn test_headshot_detected() {
        let mut analyser = HighlightAnalyser::new();
        analyser.players.insert(UserId::from(1u16), "SniperAlex".to_string());
        analyser.players.insert(UserId::from(2u16), "ScoutBob".to_string());

        analyser.detect(100, 1, 2, "tf_sniper_rifle", TF_CUSTOM_HEADSHOT, false);

        assert_eq!(analyser.highlights.len(), 1);
        assert!(matches!(analyser.highlights[0].kind, HighlightKind::Headshot));
        assert_eq!(analyser.highlights[0].killer, "SniperAlex");
        assert_eq!(analyser.highlights[0].victim, "ScoutBob");
        assert_eq!(analyser.highlights[0].weapon, "tf_sniper_rifle");
    }

    #[test]
    fn test_airshot_detected() {
        let mut analyser = HighlightAnalyser::new();
        analyser.players.insert(UserId::from(3u16), "SoldierCarla".to_string());
        analyser.players.insert(UserId::from(4u16), "MedicDave".to_string());

        analyser.detect(200, 3, 4, "tf_projectile_rocket", 0, true);

        assert_eq!(analyser.highlights.len(), 1);
        assert!(matches!(analyser.highlights[0].kind, HighlightKind::Airshot));
        assert_eq!(analyser.highlights[0].killer, "SoldierCarla");
        assert_eq!(analyser.highlights[0].victim, "MedicDave");
    }

    #[test]
    fn test_non_highlight_not_detected() {
        let mut analyser = HighlightAnalyser::new();
        analyser.detect(300, 1, 2, "tf_pistol", 0, false);
        assert!(analyser.highlights.is_empty());
    }

    #[test]
    fn test_unknown_player_fallback() {
        let analyser = HighlightAnalyser::new();
        let name = analyser.resolve_name(99u16);
        assert_eq!(name, "<#99>");
    }
}
