use std::error::Error;
use std::fmt;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::Tournament;

const SCHEMA_VERSION: u32 = 1;

pub(crate) trait PersistedState<Context: ?Sized = ()>: Serialize + DeserializeOwned {
    fn validate_loaded(&self, context: &Context) -> Result<(), &'static str>;
}

#[derive(Serialize, Deserialize)]
struct TournamentDocument<T> {
    schema_version: u32,
    name: String,
    tournament: T,
}

#[derive(Debug)]
pub enum PersistenceError {
    Json(serde_json::Error),
    UnsupportedSchema(u32),
    MissingName,
    InvalidTournament(&'static str),
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid tournament JSON: {error}"),
            Self::UnsupportedSchema(version) => {
                write!(formatter, "unsupported tournament schema version {version}")
            }
            Self::MissingName => formatter.write_str("tournament name cannot be empty"),
            Self::InvalidTournament(message) => {
                write!(formatter, "invalid tournament state: {message}")
            }
        }
    }
}

impl Error for PersistenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::UnsupportedSchema(_) | Self::MissingName | Self::InvalidTournament(_) => None,
        }
    }
}

impl From<serde_json::Error> for PersistenceError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub fn serialize_tournament(
    name: &str,
    tournament: &Tournament,
) -> Result<String, PersistenceError> {
    if name.trim().is_empty() {
        return Err(PersistenceError::MissingName);
    }

    Ok(serde_json::to_string_pretty(&TournamentDocument {
        schema_version: SCHEMA_VERSION,
        name: name.trim().to_owned(),
        tournament,
    })?)
}

pub fn deserialize_tournament(json: &str) -> Result<(String, Tournament), PersistenceError> {
    let document: TournamentDocument<Tournament> = serde_json::from_str(json)?;
    if document.schema_version != SCHEMA_VERSION {
        return Err(PersistenceError::UnsupportedSchema(document.schema_version));
    }
    if document.name.trim().is_empty() {
        return Err(PersistenceError::MissingName);
    }
    document
        .tournament
        .validate_loaded(&())
        .map_err(PersistenceError::InvalidTournament)?;

    Ok((document.name, document.tournament))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Placement, TournamentView};

    #[test]
    fn registration_round_trips() {
        let mut tournament = Tournament::default();
        tournament.add_participant("Mario").unwrap();
        tournament.add_participant("Luigi").unwrap();

        let json = serialize_tournament("Test Cup", &tournament).unwrap();
        let (name, loaded) = deserialize_tournament(&json).unwrap();

        assert_eq!(name, "Test Cup");
        let TournamentView::Registration(registration) = loaded.view() else {
            panic!("loaded tournament should remain in registration");
        };
        let names: Vec<_> = registration
            .participants
            .iter()
            .map(|participant| participant.name.as_str())
            .collect();
        assert_eq!(names, ["Mario", "Luigi"]);
    }

    #[test]
    fn pool_rng_state_round_trips() {
        let mut tournament = Tournament::default();
        for number in 1..=16 {
            tournament
                .add_participant(&format!("Player {number}"))
                .unwrap();
        }
        tournament.next_phase().unwrap();
        tournament.advance_pools().unwrap();

        let json = serialize_tournament("Test Cup", &tournament).unwrap();
        let (_, mut loaded) = deserialize_tournament(&json).unwrap();

        complete_active_pool_race(&mut tournament);
        complete_active_pool_race(&mut loaded);

        assert_eq!(active_pool_racers(&tournament), active_pool_racers(&loaded));
    }

    #[test]
    fn rejects_unknown_schema_version() {
        let json = serialize_tournament("Test Cup", &Tournament::default()).unwrap();
        let mut document: serde_json::Value = serde_json::from_str(&json).unwrap();
        document["schema_version"] = 2.into();

        assert!(matches!(
            deserialize_tournament(&document.to_string()),
            Err(PersistenceError::UnsupportedSchema(2))
        ));
    }

    fn complete_active_pool_race(tournament: &mut Tournament) {
        let TournamentView::Pools((pool, _)) = tournament.view() else {
            panic!("tournament should be in pools");
        };
        let results = pool
            .current_race
            .unwrap()
            .racers
            .iter()
            .enumerate()
            .map(|(index, (participant, _))| {
                (
                    participant.id,
                    Some(Placement::new((index + 1) as u8).unwrap()),
                )
            })
            .collect();
        tournament.update_active_race(results).unwrap();
        tournament.advance_pools().unwrap();
    }

    fn active_pool_racers(tournament: &Tournament) -> Vec<crate::ParticipantId> {
        let TournamentView::Pools((pool, _)) = tournament.view() else {
            panic!("tournament should be in pools");
        };
        pool.current_race
            .unwrap()
            .racers
            .iter()
            .map(|(participant, _)| participant.id)
            .collect()
    }
}
