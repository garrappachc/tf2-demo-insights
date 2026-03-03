# Height + All Airshots Design

**Date:** 2026-03-03

## Goals

1. Show how high above the ground the victim was at the time of each airshot (lethal and non-lethal)
2. Detect ALL projectile hits on airborne players, not just lethal ones

## New Entity State

Track two additional properties per `CTFPlayer` entity from `PacketEntities`:

- `entity_origin_z: HashMap<EntityId, f32>` — current Z coordinate from `DT_BaseEntity::m_vecOrigin`
- `entity_ground_z: HashMap<EntityId, f32>` — last Z recorded while `FL_ONGROUND` was set

**Height formula:** `max(0.0, entity_origin_z[id] - entity_ground_z[id])`

Both are updated on every `PacketEntities` message for `CTFPlayer` entities. When `FL_ONGROUND` is set, also copy current Z into `entity_ground_z`.

## Highlight Struct Changes

Add to `Highlight`:
- `height: Option<f32>` — height above last ground contact in Source units; `None` if unavailable
- `lethal: bool` — `true` for kills (`PlayerDeath`), `false` for non-lethal hits (`PlayerHurt`)

## Non-Lethal Airshot Detection

Listen to `GameEvent::PlayerHurt` in addition to `GameEvent::PlayerDeath`.

On `PlayerHurtEvent`:
- Check victim is airborne via `entity_flags` (same as death events)
- **No weapon filtering** — `PlayerHurtEvent` only carries `weapon_id` (u16), not a weapon name string. Filtering non-lethal airshots by weapon type would require tracking weapon entities (out of scope). All hits on airborne players are counted.
- No double-counting: `PlayerHurt` fires for all damage including lethal hits. `PlayerDeath` fires for the kill. The lethality is distinguished by which event triggered the highlight — hurt events produce `lethal: false` highlights, death events produce `lethal: true` highlights.

## Output Format

All highlights printed in tick order, one per line:

```
[tick   2275] AIRSHOT*  maly          →  Pawel         +84.0u  (weapon: tf_projectile_rocket)
[tick   2300] AIRSHOT   maly          →  Pawel         +52.3u  (75 dmg)
[tick  14532] HEADSHOT  Kazachu       →  deli                  (weapon: sniperrifle)
```

- `AIRSHOT*` = lethal airshot — shows weapon name, height
- `AIRSHOT ` = non-lethal airshot — shows damage amount, height (no weapon name)
- `HEADSHOT` = headshot kill — unchanged from before
- Height is in Source units, one decimal place, prefixed with `+`

Summary line:
```
--- Summary: 3 headshots | 89 airshots (56 lethal) ---
```

## Out of Scope

- Weapon name for non-lethal airshots (requires weapon entity tracking)
- Absolute Z display
- Hit marker for headshots that are also airshots (treated as headshot only, since headshot is already detected from `PlayerDeath`)
