use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use atomicwrites::{AllowOverwrite, AtomicFile};
use beeriokartbracket::{
    PUBLIC_SNAPSHOT_SCHEMA_VERSION, PublicPlacement, PublicRuleset, PublicTournamentPhase,
    PublicTournamentSnapshot,
};
use reqwest::blocking::Client;
use serde::Deserialize;

const DEFAULT_SNAPSHOT_URL: &str = "https://beeriokartbracket-api.beeriokart.workers.dev/snapshot";
const DEFAULT_POLL_INTERVAL_MS: u64 = 2_000;
const RACER_FILE_COUNT: usize = 8;

#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Config {
    snapshot_url: String,
    output_directory: PathBuf,
    poll_interval_ms: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            snapshot_url: DEFAULT_SNAPSHOT_URL.to_owned(),
            output_directory: PathBuf::from("obs-output"),
            poll_interval_ms: DEFAULT_POLL_INTERVAL_MS,
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("OBS companion failed: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let executable_directory = env::current_exe()?
        .parent()
        .ok_or("executable has no parent directory")?
        .to_owned();
    let config = load_config(&executable_directory)?;
    let output_directory = if config.output_directory.is_absolute() {
        config.output_directory.clone()
    } else {
        executable_directory.join(&config.output_directory)
    };
    fs::create_dir_all(&output_directory)?;

    let client = Client::builder().timeout(Duration::from_secs(10)).build()?;
    let poll_interval = Duration::from_millis(config.poll_interval_ms.max(250));
    let mut last_revision = None;

    println!("Reading {}", config.snapshot_url);
    println!("Writing OBS text sources to {}", output_directory.display());

    loop {
        match fetch_snapshot(&client, &config.snapshot_url) {
            Ok(snapshot) if last_revision != Some(snapshot.revision) => {
                write_outputs(&output_directory, &render_outputs(&snapshot))?;
                last_revision = Some(snapshot.revision);
                println!("Published revision {}", snapshot.revision);
            }
            Ok(_) => {}
            Err(error) => eprintln!("Snapshot refresh failed: {error}"),
        }
        thread::sleep(poll_interval);
    }
}

fn load_config(executable_directory: &Path) -> Result<Config, Box<dyn Error>> {
    let path = executable_directory.join("obs-companion.json");
    if !path.exists() {
        return Ok(Config::default());
    }
    let config: Config = serde_json::from_slice(&fs::read(&path)?)?;
    if config.snapshot_url.trim().is_empty() {
        return Err("snapshot_url cannot be empty".into());
    }
    Ok(config)
}

fn fetch_snapshot(
    client: &Client,
    snapshot_url: &str,
) -> Result<PublicTournamentSnapshot, Box<dyn Error>> {
    let response = client.get(snapshot_url).send()?.error_for_status()?;
    let snapshot: PublicTournamentSnapshot = response.json()?;
    if snapshot.schema_version != PUBLIC_SNAPSHOT_SCHEMA_VERSION {
        return Err(format!(
            "unsupported snapshot schema {}; expected {}",
            snapshot.schema_version, PUBLIC_SNAPSHOT_SCHEMA_VERSION
        )
        .into());
    }
    Ok(snapshot)
}

fn render_outputs(snapshot: &PublicTournamentSnapshot) -> BTreeMap<String, String> {
    let mut outputs = BTreeMap::from([
        (
            "tournament_name.txt".to_owned(),
            snapshot.tournament_name.clone(),
        ),
        (
            "phase.txt".to_owned(),
            phase_label(&snapshot.tournament).to_owned(),
        ),
        ("heat.txt".to_owned(), String::new()),
        ("ruleset.txt".to_owned(), String::new()),
        ("active_racers.txt".to_owned(), String::new()),
    ]);

    for slot in 1..=RACER_FILE_COUNT {
        outputs.insert(format!("racer_{slot}.txt"), String::new());
        outputs.insert(format!("placement_{slot}.txt"), String::new());
    }

    if let Some(race) = &snapshot.active_race {
        outputs.insert("heat.txt".to_owned(), race.label.clone());
        outputs.insert(
            "ruleset.txt".to_owned(),
            ruleset_label(race.ruleset).to_owned(),
        );
        outputs.insert(
            "active_racers.txt".to_owned(),
            race.racers
                .iter()
                .map(|racer| racer.racer_name.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        );
        for racer in &race.racers {
            if (1..=RACER_FILE_COUNT).contains(&racer.slot) {
                outputs.insert(
                    format!("racer_{}.txt", racer.slot),
                    racer.racer_name.clone(),
                );
                outputs.insert(
                    format!("placement_{}.txt", racer.slot),
                    racer.placement.map(placement_label).unwrap_or_default(),
                );
            }
        }
    }

    if let PublicTournamentPhase::Complete(results) = &snapshot.tournament {
        for (index, result) in results.iter().take(RACER_FILE_COUNT).enumerate() {
            outputs.insert(
                format!("racer_{}.txt", index + 1),
                result.racer_name.clone(),
            );
            outputs.insert(
                format!("placement_{}.txt", index + 1),
                placement_label(result.placement),
            );
        }
    }

    outputs
}

fn phase_label(phase: &PublicTournamentPhase) -> &'static str {
    match phase {
        PublicTournamentPhase::Registration(_) => "Registration",
        PublicTournamentPhase::Pools(_) => "Pools",
        PublicTournamentPhase::Bracket(_) => "Bracket",
        PublicTournamentPhase::Gauntlet(_) => "Grand Finals Gauntlet",
        PublicTournamentPhase::Complete(_) => "Complete",
    }
}

fn ruleset_label(ruleset: PublicRuleset) -> &'static str {
    match ruleset {
        PublicRuleset::Vanilla => "Vanilla",
        PublicRuleset::Beerio => "Beerio Kart",
    }
}

fn placement_label(placement: PublicPlacement) -> String {
    match placement {
        PublicPlacement::Placed { position } => ordinal(position),
        PublicPlacement::Disqualified => "DQ".to_owned(),
    }
}

fn ordinal(position: u8) -> String {
    let suffix = match position % 100 {
        11..=13 => "th",
        _ => match position % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    };
    format!("{position}{suffix}")
}

fn write_outputs(directory: &Path, outputs: &BTreeMap<String, String>) -> io::Result<()> {
    for (name, content) in outputs {
        let path = directory.join(name);
        if fs::read_to_string(&path).is_ok_and(|current| current == *content) {
            continue;
        }
        AtomicFile::new(path, AllowOverwrite).write(|file| file.write_all(content.as_bytes()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use beeriokartbracket::{
        PublicActiveRace, PublicRacerSlot, PublicRegistration, PublicTournamentResult,
    };

    fn snapshot(
        active_race: Option<PublicActiveRace>,
        tournament: PublicTournamentPhase,
    ) -> PublicTournamentSnapshot {
        PublicTournamentSnapshot {
            schema_version: PUBLIC_SNAPSHOT_SCHEMA_VERSION,
            revision: 1,
            published_at_unix_ms: 1,
            tournament_name: "Beerio Kart 2026".to_owned(),
            active_race,
            tournament,
        }
    }

    #[test]
    fn renders_active_race_text_sources_and_clears_unused_slots() {
        let outputs = render_outputs(&snapshot(
            Some(PublicActiveRace {
                label: "Winners Round 1 Heat 1 Race 2".to_owned(),
                ruleset: PublicRuleset::Beerio,
                racers: vec![
                    PublicRacerSlot {
                        slot: 1,
                        racer_name: "Mario".to_owned(),
                        placement: Some(PublicPlacement::Placed { position: 1 }),
                    },
                    PublicRacerSlot {
                        slot: 2,
                        racer_name: "Luigi".to_owned(),
                        placement: Some(PublicPlacement::Disqualified),
                    },
                ],
            }),
            PublicTournamentPhase::Registration(PublicRegistration {
                participants: Vec::new(),
            }),
        ));

        assert_eq!(outputs["heat.txt"], "Winners Round 1 Heat 1 Race 2");
        assert_eq!(outputs["ruleset.txt"], "Beerio Kart");
        assert_eq!(outputs["active_racers.txt"], "Mario\nLuigi");
        assert_eq!(outputs["racer_1.txt"], "Mario");
        assert_eq!(outputs["placement_1.txt"], "1st");
        assert_eq!(outputs["placement_2.txt"], "DQ");
        assert!(outputs["racer_8.txt"].is_empty());
    }

    #[test]
    fn renders_complete_results_into_racer_slots() {
        let outputs = render_outputs(&snapshot(
            None,
            PublicTournamentPhase::Complete(vec![PublicTournamentResult {
                placement: PublicPlacement::Placed { position: 1 },
                racer_name: "Champion".to_owned(),
            }]),
        ));

        assert_eq!(outputs["phase.txt"], "Complete");
        assert_eq!(outputs["racer_1.txt"], "Champion");
        assert_eq!(outputs["placement_1.txt"], "1st");
        assert!(outputs["heat.txt"].is_empty());
    }

    #[test]
    fn ordinal_handles_teens() {
        assert_eq!(ordinal(1), "1st");
        assert_eq!(ordinal(12), "12th");
        assert_eq!(ordinal(23), "23rd");
    }
}
