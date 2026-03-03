# TF2 Demo Insights: Design Document

**Date:** 2026-03-03

## Goal

A Rust CLI tool that takes a TF2 `.dem` file and prints a human-readable list of highlight moments — headshots and airshots — with tick numbers and player names.

## Requirements

- Input: path to a `.dem` file (positional CLI argument)
- Output: human-readable text to stdout, one event per line
- Highlights detected:
  - **Headshots**: any kill where `custom_kill == 1` (TF_CUSTOM_HEADSHOT)
  - **Airshots**: any kill where the victim was mid-blast-jump (`rocket_jump == true`)
- Summary line at the end: total highlights, breakdown by type
- Errors go to stderr with exit code 1

## Architecture

Single Rust binary using the `tf-demo-parser` crate. Implements the `MessageHandler` trait with a custom struct that listens for `GameEvent` messages. On each `PlayerDeathEvent`, checks kill flags and records highlights. Player names are resolved from the `userinfo` string table entries.

## Tech Stack

- Rust (edition 2021)
- `tf-demo-parser` — TF2 demo parsing
- `clap` — CLI argument parsing

## Output Format

```
[tick  12345] HEADSHOT   SniperAlex    → ScoutBob      (weapon: tf_sniper_rifle)
[tick  14200] AIRSHOT    SoldierCarla  → MedicDave     (weapon: tf_projectile_rocket)
[tick  15800] AIRSHOT    DemoEve       → ScoutBob      (weapon: tf_projectile_pipe)

--- Summary: 3 highlights (1 headshot, 2 airshots) ---
```

## Airshot Definition

Airshots are detected via the `rocket_jump` field in `PlayerDeathEvent`, which is set when the victim was performing a blast jump (rocket/sticky) at the time of death. This covers the most dramatic and interesting airshots. Full ground-state tracking (to catch regular-jump airshots) is out of scope for v1.

## Error Handling

- File not found / unreadable → stderr message, exit 1
- Parse error → stderr message with context, exit 1
