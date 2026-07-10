// TODO: Remove this
#![allow(dead_code)]

use std::collections::HashMap;

use slotmap::{SlotMap, new_key_type};

// Tournament top-level

#[derive(Debug, Default)]
pub enum TournamentPhase {
    #[default]
    Registration,
    Pools,
    Bracket,
    Complete,
}

#[derive(Debug, PartialEq)]
pub enum TournamentError {
    InvalidPhase,
    NoParticipants,
    NonExistantParticipant,
    RaceIsFull,
    ParticipantAlreadyInRace,
}

#[derive(Debug, Default)]
pub struct Tournament {
    phase: TournamentPhase,
    participants: SlotMap<ParticipantId, Participant>,
}

impl Tournament {
    pub fn get_registration(&mut self) -> Option<Registration<'_>> {
        matches!(self.phase, TournamentPhase::Registration).then(|| Registration { tourney: self })
    }
}

// Participant info

new_key_type! { pub struct ParticipantId; }

#[derive(Debug)]
pub struct Participant {
    name: String,
    seed: u32,
}

impl Participant {
    fn new(name: &str, seed: u32) -> Self {
        Self {
            name: name.to_string(),
            seed,
        }
    }
}

// Races

const MAX_RACERS: usize = 8;

pub enum Placement {
    First,
    Second,
    Third,
    Fourth,
    Fifth,
    Sixth,
    Seventh,
    Eigth,
}

impl Placement {
    fn get_points(&self) -> u32 {
        todo!("Use a config to determine point distribution");
    }
}

#[derive(Default)]
pub struct Race {
    racers: HashMap<ParticipantId, Option<Placement>>,
}

impl Race {
    pub fn add_racer(&mut self, racer: ParticipantId) -> Result<(), TournamentError> {
        if self.racers.len() >= MAX_RACERS {
            return Err(TournamentError::RaceIsFull);
        }

        if self.racers.contains_key(&racer) {
            return Err(TournamentError::ParticipantAlreadyInRace);
        }

        self.racers.insert(racer, None);
        Ok(())
    }

    pub fn remove_racer(&mut self, racer: ParticipantId) -> Result<(), TournamentError> {
        if self.racers.remove(&racer).is_none() {
            Err(TournamentError::NonExistantParticipant)
        } else {
            Ok(())
        }
    }

    pub fn set_placement(
        &mut self,
        racer: ParticipantId,
        place: Placement,
    ) -> Result<(), TournamentError> {
        if let Some(racer_place) = self.racers.get_mut(&racer) {
            *racer_place = Some(place);
            Ok(())
        } else {
            Err(TournamentError::NonExistantParticipant)
        }
    }

    pub fn is_complete(&self) -> bool {
        for racer_place in self.racers.values() {
            if racer_place.is_none() {
                return false;
            }
        }

        true
    }
}

// Registration Phase

pub struct Registration<'a> {
    tourney: &'a mut Tournament,
}

impl Registration<'_> {
    pub fn add_participant(&mut self, name: &str, seed: u32) -> ParticipantId {
        self.tourney
            .participants
            .insert(Participant::new(name, seed))
    }

    pub fn remove_participant(&mut self, id: ParticipantId) -> Result<(), TournamentError> {
        self.tourney
            .participants
            .remove(id)
            .map_or(Err(TournamentError::NonExistantParticipant), |_| Ok(()))
    }

    pub fn start(self) -> Result<(), TournamentError> {
        if self.tourney.participants.is_empty() {
            Err(TournamentError::NoParticipants)
        } else {
            self.tourney.phase = TournamentPhase::Pools;
            Ok(())
        }
    }
}
