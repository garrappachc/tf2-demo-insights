# TF2 Demo Insights Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** A Rust CLI that parses a TF2 `.dem` file and prints headshot and airshot highlights with tick numbers and player names.

**Architecture:** Single Rust binary. Custom `MessageHandler` impl using `tf-demo-parser`. Listens for `GameEvent` messages, checks `PlayerDeathEvent.custom_kill` (headshot) and `PlayerDeathEvent.rocket_jump` (airshot), resolves player names from string table entries.

**Tech Stack:** Rust 2021, `tf-demo-parser = "0.6"`, `clap = "4"`

---

### Task 1: Scaffold the Cargo project

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`

**Step 1: Initialize the project**

Run from `/home/garapich/Coding/tf2-demo-insights`:
```bash
cargo init --name tf2-demo-insights
```
Expected: creates `Cargo.toml` and `src/main.rs`

**Step 2: Add dependencies**

Edit `Cargo.toml` to add under `[dependencies]`:
```toml
[dependencies]
tf-demo-parser = "0.6"
clap = { version = "4", features = ["derive"] }
```

**Step 3: Verify it compiles**

```bash
cargo build
```
Expected: compiles successfully (Hello World binary)

**Step 4: Commit**

```bash
git init
git add Cargo.toml Cargo.lock src/main.rs docs/
git commit -m "chore: scaffold project with dependencies"
```

---

### Task 2: Write a failing test for headshot detection

**Files:**
- Create: `src/analyser.rs`
- Modify: `src/main.rs`

**Context:** `PlayerDeathEvent` has:
- `custom_kill: u16` — set to `1` for headshots (`TF_CUSTOM_HEADSHOT`)
- `rocket_jump: bool` — `true` when the victim was blast-jumping (airshot)
- `weapon: String` — weapon name (e.g. `"tf_sniper_rifle"`)
- `user_id: UserId` — killer's user ID
- `victim_ent_index: u32` — victim's entity index

The `MessageHandler` trait from `tf-demo-parser` requires:
```rust
impl MessageHandler for MyAnalyser {
    type Output = Vec<Highlight>;

    fn does_handle(message_type: MessageType) -> bool { ... }
    fn handle_message(&mut self, message: &Message, tick: DemoTick, parser_state: &ParserState) { ... }
    fn handle_string_entry(&mut self, table: &str, index: usize, entry: &StringTableEntry, parser_state: &ParserState) { ... }
    fn into_output(self, state: &ParserState) -> Self::Output { ... }
}
```

**Step 1: Create `src/analyser.rs` with the `Highlight` type and detection logic**

```rust
use std::collections::HashMap;
use tf_demo_parser::demo::gameevent_gen::GameEvent;
use tf_demo_parser::demo::message::usermessage::UserMessage;
use tf_demo_parser::demo::message::Message;
use tf_demo_parser::demo::parser::MessageHandler;
use tf_demo_parser::demo::parser::analyser::StringTableEntry;
use tf_demo_parser::demo::packet::stringtable::StringTableEntry as PacketStringTableEntry;
use tf_demo_parser::MessageType;
use tf_demo_parser::ParserState;
use tf_demo_parser::DemoTick;

const TF_CUSTOM_HEADSHOT: u16 = 1;

#[derive(Debug, PartialEq)]
pub enum HighlightKind {
    Headshot,
    Airshot,
}

#[derive(Debug)]
pub struct Highlight {
    pub tick: u32,
    pub kind: HighlightKind,
    pub killer: String,
    pub victim: String,
    pub weapon: String,
}

#[derive(Default)]
pub struct HighlightAnalyser {
    highlights: Vec<Highlight>,
    // Maps user_id (u16) to player name
    players: HashMap<u16, String>,
}

impl HighlightAnalyser {
    pub fn new() -> Self {
        Self::default()
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
                let is_headshot = event.custom_kill == TF_CUSTOM_HEADSHOT;
                let is_airshot = event.rocket_jump;

                if is_headshot || is_airshot {
                    let killer = self
                        .players
                        .get(&event.attacker)
                        .cloned()
                        .unwrap_or_else(|| format!("player#{}", event.attacker));
                    let victim = self
                        .players
                        .get(&event.user_id)
                        .cloned()
                        .unwrap_or_else(|| format!("player#{}", event.user_id));

                    if is_headshot {
                        self.highlights.push(Highlight {
                            tick: u32::from(tick),
                            kind: HighlightKind::Headshot,
                            killer: killer.clone(),
                            victim: victim.clone(),
                            weapon: event.weapon.clone(),
                        });
                    }
                    if is_airshot {
                        self.highlights.push(Highlight {
                            tick: u32::from(tick),
                            kind: HighlightKind::Airshot,
                            killer,
                            victim,
                            weapon: event.weapon.clone(),
                        });
                    }
                }
            }
        }
    }

    fn handle_string_entry(
        &mut self,
        table: &str,
        _index: usize,
        entry: &StringTableEntry,
        _parser_state: &ParserState,
    ) {
        if table == "userinfo" {
            if let Some(player_info) = entry.player_info.as_ref() {
                self.players.insert(player_info.user_id, player_info.name.clone());
            }
        }
    }

    fn into_output(self, _state: &ParserState) -> Self::Output {
        self.highlights
    }
}
```

**Step 2: Write unit tests at the bottom of `src/analyser.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_death_event(
        attacker: u16,
        user_id: u16,
        custom_kill: u16,
        rocket_jump: bool,
        weapon: &str,
    ) -> tf_demo_parser::demo::gameevent_gen::PlayerDeathEvent {
        tf_demo_parser::demo::gameevent_gen::PlayerDeathEvent {
            attacker,
            user_id,
            custom_kill,
            rocket_jump,
            weapon: weapon.to_string(),
            // fill required fields with defaults
            ..Default::default()
        }
    }

    #[test]
    fn test_headshot_detected() {
        let mut analyser = HighlightAnalyser::new();
        analyser.players.insert(1, "SniperAlex".to_string());
        analyser.players.insert(2, "ScoutBob".to_string());

        // Simulate a headshot event
        let event = make_death_event(1, 2, TF_CUSTOM_HEADSHOT, false, "tf_sniper_rifle");
        // Manually push as if handle_message was called
        analyser.highlights.push(Highlight {
            tick: 100,
            kind: HighlightKind::Headshot,
            killer: analyser.players[&1].clone(),
            victim: analyser.players[&2].clone(),
            weapon: event.weapon.clone(),
        });

        assert_eq!(analyser.highlights.len(), 1);
        assert!(matches!(analyser.highlights[0].kind, HighlightKind::Headshot));
        assert_eq!(analyser.highlights[0].killer, "SniperAlex");
        assert_eq!(analyser.highlights[0].victim, "ScoutBob");
    }

    #[test]
    fn test_airshot_detected() {
        let mut analyser = HighlightAnalyser::new();
        analyser.players.insert(3, "SoldierCarla".to_string());
        analyser.players.insert(4, "MedicDave".to_string());

        analyser.highlights.push(Highlight {
            tick: 200,
            kind: HighlightKind::Airshot,
            killer: "SoldierCarla".to_string(),
            victim: "MedicDave".to_string(),
            weapon: "tf_projectile_rocket".to_string(),
        });

        assert_eq!(analyser.highlights.len(), 1);
        assert!(matches!(analyser.highlights[0].kind, HighlightKind::Airshot));
    }

    #[test]
    fn test_non_highlight_not_detected() {
        let analyser = HighlightAnalyser::new();
        // No events added, highlights should be empty
        assert!(analyser.highlights.is_empty());
    }

    #[test]
    fn test_unknown_player_uses_fallback_name() {
        let analyser = HighlightAnalyser::new();
        let name = analyser
            .players
            .get(&99)
            .cloned()
            .unwrap_or_else(|| format!("player#{}", 99));
        assert_eq!(name, "player#99");
    }
}
```

**Step 3: Declare the module in `src/main.rs`**

Add to the top of `src/main.rs`:
```rust
mod analyser;
```

**Step 4: Run tests**

```bash
cargo test
```
Expected: tests pass (they're testing pure data logic, not parser integration)

**Step 5: Commit**

```bash
git add src/analyser.rs src/main.rs
git commit -m "feat: add HighlightAnalyser with headshot and airshot detection"
```

---

### Task 3: Implement the CLI entry point

**Files:**
- Modify: `src/main.rs`

**Step 1: Replace `src/main.rs` with the full CLI**

```rust
mod analyser;

use analyser::{HighlightAnalyser, HighlightKind};
use clap::Parser;
use std::fs;
use std::path::PathBuf;
use tf_demo_parser::demo::parser::DemoParser;
use tf_demo_parser::Demo;

/// TF2 Demo Insights — extract headshots and airshots from a .dem file
#[derive(Parser)]
#[command(version, about)]
struct Args {
    /// Path to the .dem file
    demo: PathBuf,
}

fn main() {
    let args = Args::parse();

    let data = match fs::read(&args.demo) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error reading {:?}: {}", args.demo, e);
            std::process::exit(1);
        }
    };

    let demo = Demo::new(&data);
    let parser = DemoParser::new_all_with_analyser(demo.get_stream(), HighlightAnalyser::new());
    let (_header, highlights) = match parser.parse() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error parsing demo: {}", e);
            std::process::exit(1);
        }
    };

    if highlights.is_empty() {
        println!("No highlights found.");
        return;
    }

    let name_width = highlights
        .iter()
        .map(|h| h.killer.len().max(h.victim.len()))
        .max()
        .unwrap_or(10);

    for h in &highlights {
        let kind_str = match h.kind {
            HighlightKind::Headshot => "HEADSHOT",
            HighlightKind::Airshot => "AIRSHOT ",
        };
        println!(
            "[tick {:>6}] {}  {:width$}  →  {:width$}  (weapon: {})",
            h.tick,
            kind_str,
            h.killer,
            h.victim,
            h.weapon,
            width = name_width,
        );
    }

    let headshots = highlights.iter().filter(|h| matches!(h.kind, HighlightKind::Headshot)).count();
    let airshots = highlights.iter().filter(|h| matches!(h.kind, HighlightKind::Airshot)).count();
    println!(
        "\n--- Summary: {} highlight{} ({} headshot{}, {} airshot{}) ---",
        highlights.len(),
        if highlights.len() == 1 { "" } else { "s" },
        headshots,
        if headshots == 1 { "" } else { "s" },
        airshots,
        if airshots == 1 { "" } else { "s" },
    );
}
```

**Step 2: Build and check for compile errors**

```bash
cargo build
```
Expected: compiles cleanly. Fix any type mismatches by checking the actual field names in `tf-demo-parser` docs — run `cargo doc --open` if needed.

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: add CLI entry point with formatted output"
```

---

### Task 4: Fix API mismatches and verify with real demo files

**Context:** The `tf-demo-parser` API may differ slightly from what's written above (field names, types). This task is about resolving compile errors and verifying the tool works end-to-end with the `.dem` files already in the project directory.

**Files in project directory:**
- `171ac3845d178f69794c05a7d17de346_match-20260110-1659-cp_gullywash_f9.dem`
- `f64b763761402165a230f21afd5c3c86_match-20251115-2033-cp_sunshine.dem`

**Step 1: Check the actual `PlayerDeathEvent` fields**

```bash
cargo doc --no-deps --open
```
Or browse to `https://docs.rs/tf-demo-parser/latest/tf_demo_parser/demo/gameevent_gen/struct.PlayerDeathEvent.html`

Key fields to verify:
- The field for the attacker's user_id (may be `attacker` as `u16` or `UserId`)
- The field for the victim's user_id (may be `user_id` as `u16` or `UserId`)
- The `rocket_jump` field name and type
- The `custom_kill` field type

**Step 2: Fix any type mismatches in `src/analyser.rs`**

Common patterns:
- `UserId` is a newtype wrapping `u16`. Use `.0` to get the inner value, or update the `HashMap` key type to `UserId`.
- `DemoTick` is a newtype. Use `u32::from(tick)` to convert.

**Step 3: Run against a real demo**

```bash
cargo run -- "171ac3845d178f69794c05a7d17de346_match-20260110-1659-cp_gullywash_f9.dem"
```
Expected: prints highlights to stdout with summary line. No crash. If 0 highlights found, try the other demo file.

**Step 4: Run tests**

```bash
cargo test
```
Expected: all tests pass.

**Step 5: Commit**

```bash
git add src/analyser.rs src/main.rs
git commit -m "fix: resolve tf-demo-parser API types, verify with real demo files"
```

---

### Task 5: Polish and final verification

**Step 1: Run both demo files**

```bash
cargo run -- "171ac3845d178f69794c05a7d17de346_match-20260110-1659-cp_gullywash_f9.dem"
cargo run -- "f64b763761402165a230f21afd5c3c86_match-20251115-2033-cp_sunshine.dem"
```
Verify output looks correct — player names resolve, ticks are reasonable numbers, event types make sense.

**Step 2: Test error handling**

```bash
cargo run -- nonexistent.dem
```
Expected: `Error reading "nonexistent.dem": ...` on stderr, exit code 1.

**Step 3: Run all tests one final time**

```bash
cargo test
```
Expected: all pass.

**Step 4: Final commit**

```bash
git add -A
git commit -m "chore: final polish and verification"
```
