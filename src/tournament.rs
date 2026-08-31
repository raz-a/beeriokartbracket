use std::collections::HashSet;

use crate::bracket::Bracket;
use crate::config::Config;
use crate::error::TournamentError;
use crate::gauntlet::Gauntlet;
use crate::participant::{Participant, ParticipantId, ParticipantMap, ParticipantView};
use crate::pool::Pool;
use crate::race::{Race, RaceId};
use crate::view::{RegistrationView, TournamentView, Viewable};
use crate::{BracketSetId, Placement};

#[derive(Debug, Default)]
enum TournamentPhase {
    #[default]
    Registration,
    Pools(Box<Pool>),
    Bracket(Box<Bracket>),
    Gauntlet(Box<Gauntlet>),
    _Complete,
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

    fn bracket_mut(&mut self) -> Result<&mut Bracket, TournamentError> {
        match &mut self.phase {
            TournamentPhase::Bracket(bracket) => Ok(bracket),
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
            TournamentPhase::Bracket(bracket) => {
                if !bracket.is_complete() {
                    return Err(TournamentError::BracketNotCompleted);
                }

                let (winners, losers) = bracket
                    .get_results()
                    .ok_or(TournamentError::BracketNotCompleted)?;

                self.phase = TournamentPhase::Gauntlet(Box::new(Gauntlet::new(
                    winners,
                    losers,
                    self.config.gauntlet_lives.into(),
                )));

                Ok(())
            }
            TournamentPhase::Gauntlet(_) => todo!(),
            TournamentPhase::_Complete => todo!(),
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

    // Brackets Functions.
    pub fn advance_bracket(&mut self) -> Result<bool, TournamentError> {
        self.bracket_mut()?.advance()
    }

    pub fn update_bracket_set(
        &mut self,
        id: BracketSetId,
        race_index: usize,
        results: Vec<(ParticipantId, Option<Placement>)>,
    ) -> Result<bool, TournamentError> {
        let set = self.bracket_mut()?.set(id)?;
        let race = set.race(race_index).ok_or(TournamentError::RaceNotFound)?;
        Self::update_race(race, results)?;
        Ok(set.is_completed())
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
            TournamentPhase::Gauntlet(gauntlet) => {
                TournamentView::Gauntlet(gauntlet.as_ref().view(id_map))
            }
            TournamentPhase::_Complete => TournamentView::Complete,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_bracket_advances_finalists_to_gauntlet() {
        let mut tournament = Tournament::default();
        let racers: Vec<_> = (1..=16)
            .map(|number| {
                tournament
                    .participants
                    .insert(Participant::new(&format!("Player {number}")))
            })
            .collect();
        tournament.phase = TournamentPhase::Bracket(Box::new(Bracket::new(1, &racers).unwrap()));

        loop {
            let TournamentView::Bracket(bracket) = tournament.view() else {
                panic!("tournament should remain in the bracket phase");
            };
            let Some(active_id) = bracket.active_set else {
                assert_eq!(bracket.winners_finalists.len(), 4);
                assert_eq!(bracket.losers_finalists.len(), 4);
                break;
            };
            let active_set = bracket
                .winners
                .iter()
                .chain(&bracket.losers)
                .flat_map(|round| &round.sets)
                .find(|(id, _)| *id == active_id)
                .map(|(_, set)| set)
                .unwrap();
            let results =
                active_set.racers.iter().enumerate().map(|(index, racer)| {
                    (racer.id, Some(Placement::new((index + 1) as u8).unwrap()))
                });

            for race_index in 0..active_set.races.len() {
                tournament
                    .update_bracket_set(active_id, race_index, results.clone().collect())
                    .unwrap();
            }
            tournament.advance_bracket().unwrap();
        }

        tournament.next_phase().unwrap();

        let TournamentView::Gauntlet(gauntlet) = tournament.view() else {
            panic!("completed bracket should advance to the gauntlet");
        };
        assert_eq!(gauntlet.racers.len(), 8);
        assert!(
            gauntlet
                .racers
                .windows(2)
                .all(|racers| racers[0].lives >= racers[1].lives)
        );
        assert_eq!(
            gauntlet
                .racers
                .iter()
                .filter(|racer| racer.lives == 6)
                .count(),
            4
        );
        assert_eq!(
            gauntlet
                .racers
                .iter()
                .filter(|racer| racer.lives == 3)
                .count(),
            4
        );
    }
}
