use std::collections::HashSet;

use crate::Placement;
use crate::bracket::Bracket;
use crate::config::Config;
use crate::error::TournamentError;
use crate::participant::{Participant, ParticipantId, ParticipantMap, ParticipantView};
use crate::pool::Pool;
use crate::race::{Race, RaceId};
use crate::view::{RegistrationView, TournamentView, Viewable};

#[derive(Debug, Default)]
enum TournamentPhase {
    #[default]
    Registration,
    Pools(Box<Pool>),
    Bracket(Box<Bracket>),
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

                self.phase = TournamentPhase::Pools(Box::new(Pool::new(
                    self.config.pool_rounds.into(),
                    &self.participants.keys().collect::<Vec<_>>(),
                    self.config.seed,
                )?));

                Ok(())
            }
            TournamentPhase::Pools(pool) => {
                if !pool.is_complete() {
                    return Err(TournamentError::PoolsNotCompleted);
                }

                let results = pool
                    .get_results(self.config.bracket_size.get())
                    .ok_or(TournamentError::PoolsNotCompleted)?;
                let racers = results.advanced_ids();

                self.phase = TournamentPhase::Bracket(Box::new(Bracket::new(
                    self.config.bracket_races_per_round.get(),
                    &racers,
                )?));

                Ok(())
            }
            TournamentPhase::Bracket(_) => todo!(),
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

    pub fn update_active_race(
        &mut self,
        results: Vec<(ParticipantId, Option<Placement>)>,
    ) -> Result<bool, TournamentError> {
        let current_race = self
            .pools_mut()?
            .active_race()
            .ok_or(TournamentError::RaceNotFound)?;

        Self::update_race(current_race, results)
    }

    pub fn update_completed_race(
        &mut self,
        id: RaceId,
        results: Vec<(ParticipantId, Option<Placement>)>,
    ) -> Result<bool, TournamentError> {
        let race = self
            .pools_mut()?
            .completed_race(id)
            .ok_or(TournamentError::RaceNotFound)?;

        Self::update_race(race, results)
    }

    fn update_race(
        race: &mut Race,
        results: Vec<(ParticipantId, Option<Placement>)>,
    ) -> Result<bool, TournamentError> {
        let race_ids: HashSet<_> = race.get_racers().collect();
        let result_ids: HashSet<_> = results.iter().map(|(id, _)| *id).collect();

        // Set match rejects missing/extra ids; the length match rejects duplicates.
        if race_ids != result_ids || results.len() != race_ids.len() {
            return Err(TournamentError::ResultsDontMatchRace);
        }

        for (_, p) in results.iter() {
            if let Some(p) = p
                && p.placement() as usize > results.len()
            {
                return Err(TournamentError::InvalidPlacementValue);
            }
        }

        for (racer, place) in results {
            race.set_placement(racer, place)?;
        }

        Ok(race.is_complete())
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
            TournamentPhase::Pools(pool) => TournamentView::Pools((
                pool.as_ref().view(id_map),
                pool.get_results(self.config.bracket_size.get())
                    .map(|r| r.view(id_map)),
            )),
            TournamentPhase::Bracket(bracket) => {
                TournamentView::Bracket(bracket.as_ref().view(id_map))
            }
            TournamentPhase::Gauntlet => TournamentView::Gauntlet,
            TournamentPhase::Complete => TournamentView::Complete,
        }
    }
}
