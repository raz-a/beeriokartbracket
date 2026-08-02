use std::io::{self, Write};
use std::num::NonZero;

use beeriokartbracket::{Config, Tournament, TournamentView};

fn main() {
    let mut tournament = Tournament::default();

    loop {
        let view = tournament.view();
        render(&view);

        match view {
            TournamentView::Registration(reg) => {
                println!(
                    "\ncommands: add <name> | remove <n> | set <pool_rounds> <bracket_size> | start | quit"
                );

                let Some(line) = prompt("> ") else { break };
                let line = line.trim();

                if line.is_empty() {
                    continue;
                } else if let Some(name) = line.strip_prefix("add ") {
                    if let Err(e) = tournament.add_participant(name.trim()) {
                        println!("error: {e:?}");
                    }
                } else if let Some(rest) = line.strip_prefix("remove ") {
                    match rest.trim().parse::<usize>() {
                        Ok(n) if (1..=reg.participants.len()).contains(&n) => {
                            let id = reg.participants[n - 1].id;
                            if let Err(e) = tournament.remove_participant(id) {
                                println!("error: {e:?}");
                            }
                        }
                        _ => println!("usage: remove <n> (1..={})", reg.participants.len()),
                    }
                } else if let Some(rest) = line.strip_prefix("set ") {
                    match parse_config(rest) {
                        Some(config) => {
                            if let Err(e) = tournament.set_config(config) {
                                println!("error: {e:?}");
                            }
                        }
                        None => println!("usage: set <pool_rounds> <bracket_size> (both non-zero)"),
                    }
                } else if line == "start" {
                    match tournament.next_phase() {
                        Ok(()) => println!("Starting pools..."),
                        Err(e) => println!("cannot start: {e:?}"),
                    }
                } else if line == "quit" {
                    break;
                } else {
                    println!("unknown command");
                }
            }
            _ => {
                println!("\n(this phase has no interactions yet)");
                break;
            }
        }
    }
}

/// Reads a line of input, returning `None` on EOF.
fn prompt(msg: &str) -> Option<String> {
    print!("{msg}");
    io::stdout().flush().ok()?;

    let mut line = String::new();
    match io::stdin().read_line(&mut line) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(line),
    }
}

/// Parses "<pool_rounds> <bracket_size>" into a Config; both must be non-zero.
fn parse_config(input: &str) -> Option<Config> {
    let mut parts = input.split_whitespace();
    let pool_rounds = NonZero::new(parts.next()?.parse::<usize>().ok()?)?;
    let bracket_size = NonZero::new(parts.next()?.parse::<usize>().ok()?)?;
    Some(Config {
        pool_rounds,
        bracket_size,
    })
}

fn render(view: &TournamentView) {
    match view {
        TournamentView::Registration(reg) => {
            print_header("Registration");
            if reg.participants.is_empty() {
                println!("  participants: (none yet)");
            } else {
                println!("  participants ({}):", reg.participants.len());
                for (i, participant) in reg.participants.iter().enumerate() {
                    println!("    {:>2}. {}", i + 1, participant.name);
                }
            }
            println!(
                "  config: {} pool rounds, top {} to bracket",
                reg.config.pool_rounds, reg.config.bracket_size
            );
        }
        TournamentView::Pools(pool) => {
            print_header("Pools");
            println!("  round {} / {}", pool.current_round, pool.max_rounds);
            match &pool.current_race {
                Some(race) => {
                    println!("  current race [{:?}]:", race.ruleset);
                    for (racer, placement) in &race.racers {
                        match placement {
                            Some(place) => println!("    {} - {:?}", racer.name, place),
                            None => println!("    {}", racer.name),
                        }
                    }
                }
                None => println!("  current race: (none yet - advance the pool)"),
            }
            println!(
                "  this round: {} waiting, {} done",
                pool.remaining_racers_in_round.len(),
                pool.completed_racers_in_round.len()
            );
            println!("  completed races: {}", pool.completed_races.len());
        }
        TournamentView::Bracket => print_header("Bracket"),
        TournamentView::Gauntlet => print_header("Gauntlet"),
        TournamentView::Complete => print_header("Complete"),
    }
}

fn print_header(title: &str) {
    println!("\n=== {title} ===");
}
