use serde::{Deserialize, Serialize};

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
