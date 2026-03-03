mod analyser;

use std::path::PathBuf;
use std::process;

use clap::Parser;
use tf_demo_parser::{Demo, DemoParser};

use analyser::{HighlightAnalyser, HighlightKind};

#[derive(Parser)]
#[command(name = "tf2-demo-insights", about = "Analyse TF2 demo files for highlights")]
struct Args {
    /// Path to the .dem file to analyse
    demo: PathBuf,
}

fn main() {
    let args = Args::parse();

    let file = match std::fs::read(&args.demo) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Error reading {}: {}", args.demo.display(), e);
            process::exit(1);
        }
    };

    let demo = Demo::new(&file);
    let parser = DemoParser::new_all_with_analyser(demo.get_stream(), HighlightAnalyser::new());
    let (_header, highlights) = match parser.parse() {
        Ok(result) => result,
        Err(e) => {
            eprintln!("Error parsing demo: {}", e);
            process::exit(1);
        }
    };

    if highlights.is_empty() {
        println!("No highlights found.");
        return;
    }

    // Use char count (not byte length) so Unicode names (e.g. "mały") align correctly.
    let name_width = highlights
        .iter()
        .map(|h| h.killer.chars().count().max(h.victim.chars().count()))
        .max()
        .unwrap_or(10);

    for h in &highlights {
        let kind_str = match (&h.kind, h.lethal) {
            (HighlightKind::Airshot, true)  => "AIRSHOT*",
            (HighlightKind::Airshot, false) => "AIRSHOT ",
            _                               => "HEADSHOT",
        };

        // Height column: "+NNN.Nu" (7 chars + unit) or 8 spaces to match
        let height_str = match h.height {
            Some(ht) => format!("{:+7.1}u", ht),
            None      => "        ".to_string(), // 8 spaces
        };

        let trailing = match (&h.kind, h.lethal) {
            (HighlightKind::Airshot, false) => {
                format!("({} dmg)", h.damage.unwrap_or(0))
            }
            _ => {
                format!("(weapon: {})", h.weapon)
            }
        };

        println!(
            "[tick {:>6}] {}  {:width$}  \u{2192}  {:width$}  {}  {}",
            h.tick,
            kind_str,
            h.killer,
            h.victim,
            height_str,
            trailing,
            width = name_width,
        );
    }

    let headshots = highlights.iter().filter(|h| matches!(h.kind, HighlightKind::Headshot)).count();
    let total_airshots = highlights.iter().filter(|h| matches!(h.kind, HighlightKind::Airshot)).count();
    let lethal_airshots = highlights.iter().filter(|h| matches!(h.kind, HighlightKind::Airshot) && h.lethal).count();

    println!(
        "\n--- Summary: {} headshot{} | {} airshot{} ({} lethal) ---",
        headshots,
        if headshots == 1 { "" } else { "s" },
        total_airshots,
        if total_airshots == 1 { "" } else { "s" },
        lethal_airshots,
    );
}
