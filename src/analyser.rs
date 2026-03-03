use std::collections::{HashMap, HashSet};

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

/// Minimum height above ground (in Source units) for a valid airshot,
/// matching the threshold used by the F2 SourceMod plugin (supstats2).
const MIN_AIRSHOT_HEIGHT: f32 = 170.0;

/// Returns true if `class` is a non-projectile (hitscan or melee) weapon server class.
///
/// Used to filter out non-lethal hits from weapons that cannot legitimately
/// produce airshots in the traditional sense (rockets, grenades, stickies, etc.).
/// Unknown weapon classes are NOT listed here and default to "allowed".
fn is_hitscan_weapon_class(class: &str) -> bool {
    matches!(
        class,
        "CTFScatterGun" | "CTFMinigun" | "CTFRevolver" | "CTFMedigun" | "CTFLaserPointer"
    ) || class.starts_with("CTFShotgun")
        || class.starts_with("CTFPistol")
        || class.starts_with("CTFSniperRifle")
}

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
pub struct Highlight {
    pub tick: u32,
    pub kind: HighlightKind,
    pub killer: String,
    pub victim: String,
    pub victim_user_id: u16, // raw user ID; used for deduplication (names can collide)
    pub weapon: String,   // empty string = not available (used for non-lethal airshots)
    pub lethal: bool,
    pub height: Option<f32>,
    pub damage: Option<u16>,  // only set for non-lethal airshot hits
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
    /// Current Z coordinate per entity, from DT_TFNonLocalPlayerExclusive (or
    /// DT_TFLocalPlayerExclusive for the demo recorder) prop "m_vecOrigin[2]".
    /// `pub` so tests can pre-populate Z state without going through PacketEntities.
    pub entity_origin_z: HashMap<EntityId, f32>,
    /// Last Z coordinate where FL_ONGROUND was set, per entity.
    /// `pub` so tests can pre-populate Z state without going through PacketEntities.
    pub entity_ground_z: HashMap<EntityId, f32>,
    /// Entity IDs whose server class is a known non-projectile (hitscan/melee) weapon.
    /// Populated from PacketEntities; used to filter non-lethal airshots.
    hitscan_weapon_entities: HashSet<EntityId>,
    /// Maps player entity ID → their currently active weapon entity ID,
    /// decoded from DT_BaseCombatCharacter::m_hActiveWeapon handle.
    player_active_weapon: HashMap<EntityId, EntityId>,
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

    fn compute_height(&self, entity_id: EntityId) -> Option<f32> {
        let current_z = self.entity_origin_z.get(&entity_id)?;
        let ground_z = self.entity_ground_z.get(&entity_id)?;
        // Clamp to 0: origin can briefly dip below ground_z on slopes.
        Some((current_z - ground_z).max(0.0))
    }

    // `detect` accumulates arguments across tasks; a parameter struct will be
    // introduced once the signature is stable (after Task 3 adds `damage`).
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
        if attacker == user_id {
            return;
        }

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
                    victim_user_id: user_id,
                    weapon: weapon.clone(),
                    lethal,
                    height: None,
                    damage: None,
                });
            } else if is_airshot {
                if height.map_or(false, |h| h >= MIN_AIRSHOT_HEIGHT) {
                    self.highlights.push(Highlight {
                        tick,
                        kind: HighlightKind::Airshot,
                        killer,
                        victim,
                        victim_user_id: user_id,
                        weapon,
                        lethal,
                        height,
                        damage: None,
                    });
                }
            }
        }
    }

    fn push_non_lethal_airshot(&mut self, tick: u32, attacker: u16, user_id: u16, damage: u16) {
        if attacker == user_id {
            return;
        }

        let victim_uid = UserId::from(user_id);
        let victim_entity = self.user_to_entity.get(&victim_uid).copied();

        let is_airborne = victim_entity
            .and_then(|eid| self.entity_flags.get(&eid))
            .map(|flags| flags & FL_ONGROUND == 0)
            .unwrap_or(false);

        if !is_airborne {
            return;
        }

        // Reject hits from known hitscan weapons (scattergun, shotgun, minigun, etc.).
        // If the attacker's weapon entity is unknown, err on the side of inclusion.
        let attacker_entity = self.user_to_entity.get(&UserId::from(attacker)).copied();
        let is_hitscan = attacker_entity
            .and_then(|eid| self.player_active_weapon.get(&eid))
            .map(|weid| self.hitscan_weapon_entities.contains(weid))
            .unwrap_or(false);

        if is_hitscan {
            return;
        }

        let height = victim_entity.and_then(|eid| self.compute_height(eid));

        if !height.map_or(false, |h| h >= MIN_AIRSHOT_HEIGHT) {
            return;
        }

        let killer = self.resolve_name(attacker);
        let victim = self.resolve_name(user_id);

        self.highlights.push(Highlight {
            tick,
            kind: HighlightKind::Airshot,
            killer,
            victim,
            victim_user_id: user_id,
            weapon: String::new(),
            lethal: false,
            height,
            damage: Some(damage),
        });
    }

    fn deduplicated_highlights(self) -> Vec<Highlight> {
        use std::collections::HashSet;

        // Key on (tick, victim_user_id) — not victim name — to correctly handle
        // duplicate player names (TF2 allows multiple players to share a display name).
        let lethal_keys: HashSet<(u32, u16)> = self
            .highlights
            .iter()
            .filter(|h| h.lethal && matches!(h.kind, HighlightKind::Airshot))
            .map(|h| (h.tick, h.victim_user_id))
            .collect();

        let mut result: Vec<Highlight> = self
            .highlights
            .into_iter()
            .filter(|h| {
                // Remove non-lethal airshots that have a lethal counterpart at same tick+victim
                if !h.lethal && matches!(h.kind, HighlightKind::Airshot) {
                    !lethal_keys.contains(&(h.tick, h.victim_user_id))
                } else {
                    true
                }
            })
            .collect();

        result.sort_by_key(|h| h.tick);
        result
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
                    let Some(class) = parser_state
                        .server_classes
                        .get(<ClassId as Into<usize>>::into(entity.server_class))
                    else {
                        continue;
                    };
                    let class_name = class.name.as_str();

                    // Track hitscan weapon entities for all entity types.
                    if is_hitscan_weapon_class(class_name) {
                        self.hitscan_weapon_entities.insert(entity.entity_index);
                    }

                    if class_name == "CTFPlayer" {
                        // 1. Update Z position first.
                        // Players split origin into XY + Z: the Z component is the float prop
                        // "m_vecOrigin[2]" in DT_TFNonLocalPlayerExclusive (all other players)
                        // or DT_TFLocalPlayerExclusive (the demo recorder).
                        let origin_z_prop = entity
                            .get_prop_by_name("DT_TFNonLocalPlayerExclusive", "m_vecOrigin[2]", parser_state)
                            .or_else(|| entity.get_prop_by_name("DT_TFLocalPlayerExclusive", "m_vecOrigin[2]", parser_state));
                        if let Some(prop) = origin_z_prop
                            && let SendPropValue::Float(z) = prop.value
                        {
                            self.entity_origin_z.insert(entity.entity_index, z);
                        }

                        // 2. Update flags, and when on ground capture Z as ground reference.
                        // If m_vecOrigin was absent from this delta update, entity_origin_z
                        // still holds the last-known position — the correct ground reference
                        // for a landing event where only m_fFlags changed.
                        if let Some(prop) = entity.get_prop_by_name("DT_BasePlayer", "m_fFlags", parser_state)
                            && let SendPropValue::Integer(flags) = prop.value
                        {
                            let flags = flags as u32;
                            self.entity_flags.insert(entity.entity_index, flags);
                            if flags & FL_ONGROUND != 0
                                && let Some(z) = self.entity_origin_z.get(&entity.entity_index).copied() {
                                    self.entity_ground_z.insert(entity.entity_index, z);
                                }
                        }

                        // 3. Track active weapon (entity handle → entity index, lower 11 bits).
                        if let Some(prop) = entity.get_prop_by_name("DT_BaseCombatCharacter", "m_hActiveWeapon", parser_state)
                            && let SendPropValue::Integer(handle) = prop.value
                        {
                            let weapon_eid = EntityId::from((handle as u32) & 0x7FF);
                            self.player_active_weapon.insert(entity.entity_index, weapon_eid);
                        }
                    }
                }
            }
            Message::GameEvent(event_msg) => {
                match &event_msg.event {
                    GameEvent::PlayerDeath(event) => {
                        let victim_uid = UserId::from(event.user_id);
                        let victim_entity = self.user_to_entity.get(&victim_uid).copied();

                        let is_airborne = victim_entity
                            .and_then(|eid| self.entity_flags.get(&eid))
                            .map(|flags| flags & FL_ONGROUND == 0)
                            .unwrap_or(false);

                        let height = victim_entity.and_then(|eid| self.compute_height(eid));

                        self.detect(
                            u32::from(tick),
                            event.attacker,
                            event.user_id,
                            event.weapon.as_ref(),
                            event.custom_kill,
                            is_airborne,
                            true,
                            height,
                        );
                    }
                    GameEvent::PlayerHurt(event) => {
                        self.push_non_lethal_airshot(
                            u32::from(tick),
                            event.attacker,
                            event.user_id,
                            event.damage_amount,
                        );
                    }
                    _ => {}
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
        self.deduplicated_highlights()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tf_demo_parser::demo::message::packetentities::EntityId;

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

        analyser.detect(200, 3, 4, "tf_projectile_rocket", 0, true, true, Some(200.0));

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
        analyser.detect(100, 1, 2, "tf_projectile_rocket", 0, true, true, Some(200.0));
        assert_eq!(analyser.highlights[0].height, Some(200.0));
    }

    // --- Task 2 tests ---

    #[test]
    fn test_compute_height_above_ground() {
        let mut analyser = HighlightAnalyser::new();
        let eid = EntityId::from(5u32);
        analyser.entity_ground_z.insert(eid, 100.0);
        analyser.entity_origin_z.insert(eid, 184.5);
        assert_eq!(analyser.compute_height(eid), Some(84.5));
    }

    #[test]
    fn test_compute_height_below_ground_clamps_to_zero() {
        let mut analyser = HighlightAnalyser::new();
        let eid = EntityId::from(6u32);
        analyser.entity_ground_z.insert(eid, 200.0);
        analyser.entity_origin_z.insert(eid, 190.0);
        assert_eq!(analyser.compute_height(eid), Some(0.0));
    }

    #[test]
    fn test_compute_height_unknown_returns_none() {
        let analyser = HighlightAnalyser::new();
        let eid = EntityId::from(7u32);
        assert_eq!(analyser.compute_height(eid), None);
    }

    #[test]
    fn test_compute_height_missing_origin_returns_none() {
        let mut analyser = HighlightAnalyser::new();
        let eid = EntityId::from(8u32);
        analyser.entity_ground_z.insert(eid, 100.0);
        // entity_origin_z not populated
        assert_eq!(analyser.compute_height(eid), None);
    }

    // --- Task 3 tests ---

    #[test]
    fn test_non_lethal_airshot_added_when_airborne() {
        let mut analyser = HighlightAnalyser::new();
        let eid = EntityId::from(4u32);
        // Mark victim as airborne (FL_ONGROUND not set)
        analyser.entity_flags.insert(eid, 0);
        analyser.entity_ground_z.insert(eid, 0.0);
        analyser.entity_origin_z.insert(eid, 200.0);
        analyser.user_to_entity.insert(UserId::from(2u16), eid);
        analyser.players.insert(UserId::from(1u16), "A".to_string());
        analyser.players.insert(UserId::from(2u16), "B".to_string());

        analyser.push_non_lethal_airshot(100, 1, 2, 75);
        assert_eq!(analyser.highlights.len(), 1);
        assert!(!analyser.highlights[0].lethal);
        assert_eq!(analyser.highlights[0].damage, Some(75));
    }

    #[test]
    fn test_non_lethal_airshot_not_added_when_grounded() {
        let mut analyser = HighlightAnalyser::new();
        let eid = EntityId::from(5u32);
        // Mark victim as on ground (FL_ONGROUND set)
        analyser.entity_flags.insert(eid, FL_ONGROUND);
        analyser.user_to_entity.insert(UserId::from(2u16), eid);
        analyser.players.insert(UserId::from(1u16), "A".to_string());
        analyser.players.insert(UserId::from(2u16), "B".to_string());

        analyser.push_non_lethal_airshot(100, 1, 2, 75);
        assert!(analyser.highlights.is_empty());
    }

    #[test]
    fn test_deduplication_removes_non_lethal_when_lethal_exists() {
        let mut analyser = HighlightAnalyser::new();
        // One lethal and one non-lethal at same tick for same victim (user_id=2)
        analyser.highlights.push(Highlight {
            tick: 100,
            kind: HighlightKind::Airshot,
            killer: "A".to_string(),
            victim: "B".to_string(),
            victim_user_id: 2,
            weapon: "tf_projectile_rocket".to_string(),
            lethal: true,
            height: None,
            damage: None,
        });
        analyser.highlights.push(Highlight {
            tick: 100,
            kind: HighlightKind::Airshot,
            killer: "A".to_string(),
            victim: "B".to_string(),
            victim_user_id: 2,
            weapon: String::new(),
            lethal: false,
            height: None,
            damage: Some(75),
        });

        let output = analyser.deduplicated_highlights();
        assert_eq!(output.len(), 1);
        assert!(output[0].lethal);
    }

    #[test]
    fn test_deduplication_keeps_non_lethal_with_no_matching_lethal() {
        let mut analyser = HighlightAnalyser::new();
        analyser.highlights.push(Highlight {
            tick: 200,
            kind: HighlightKind::Airshot,
            killer: "A".to_string(),
            victim: "B".to_string(),
            victim_user_id: 2,
            weapon: String::new(),
            lethal: false,
            height: None,
            damage: Some(50),
        });

        let output = analyser.deduplicated_highlights();
        assert_eq!(output.len(), 1);
        assert!(!output[0].lethal);
    }

    #[test]
    fn test_self_airshot_not_recorded() {
        let mut analyser = HighlightAnalyser::new();
        analyser.players.insert(UserId::from(1u16), "A".to_string());
        analyser.detect(100, 1, 1, "tf_projectile_rocket", 0, true, true, Some(200.0));
        assert!(analyser.highlights.is_empty());
    }

    #[test]
    fn test_non_lethal_self_airshot_not_recorded() {
        let mut analyser = HighlightAnalyser::new();
        let eid = EntityId::from(4u32);
        analyser.entity_flags.insert(eid, 0);
        analyser.entity_ground_z.insert(eid, 0.0);
        analyser.entity_origin_z.insert(eid, 200.0);
        analyser.user_to_entity.insert(UserId::from(1u16), eid);
        analyser.players.insert(UserId::from(1u16), "A".to_string());
        analyser.push_non_lethal_airshot(100, 1, 1, 75);
        assert!(analyser.highlights.is_empty());
    }

    #[test]
    fn test_airshot_below_height_threshold_not_recorded() {
        let mut analyser = HighlightAnalyser::new();
        analyser.players.insert(UserId::from(1u16), "A".to_string());
        analyser.players.insert(UserId::from(2u16), "B".to_string());
        analyser.detect(100, 1, 2, "tf_projectile_rocket", 0, true, true, Some(100.0));
        assert!(analyser.highlights.is_empty());
    }

    #[test]
    fn test_airshot_unknown_height_not_recorded() {
        let mut analyser = HighlightAnalyser::new();
        analyser.players.insert(UserId::from(1u16), "A".to_string());
        analyser.players.insert(UserId::from(2u16), "B".to_string());
        analyser.detect(100, 1, 2, "tf_projectile_rocket", 0, true, true, None);
        assert!(analyser.highlights.is_empty());
    }

    #[test]
    fn test_non_lethal_airshot_below_threshold_not_added() {
        let mut analyser = HighlightAnalyser::new();
        let eid = EntityId::from(9u32);
        analyser.entity_flags.insert(eid, 0); // airborne
        analyser.entity_ground_z.insert(eid, 0.0);
        analyser.entity_origin_z.insert(eid, 50.0); // only 50 units high
        analyser.user_to_entity.insert(UserId::from(2u16), eid);
        analyser.players.insert(UserId::from(1u16), "A".to_string());
        analyser.players.insert(UserId::from(2u16), "B".to_string());

        analyser.push_non_lethal_airshot(100, 1, 2, 75);
        assert!(analyser.highlights.is_empty());
    }

    #[test]
    fn test_non_lethal_airshot_unknown_entity_not_added() {
        let mut analyser = HighlightAnalyser::new();
        // No user_to_entity mapping for user_id=2 — unknown entity, treated as grounded
        analyser.players.insert(UserId::from(1u16), "A".to_string());
        analyser.players.insert(UserId::from(2u16), "B".to_string());

        analyser.push_non_lethal_airshot(100, 1, 2, 75);
        assert!(analyser.highlights.is_empty());
    }

    #[test]
    fn test_non_lethal_airshot_hitscan_weapon_rejected() {
        let mut analyser = HighlightAnalyser::new();
        let victim_eid = EntityId::from(10u32);
        let attacker_eid = EntityId::from(11u32);
        let weapon_eid = EntityId::from(20u32);

        // Victim is airborne and high enough
        analyser.entity_flags.insert(victim_eid, 0);
        analyser.entity_ground_z.insert(victim_eid, 0.0);
        analyser.entity_origin_z.insert(victim_eid, 200.0);
        analyser.user_to_entity.insert(UserId::from(2u16), victim_eid);

        // Attacker's active weapon is a scattergun (hitscan)
        analyser.user_to_entity.insert(UserId::from(1u16), attacker_eid);
        analyser.player_active_weapon.insert(attacker_eid, weapon_eid);
        analyser.hitscan_weapon_entities.insert(weapon_eid);

        analyser.players.insert(UserId::from(1u16), "A".to_string());
        analyser.players.insert(UserId::from(2u16), "B".to_string());

        analyser.push_non_lethal_airshot(100, 1, 2, 75);
        assert!(analyser.highlights.is_empty());
    }

    #[test]
    fn test_non_lethal_airshot_unknown_weapon_allowed() {
        let mut analyser = HighlightAnalyser::new();
        let victim_eid = EntityId::from(10u32);
        let attacker_eid = EntityId::from(11u32);

        // Victim is airborne and high enough
        analyser.entity_flags.insert(victim_eid, 0);
        analyser.entity_ground_z.insert(victim_eid, 0.0);
        analyser.entity_origin_z.insert(victim_eid, 200.0);
        analyser.user_to_entity.insert(UserId::from(2u16), victim_eid);

        // Attacker entity known but active weapon entity not tracked
        analyser.user_to_entity.insert(UserId::from(1u16), attacker_eid);

        analyser.players.insert(UserId::from(1u16), "A".to_string());
        analyser.players.insert(UserId::from(2u16), "B".to_string());

        analyser.push_non_lethal_airshot(100, 1, 2, 75);
        assert_eq!(analyser.highlights.len(), 1);
    }
}
