use crate::config::Config;
use crate::error::TournamentError;
use crate::participant::{Participant, ParticipantId, ParticipantView};
use crate::pool::Pool;
use crate::view::{ParticipantMap, RegistrationView, TournamentView, Viewable};

#[derive(Debug, Default)]
enum TournamentPhase {
    #[default]
    Registration,
    Pools(Box<Pool>),
    Bracket,
    Gauntlet,
    Complete,
}

#[derive(Debug, Default)]
pub struct Tournament {
    phase: TournamentPhase,
    config: Config,
    participants: ParticipantMap,
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

    pub fn next_phase(&mut self) -> Result<(), TournamentError> {
        match &self.phase {
            TournamentPhase::Registration => {
                if self.participants.is_empty() {
                    return Err(TournamentError::NoParticipants);
                }

                // TODO: The pool seed is non-deterministic (rand::random()), so the
                // tournament flow can't be reproduced in tests. Thread a seedable
                // value through Config to make this deterministic later.
                self.phase = TournamentPhase::Pools(Box::new(Pool::new(
                    self.config.pool_rounds.into(),
                    &self.participants.keys().collect::<Vec<_>>(),
                    rand::random(),
                )?));

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

    pub fn view(&self) -> TournamentView {
        Viewable::view(self, &self.participants)
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
        self.pools_mut()?.advance()
    }
}

impl Viewable<TournamentView> for Tournament {
    fn view(&self, id_map: &ParticipantMap) -> TournamentView {
        match &self.phase {
            TournamentPhase::Registration => TournamentView::Registration(RegistrationView {
                participants: id_map
                    .iter()
                    .map(|(id, participant)| ParticipantView {
                        id,
                        name: participant.name().to_owned(),
                    })
                    .collect(),
                config: self.config,
            }),
            TournamentPhase::Pools(pool) => TournamentView::Pools(pool.as_ref().view(id_map)),
            TournamentPhase::Bracket => TournamentView::Bracket,
            TournamentPhase::Gauntlet => TournamentView::Gauntlet,
            TournamentPhase::Complete => TournamentView::Complete,
        }
    }
}
