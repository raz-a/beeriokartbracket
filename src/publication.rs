use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::bracket::{BracketRoundView, BracketSetId, BracketView};
use crate::gauntlet::GauntletView;
use crate::participant::ParticipantId;
use crate::pool::{PoolResultView, PoolView};
use crate::race::{Placement, RaceRuleset, RaceView};
use crate::view::{RegistrationView, TournamentResultView, TournamentView};

pub const PUBLIC_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicTournamentSnapshot {
    pub schema_version: u32,
    pub revision: u64,
    pub published_at_unix_ms: u64,
    pub tournament_name: String,
    pub active_race: Option<PublicActiveRace>,
    pub tournament: PublicTournamentPhase,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "phase", content = "state", rename_all = "snake_case")]
pub enum PublicTournamentPhase {
    Registration(PublicRegistration),
    Pools(PublicPools),
    Bracket(PublicBracket),
    Gauntlet(PublicGauntlet),
    Complete(Vec<PublicTournamentResult>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRegistration {
    pub participants: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicPools {
    pub current_round: usize,
    pub total_rounds: usize,
    pub standings: Vec<PublicPoolStanding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicPoolStanding {
    pub rank: usize,
    pub racer_name: String,
    pub points: usize,
    pub races_completed: usize,
    pub status: PublicStandingStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicStandingStatus {
    Racing,
    Advanced,
    Eliminated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicBracket {
    pub winners: Vec<PublicBracketRound>,
    pub losers: Vec<PublicBracketRound>,
    pub winners_finalists: Vec<String>,
    pub losers_finalists: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicBracketRound {
    pub label: String,
    pub heats: Vec<PublicBracketHeat>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicBracketHeat {
    pub id: String,
    pub status: PublicHeatStatus,
    pub racers: Vec<String>,
    pub races: Vec<PublicRaceResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicHeatStatus {
    Waiting,
    Ready,
    Active,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicGauntlet {
    pub racers: Vec<PublicGauntletRacer>,
    pub completed_races: Vec<PublicRaceResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicGauntletRacer {
    pub racer_name: String,
    pub lives: usize,
    pub placement: Option<PublicPlacement>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicActiveRace {
    pub label: String,
    pub ruleset: PublicRuleset,
    pub racers: Vec<PublicRacerSlot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRaceResult {
    pub label: String,
    pub ruleset: PublicRuleset,
    pub racers: Vec<PublicRacerSlot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicRacerSlot {
    pub slot: usize,
    pub racer_name: String,
    pub placement: Option<PublicPlacement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicRuleset {
    Vanilla,
    Beerio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum PublicPlacement {
    Placed { position: u8 },
    Disqualified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicTournamentResult {
    pub placement: PublicPlacement,
    pub racer_name: String,
}

impl PublicTournamentSnapshot {
    pub(crate) fn from_view(
        tournament_name: &str,
        revision: u64,
        published_at_unix_ms: u64,
        view: TournamentView,
    ) -> Self {
        let (active_race, tournament) = match view {
            TournamentView::Registration(registration) => (
                None,
                PublicTournamentPhase::Registration(public_registration(registration)),
            ),
            TournamentView::Pools((pool, results)) => {
                let active_race = pool.current_race.as_ref().map(|race| {
                    public_active_race(format!("Pools Round {}", pool.current_round + 1), race)
                });
                (
                    active_race,
                    PublicTournamentPhase::Pools(public_pools(&pool, results.as_ref())),
                )
            }
            TournamentView::Bracket(bracket) => (
                active_bracket_race(&bracket),
                PublicTournamentPhase::Bracket(public_bracket(&bracket)),
            ),
            TournamentView::Gauntlet(gauntlet) => (
                active_gauntlet_race(&gauntlet),
                PublicTournamentPhase::Gauntlet(public_gauntlet(&gauntlet)),
            ),
            TournamentView::Complete(results) => (
                None,
                PublicTournamentPhase::Complete(
                    results.iter().map(public_tournament_result).collect(),
                ),
            ),
        };

        Self {
            schema_version: PUBLIC_SNAPSHOT_SCHEMA_VERSION,
            revision,
            published_at_unix_ms,
            tournament_name: tournament_name.to_owned(),
            active_race,
            tournament,
        }
    }
}

fn public_registration(view: RegistrationView) -> PublicRegistration {
    PublicRegistration {
        participants: view
            .participants
            .into_iter()
            .map(|participant| participant.name)
            .collect(),
    }
}

fn public_pools(view: &PoolView, results: Option<&PoolResultView>) -> PublicPools {
    let mut totals: HashMap<ParticipantId, (String, usize, usize)> = HashMap::new();
    let current_roster = view
        .remaining_racers_in_round
        .iter()
        .chain(&view.completed_racers_in_round)
        .chain(
            view.current_race
                .iter()
                .flat_map(|race| race.racers.iter().map(|(participant, _)| participant)),
        );

    for participant in current_roster {
        totals
            .entry(participant.id)
            .or_insert_with(|| (participant.name.clone(), 0, 0));
    }
    for (_, race, _) in &view.completed_races {
        for (participant, placement) in &race.racers {
            let entry = totals
                .entry(participant.id)
                .or_insert_with(|| (participant.name.clone(), 0, 0));
            entry.1 += placement.map_or(0, |placement| placement.points());
            entry.2 += 1;
        }
    }

    let advanced: HashSet<_> = results
        .into_iter()
        .flat_map(|results| {
            results
                .advanced
                .iter()
                .map(|(participant, _)| participant.id)
        })
        .collect();
    let eliminated: HashSet<_> = results
        .into_iter()
        .flat_map(|results| {
            results
                .eliminated
                .iter()
                .map(|(participant, _)| participant.id)
        })
        .collect();

    let mut standings: Vec<_> = totals
        .into_iter()
        .map(|(id, (racer_name, points, races_completed))| {
            (id, racer_name, points, races_completed)
        })
        .collect();
    standings.sort_by(|left, right| right.2.cmp(&left.2).then_with(|| left.1.cmp(&right.1)));
    let standings = standings
        .into_iter()
        .enumerate()
        .map(
            |(index, (id, racer_name, points, races_completed))| PublicPoolStanding {
                rank: index + 1,
                racer_name,
                points,
                races_completed,
                status: if advanced.contains(&id) {
                    PublicStandingStatus::Advanced
                } else if eliminated.contains(&id) {
                    PublicStandingStatus::Eliminated
                } else {
                    PublicStandingStatus::Racing
                },
            },
        )
        .collect();

    PublicPools {
        current_round: (view.current_round + 1).min(view.max_rounds),
        total_rounds: view.max_rounds,
        standings,
    }
}

fn public_bracket(view: &BracketView) -> PublicBracket {
    PublicBracket {
        winners: public_bracket_rounds(&view.winners, "winners", view.active_set),
        losers: public_bracket_rounds(&view.losers, "losers", view.active_set),
        winners_finalists: view
            .winners_finalists
            .iter()
            .map(|participant| participant.name.clone())
            .collect(),
        losers_finalists: view
            .losers_finalists
            .iter()
            .map(|participant| participant.name.clone())
            .collect(),
    }
}

fn public_bracket_rounds(
    rounds: &[BracketRoundView],
    side: &str,
    active_set: Option<BracketSetId>,
) -> Vec<PublicBracketRound> {
    rounds
        .iter()
        .enumerate()
        .map(|(round_index, round)| PublicBracketRound {
            label: match (side, round.from_wb_round) {
                ("losers", Some(winners_round)) => format!(
                    "Losers Round {} (from Winners Round {})",
                    round_index + 1,
                    winners_round + 1
                ),
                _ => format!("{} Round {}", title_case(side), round_index + 1),
            },
            heats: round
                .sets
                .iter()
                .enumerate()
                .map(|(heat_index, (id, heat))| {
                    let is_active = active_set == Some(*id);
                    let status = if is_active {
                        PublicHeatStatus::Active
                    } else if heat.is_ready && heat.current_race_index >= heat.races.len() {
                        PublicHeatStatus::Complete
                    } else if heat.is_ready {
                        PublicHeatStatus::Ready
                    } else {
                        PublicHeatStatus::Waiting
                    };
                    PublicBracketHeat {
                        id: format!("{side}-{}-{}", round_index + 1, heat_index + 1),
                        status,
                        racers: heat
                            .racers
                            .iter()
                            .map(|participant| participant.name.clone())
                            .collect(),
                        races: heat
                            .races
                            .iter()
                            .enumerate()
                            .filter(|(_, race)| race_is_complete(race))
                            .map(|(race_index, race)| {
                                public_race_result(format!("Race {}", race_index + 1), race)
                            })
                            .collect(),
                    }
                })
                .collect(),
        })
        .collect()
}

fn active_bracket_race(view: &BracketView) -> Option<PublicActiveRace> {
    let active_set = view.active_set?;
    for (side, rounds) in [("Winners", &view.winners), ("Losers", &view.losers)] {
        for (round_index, round) in rounds.iter().enumerate() {
            for (heat_index, (id, heat)) in round.sets.iter().enumerate() {
                if *id == active_set {
                    let race = heat.races.get(heat.current_race_index)?;
                    return Some(public_active_race(
                        format!(
                            "{side} Round {} Heat {} Race {}",
                            round_index + 1,
                            heat_index + 1,
                            heat.current_race_index + 1
                        ),
                        race,
                    ));
                }
            }
        }
    }
    None
}

fn public_gauntlet(view: &GauntletView) -> PublicGauntlet {
    PublicGauntlet {
        racers: view
            .racers
            .iter()
            .map(|racer| PublicGauntletRacer {
                racer_name: racer.participant.name.clone(),
                lives: racer.lives,
                placement: racer.placement.map(public_placement),
            })
            .collect(),
        completed_races: view
            .races
            .iter()
            .enumerate()
            .filter(|(_, race)| race_is_complete(race))
            .map(|(index, race)| public_race_result(format!("Gauntlet Race {}", index + 1), race))
            .collect(),
    }
}

fn active_gauntlet_race(view: &GauntletView) -> Option<PublicActiveRace> {
    view.races
        .iter()
        .enumerate()
        .find(|(_, race)| !race_is_complete(race))
        .map(|(index, race)| public_active_race(format!("Gauntlet Race {}", index + 1), race))
}

fn public_active_race(label: String, race: &RaceView) -> PublicActiveRace {
    PublicActiveRace {
        label,
        ruleset: public_ruleset(race.ruleset),
        racers: public_racer_slots(race),
    }
}

fn public_race_result(label: String, race: &RaceView) -> PublicRaceResult {
    PublicRaceResult {
        label,
        ruleset: public_ruleset(race.ruleset),
        racers: public_racer_slots(race),
    }
}

fn public_racer_slots(race: &RaceView) -> Vec<PublicRacerSlot> {
    race.racers
        .iter()
        .enumerate()
        .map(|(index, (participant, placement))| PublicRacerSlot {
            slot: index + 1,
            racer_name: participant.name.clone(),
            placement: placement.map(public_placement),
        })
        .collect()
}

fn public_tournament_result(result: &TournamentResultView) -> PublicTournamentResult {
    PublicTournamentResult {
        placement: public_placement(result.placement),
        racer_name: result.participant.name.clone(),
    }
}

fn public_placement(placement: Placement) -> PublicPlacement {
    if placement.is_disqualified() {
        PublicPlacement::Disqualified
    } else {
        PublicPlacement::Placed {
            position: placement.placement(),
        }
    }
}

fn public_ruleset(ruleset: RaceRuleset) -> PublicRuleset {
    match ruleset {
        RaceRuleset::Vanilla => PublicRuleset::Vanilla,
        RaceRuleset::Beerio => PublicRuleset::Beerio,
    }
}

fn race_is_complete(race: &RaceView) -> bool {
    !race.racers.is_empty() && race.racers.iter().all(|(_, placement)| placement.is_some())
}

fn title_case(value: &str) -> String {
    let mut characters = value.chars();
    characters
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + characters.as_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_json_has_stable_phase_and_placement_tags() {
        let snapshot = PublicTournamentSnapshot {
            schema_version: PUBLIC_SNAPSHOT_SCHEMA_VERSION,
            revision: 42,
            published_at_unix_ms: 1_800_000_000_000,
            tournament_name: "Invitational".to_owned(),
            active_race: Some(PublicActiveRace {
                label: "Grand Finals Race 3".to_owned(),
                ruleset: PublicRuleset::Beerio,
                racers: vec![PublicRacerSlot {
                    slot: 1,
                    racer_name: "Mario".to_owned(),
                    placement: Some(PublicPlacement::Placed { position: 1 }),
                }],
            }),
            tournament: PublicTournamentPhase::Complete(vec![PublicTournamentResult {
                placement: PublicPlacement::Placed { position: 1 },
                racer_name: "Mario".to_owned(),
            }]),
        };

        let json = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["tournament"]["phase"], "complete");
        assert_eq!(
            json["active_race"]["racers"][0]["placement"]["result"],
            "placed"
        );
        assert_eq!(json["tournament"]["state"][0]["placement"]["position"], 1);

        let decoded: PublicTournamentSnapshot = serde_json::from_value(json).unwrap();
        assert_eq!(decoded, snapshot);
    }
}
