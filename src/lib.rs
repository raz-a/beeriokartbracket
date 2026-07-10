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
    Complete,
}

#[derive(Debug, PartialEq)]
pub enum TournamentError {
    InvalidPhase,
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

    pub fn remove_participant(&mut self, id: ParticipantId) {
        self.tourney.participants.remove(id);
    }

    pub fn start(self) {
        self.tourney.phase = TournamentPhase::Pools;
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
