# tf2-demo-insights

A CLI tool that parses TF2 `.dem` files and prints highlight moments — headshots and airshots.

## Usage

```
tf2-demo-insights <path/to/file.dem>
```

### Example output

```
[tick  15749] AIRSHOT*  rozen                 →  clean mousepad owner   +467.9u  (weapon: tf_projectile_pipe_remote)
[tick  16213] HEADSHOT  Phrog                 →  Rahmed                          (weapon: sniperrifle)
[tick  18074] AIRSHOT   Rahmed                →  Rahmed                 +464.5u  (19 dmg)

--- Summary: 3 headshots | 83 airshots (5 lethal) ---
```

**`AIRSHOT*`** — lethal airshot (the killing blow).
**`AIRSHOT`** — non-lethal airshot; trailing number is damage dealt.
**Height** — victim's altitude above their last ground contact, in Source units. A `+` prefix is always shown.

## Highlight detection

### Headshots
Kills where `custom_kill == TF_CUSTOM_HEADSHOT` (value `1`) in the `player_death` game event.

### Airshots
The victim must be:
- **Airborne** — `FL_ONGROUND` not set in `m_fFlags` at the moment of the hit
- **High enough** — ≥ 170 Source units above their last ground contact (matches the [supstats2](https://github.com/F2/F2s-sourcemod-plugins) threshold)

The weapon must be a **projectile weapon** — rockets, grenades, stickies, and item-named equivalents (e.g. `iron_bomber`, `quake_rl`).

For non-lethal hits, the attacker's active weapon entity is also checked against known hitscan server classes (`CTFScatterGun`, `CTFShotgun*`, `CTFMinigun`, `CTFPistol*`, `CTFSniperRifle*`, `CTFRevolver`) and rejected if matched.

When both a lethal and a non-lethal airshot are recorded for the same victim at the same tick, the non-lethal entry is deduplicated away.

## Known limitations

- Airshot detection relies on `FL_ONGROUND` being clear at impact time. Victims who are airborne but below the 170-unit threshold (e.g. just hopped off a ledge) are excluded.
- Height is measured relative to the last position where `FL_ONGROUND` was set, which may be slightly off on slopes or during fast falls.
- `<#N>` player names indicate hits where the player's userinfo entry was not seen in the demo stream (uncommon).

## Build

Requires Rust (stable). No additional system dependencies.

```
cargo build --release
```

Binary lands at `target/release/tf2-demo-insights`.

## Dependencies

- [`tf-demo-parser`](https://crates.io/crates/tf-demo-parser) `0.6` — TF2 demo parsing
- [`clap`](https://crates.io/crates/clap) `4` — CLI argument parsing
