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

pub enum TournamentError {
    InvalidPhase,
}

#[derive(Debug, Default)]
pub struct Tournament {
    phase: TournamentPhase,
    participants: SlotMap<ParticipantId, Participant>,
}

impl Tournament {
    pub fn registration(&mut self) -> Option<Registration<'_>> {
        matches!(self.phase, TournamentPhase::Registration).then(|| Registration { tourney: self })
    }

    pub fn start(&mut self) -> Result<(), TournamentError> {
        if self.registration().is_some() {
            self.phase = TournamentPhase::Pools;
            return Ok(());
        }

        Err(TournamentError::InvalidPhase)
    }
}

// Registration logic

impl Registration<'_> {
    pub fn add_participant(&mut self, name: &str, seed: u32) -> ParticipantId {
        self.tourney
            .participants
            .insert(Participant::new(name, seed))
    }

    pub fn remove_participant(&mut self, id: ParticipantId) {
        self.tourney.participants.remove(id);
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

pub struct Registration<'a> {
    tourney: &'a mut Tournament,
}
