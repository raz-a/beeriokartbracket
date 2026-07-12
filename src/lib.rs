// TODO: Remove this
#![allow(dead_code)]

use slotmap::{SlotMap, new_key_type};

// Tournament top-level

#[derive(Debug, Default)]
pub enum TournamentPhase {
    #[default]
    Registration,
    Pools,
    Bracket,
    Gauntlet,
    Complete,
}

#[derive(Debug, PartialEq)]
pub enum TournamentError {
    InvalidPhase,
    NoParticipants,
    NonExistantParticipant,
    RaceIsFull,
    ParticipantAlreadyInRace,
    InvalidPlacementValue,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement(u8);

impl Placement {
    pub fn new(val: u8) -> Result<Self, TournamentError> {
        if val == 0 || val > MAX_RACERS as u8 {
            return Err(TournamentError::InvalidPlacementValue);
        }

        Ok(Placement(val))
    }

    pub fn get_points(&self) -> u32 {
        9 - self.0 as u32
    }
}
#[derive(Debug, Default)]
pub enum RaceRuleset {
    #[default]
    Vanilla,
    Beerio,
}

#[derive(Debug, Default)]
pub struct Race {
    racers: Vec<(ParticipantId, Option<Placement>)>,
    ruleset: RaceRuleset,
}

impl Race {
    pub fn set_ruleset(&mut self, ruleset: RaceRuleset) {
        self.ruleset = ruleset;
    }

    pub fn add_racer(&mut self, racer: ParticipantId) -> Result<(), TournamentError> {
        if self.racers.len() >= MAX_RACERS {
            return Err(TournamentError::RaceIsFull);
        }

        if self.racers.iter().any(|(r, _)| *r == racer) {
            return Err(TournamentError::ParticipantAlreadyInRace);
        }

        self.racers.push((racer, None));
        Ok(())
    }

    pub fn remove_racer(&mut self, racer: ParticipantId) -> Result<(), TournamentError> {
        if let Some(idx) = self.racers.iter().position(|(r, _)| *r == racer) {
            self.racers.remove(idx);
            Ok(())
        } else {
            Err(TournamentError::NonExistantParticipant)
        }
    }

    pub fn set_placement(
        &mut self,
        racer: ParticipantId,
        place: Placement,
    ) -> Result<(), TournamentError> {
        if place.0 > self.racers.len() as u8 {
            return Err(TournamentError::InvalidPlacementValue);
        }

        if let Some((_, p)) = self.racers.iter_mut().find(|(r, _)| *r == racer) {
            *p = Some(place);
            Ok(())
        } else {
            Err(TournamentError::NonExistantParticipant)
        }
    }

    pub fn is_complete(&self) -> bool {
        !self.racers.is_empty() && self.racers.iter().all(|(_, p)| p.is_some())
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

// Pool Logic

#[derive(Debug)]
pub struct Pool {
    buckets: Box<[Vec<ParticipantId>]>,
    races: Vec<Race>,
}

impl Pool {
    pub fn new(race_count: usize) -> Self {
        Self::new_with_participants(race_count, &[])
    }

    pub fn new_with_participants(race_count: usize, participants: &[ParticipantId]) -> Self {
        // The number of races determines the number of buckets.
        let mut pool = Self {
            buckets: vec![vec![]; race_count + 1].into_boxed_slice(),
            races: vec![],
        };

        // Put all racers in the first bucket.
        pool.buckets[0].extend_from_slice(participants);
        pool
    }
}
