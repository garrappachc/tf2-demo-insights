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
            eprintln!("Error reading {:?}: {}", args.demo, e);
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

    let name_width = highlights
        .iter()
        .map(|h| h.killer.len().max(h.victim.len()))
        .max()
        .unwrap_or(0);

    for h in &highlights {
        let kind_str = match h.kind {
            HighlightKind::Headshot => "HEADSHOT",
            HighlightKind::Airshot => "AIRSHOT ",
        };
        println!(
            "[tick {:>6}] {}  {:width$}  \u{2192}  {:width$}  (weapon: {})",
            h.tick,
            kind_str,
            h.killer,
            h.victim,
            h.weapon,
            width = name_width,
        );
    }

    let headshots = highlights
        .iter()
        .filter(|h| h.kind == HighlightKind::Headshot)
        .count();
    let airshots = highlights
        .iter()
        .filter(|h| h.kind == HighlightKind::Airshot)
        .count();
    let total = highlights.len();

    let headshot_str = if headshots == 1 {
        "1 headshot".to_string()
    } else {
        format!("{} headshots", headshots)
    };
    let airshot_str = if airshots == 1 {
        "1 airshot".to_string()
    } else {
        format!("{} airshots", airshots)
    };

    println!(
        "\n--- Summary: {} highlight{} ({}, {}) ---",
        total,
        if total == 1 { "" } else { "s" },
        headshot_str,
        airshot_str,
    );
}
