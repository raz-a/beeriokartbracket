use std::num::NonZero;

use slotmap::SlotMap;

use crate::Pool;
use crate::error::TournamentError;
use crate::participant::{Participant, ParticipantId};

#[derive(Debug, Default)]
pub enum TournamentPhase {
    #[default]
    Registration,
    Pools(Pool),
    Bracket,
    Gauntlet,
    Complete,
}

pub enum TournamentView {}

#[derive(Debug, Default)]
pub struct Tournament {
    phase: TournamentPhase,
    config: Config,
    participants: SlotMap<ParticipantId, Participant>,
}

impl Tournament {
    // Phase guards

    fn ensure_registration(&self) -> Result<(), TournamentError> {
        matches!(self.phase, TournamentPhase::Registration)
            .then_some(())
            .ok_or(TournamentError::WrongPhase)
    }

    fn pools_mut(&mut self) -> Result<&mut Pool, TournamentError> {
        match &mut self.phase {
            TournamentPhase::Pools(pool) => Ok(pool),
            _ => Err(TournamentError::WrongPhase),
        }
    }

    // Common Functions:

    pub fn view(&self) -> TournamentView {
        todo!("Implement View logic to give state based on current phase");
    }

    pub fn next_phase(&mut self) -> Result<(), TournamentError> {
        match &self.phase {
            TournamentPhase::Registration => {
                self.phase = TournamentPhase::Pools(Pool::new(
                    self.config.pool_rounds.into(),
                    &self.participants.keys().collect::<Vec<_>>(),
                    rand::random(),
                )?);

                Ok(())
            }
            TournamentPhase::Pools(pool) => {
                if pool.is_complete() {
                    self.phase = TournamentPhase::Bracket;
                    Ok(())
                } else {
                    Err(TournamentError::PoolsNotCompleted)
                }
            }
            TournamentPhase::Bracket => todo!(),
            TournamentPhase::Gauntlet => todo!(),
            TournamentPhase::Complete => todo!(),
        }
    }

    // Registration Functions

    pub fn add_participant(&mut self, name: &str) -> Result<ParticipantId, TournamentError> {
        self.ensure_registration()?;
        Ok(self.participants.insert(Participant::new(name)))
    }

    pub fn remove_participant(&mut self, id: ParticipantId) -> Result<(), TournamentError> {
        self.ensure_registration()?;
        self.participants
            .remove(id)
            .map(|_| ())
            .ok_or(TournamentError::NonExistentParticipant)
    }

    pub fn set_config(&mut self, config: Config) -> Result<(), TournamentError> {
        self.ensure_registration()?;
        self.config = config;
        Ok(())
    }

    // Pools Functions
    pub fn advance_pools(&mut self) -> Result<bool, TournamentError> {
        let pool = self.pools_mut()?;
        Ok(pool.advance()?.is_none())
    }
}

#[derive(Debug)]
pub struct Config {
    pool_rounds: NonZero<usize>,
    bracket_size: NonZero<usize>,
}

const DEFAULT_POOL_ROUNDS: NonZero<usize> = NonZero::new(8).unwrap();
const DEFAULT_BRACKET_SIZE: NonZero<usize> = NonZero::new(16).unwrap();

impl Default for Config {
    fn default() -> Self {
        Self {
            pool_rounds: DEFAULT_POOL_ROUNDS,
            bracket_size: DEFAULT_BRACKET_SIZE,
        }
    }
}

impl Config {
    pub fn set_pool_rounds(&mut self, rounds: NonZero<usize>) {
        self.pool_rounds = rounds;
    }

    pub fn set_bracket_size(&mut self, size: NonZero<usize>) {
        self.bracket_size = size;
    }
}
