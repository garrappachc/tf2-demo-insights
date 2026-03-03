use std::collections::HashMap;

use tf_demo_parser::demo::data::UserInfo as DataUserInfo;
use tf_demo_parser::demo::gameevent_gen::GameEvent;
use tf_demo_parser::demo::message::packetentities::EntityId;
use tf_demo_parser::demo::message::{Message, MessageType};
use tf_demo_parser::demo::packet::datatable::ClassId;
use tf_demo_parser::demo::packet::stringtable::StringTableEntry;
use tf_demo_parser::demo::parser::analyser::UserId;
use tf_demo_parser::demo::parser::handler::MessageHandler;
use tf_demo_parser::demo::data::DemoTick;
use tf_demo_parser::demo::sendprop::SendPropValue;
use tf_demo_parser::ParserState;

/// TF2 custom kill type: headshot (sniper rifle)
const TF_CUSTOM_HEADSHOT: u16 = 1;

/// Source Engine FL_ONGROUND flag — set when the player is touching the ground.
const FL_ONGROUND: u32 = 1 << 0;

/// Returns true if `weapon` is a projectile-firing weapon.
///
/// TF2 demos report the killing weapon as either the projectile entity classname
/// (e.g. `tf_projectile_rocket`) or the weapon's item name (e.g. `iron_bomber`).
fn is_projectile_weapon(weapon: &str) -> bool {
    weapon.starts_with("tf_projectile_")
        || matches!(
            weapon,
            "iron_bomber"
                | "loose_cannon"
                | "quickiebomb_launcher"
                | "quake_rl"
        )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HighlightKind {
    Headshot,
    Airshot,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Highlight {
    pub tick: u32,
    pub kind: HighlightKind,
    pub killer: String,
    pub victim: String,
    pub weapon: String,   // empty string = not available (used for non-lethal airshots)
    pub lethal: bool,
    pub height: Option<f32>,
    pub damage: Option<u16>,  // only set for non-lethal airshot hits; populated in Task 3
}

#[derive(Default)]
pub struct HighlightAnalyser {
    pub highlights: Vec<Highlight>,
    /// Maps UserId -> player name, populated from the userinfo string table
    pub players: HashMap<UserId, String>,
    /// Tracks m_fFlags per player entity, updated from PacketEntities messages.
    entity_flags: HashMap<EntityId, u32>,
    /// Maps UserId to entity index, populated from the userinfo string table.
    user_to_entity: HashMap<UserId, EntityId>,
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

    #[allow(clippy::too_many_arguments)]
    fn detect(
        &mut self,
        tick: u32,
        attacker: u16,
        user_id: u16,
        weapon: &str,
        custom_kill: u16,
        is_airborne: bool,
        lethal: bool,
        height: Option<f32>,
    ) {
        let is_headshot = custom_kill == TF_CUSTOM_HEADSHOT;
        // Airshot: victim was not touching the ground (FL_ONGROUND not set in m_fFlags)
        // as tracked from PacketEntities messages.
        let is_airshot = is_airborne && is_projectile_weapon(weapon);

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
                    lethal,
                    height: None,
                    damage: None,
                });
            } else if is_airshot {
                self.highlights.push(Highlight {
                    tick,
                    kind: HighlightKind::Airshot,
                    killer,
                    victim,
                    weapon,
                    lethal,
                    height,
                    damage: None,
                });
            }
        }
    }
}

impl MessageHandler for HighlightAnalyser {
    type Output = Vec<Highlight>;

    fn does_handle(message_type: MessageType) -> bool {
        matches!(message_type, MessageType::GameEvent | MessageType::PacketEntities)
    }

    fn handle_message(&mut self, message: &Message, tick: DemoTick, parser_state: &ParserState) {
        match message {
            Message::PacketEntities(entity_msg) => {
                for entity in &entity_msg.entities {
                    if let Some(class) = parser_state
                        .server_classes
                        .get(<ClassId as Into<usize>>::into(entity.server_class))
                        && class.name.as_str() == "CTFPlayer"
                        && let Some(prop) = entity.get_prop_by_name(
                            "DT_BasePlayer",
                            "m_fFlags",
                            parser_state,
                        )
                        && let SendPropValue::Integer(flags) = prop.value
                    {
                        self.entity_flags.insert(entity.entity_index, flags as u32);
                    }
                }
            }
            Message::GameEvent(event_msg) => {
                if let GameEvent::PlayerDeath(event) = &event_msg.event {
                    let victim_uid = UserId::from(event.user_id);
                    let is_airborne = self
                        .user_to_entity
                        .get(&victim_uid)
                        .and_then(|eid| self.entity_flags.get(eid))
                        .map(|flags| flags & FL_ONGROUND == 0)
                        .unwrap_or(false);

                    self.detect(
                        u32::from(tick),
                        event.attacker,
                        event.user_id,
                        event.weapon.as_ref(),
                        event.custom_kill,
                        is_airborne,
                        true,   // lethal = true for PlayerDeath
                        None,   // height will be computed in Task 2; None for now
                    );
                }
            }
            _ => {}
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
                self.user_to_entity.insert(user_info.player_info.user_id, user_info.entity_id);
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

        analyser.detect(100, 1, 2, "tf_sniper_rifle", TF_CUSTOM_HEADSHOT, false, true, None);

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

        analyser.detect(200, 3, 4, "tf_projectile_rocket", 0, true, true, None);

        assert_eq!(analyser.highlights.len(), 1);
        assert!(matches!(analyser.highlights[0].kind, HighlightKind::Airshot));
        assert_eq!(analyser.highlights[0].killer, "SoldierCarla");
        assert_eq!(analyser.highlights[0].victim, "MedicDave");
    }

    #[test]
    fn test_non_highlight_not_detected() {
        let mut analyser = HighlightAnalyser::new();
        analyser.detect(300, 1, 2, "tf_pistol", 0, false, true, None);
        assert!(analyser.highlights.is_empty());
    }

    #[test]
    fn test_unknown_player_fallback() {
        let analyser = HighlightAnalyser::new();
        let name = analyser.resolve_name(99u16);
        assert_eq!(name, "<#99>");
    }

    #[test]
    fn test_lethal_flag_set_on_kill() {
        let mut analyser = HighlightAnalyser::new();
        analyser.players.insert(UserId::from(1u16), "A".to_string());
        analyser.players.insert(UserId::from(2u16), "B".to_string());
        analyser.detect(100, 1, 2, "tf_sniper_rifle", TF_CUSTOM_HEADSHOT, false, true, None);
        assert!(analyser.highlights[0].lethal);
    }

    #[test]
    fn test_height_stored_on_highlight() {
        let mut analyser = HighlightAnalyser::new();
        analyser.players.insert(UserId::from(1u16), "A".to_string());
        analyser.players.insert(UserId::from(2u16), "B".to_string());
        analyser.detect(100, 1, 2, "tf_projectile_rocket", 0, true, true, Some(84.5));
        assert_eq!(analyser.highlights[0].height, Some(84.5));
    }
}
