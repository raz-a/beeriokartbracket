use slotmap::SlotMap;

use crate::error::TournamentError;
use crate::participant::{Participant, ParticipantId};

#[derive(Debug, Default)]
pub enum TournamentPhase {
    #[default]
    Registration,
    Pools,
    Bracket,
    Gauntlet,
    Complete,
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
}

pub struct Registration<'a> {
    tourney: &'a mut Tournament,
}

impl Registration<'_> {
    pub fn add_participant(&mut self, name: &str, seed: usize) -> ParticipantId {
        self.tourney
            .participants
            .insert(Participant::new(name, seed))
    }

    pub fn remove_participant(&mut self, id: ParticipantId) -> Result<(), TournamentError> {
        self.tourney
            .participants
            .remove(id)
            .map(|_| ())
            .ok_or(TournamentError::NonExistentParticipant)
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
