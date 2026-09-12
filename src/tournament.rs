use std::collections::HashSet;

use crate::bracket::Bracket;
use crate::config::Config;
use crate::error::TournamentError;
use crate::gauntlet::Gauntlet;
use crate::participant::{Participant, ParticipantId, ParticipantMap, ParticipantView};
use crate::persistence::PersistedState;
use crate::pool::Pool;
use crate::race::{Race, RaceId};
use crate::view::{RegistrationView, TournamentResultView, TournamentView, Viewable};
use crate::{BracketSetId, Placement};

// TODO: Add ability to go back from states.

// TODO: Look into having this hosted somewhere so others can view the current tourney state.

#[derive(Default, serde::Serialize, serde::Deserialize)]
enum TournamentPhase {
    #[default]
    Registration,
    Pools(Box<Pool>),
    Bracket(Box<Bracket>),
    Gauntlet(Box<Gauntlet>),
    Complete(Vec<(ParticipantId, Placement)>),
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct Tournament {
    phase: TournamentPhase,
    config: Config,
    participants: ParticipantMap,
}

impl PersistedState for Tournament {
    fn validate_loaded(&self, _context: &()) -> Result<(), &'static str> {
        match &self.phase {
            TournamentPhase::Registration => Ok(()),
            TournamentPhase::Pools(pool) => pool.validate_loaded(&self.participants),
            TournamentPhase::Bracket(bracket) => bracket.validate_loaded(&self.participants),
            TournamentPhase::Gauntlet(gauntlet) => gauntlet.validate_loaded(&self.participants),
            TournamentPhase::Complete(results) => {
                if results
                    .iter()
                    .any(|(id, _)| !self.participants.contains_key(*id))
                {
                    return Err("complete results reference a missing participant");
                }
                Ok(())
            }
        }
    }
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

    fn gauntlet_mut(&mut self) -> Result<&mut Gauntlet, TournamentError> {
        match &mut self.phase {
            TournamentPhase::Gauntlet(gauntlet) => Ok(gauntlet),
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
                    self.config.gauntlet_lives,
                )));

                Ok(())
            }
            TournamentPhase::Gauntlet(gauntlet) => {
                self.phase = TournamentPhase::Complete(gauntlet.results()?);
                Ok(())
            }
            TournamentPhase::Complete(_) => Ok(()),
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
                && !p.is_valid_for_race(results.len())
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

    // Gauntlet Functions.
    pub fn advance_gauntlet(&mut self) -> Result<bool, TournamentError> {
        self.gauntlet_mut()?.advance()
    }

    pub fn update_gauntlet_race(
        &mut self,
        race_index: usize,
        results: Vec<(ParticipantId, Option<Placement>)>,
    ) -> Result<bool, TournamentError> {
        let race = self
            .gauntlet_mut()?
            .race_by_id(race_index)
            .ok_or(TournamentError::RaceNotFound)?;
        Self::update_race(race, results)
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
            TournamentPhase::Complete(results) => {
                let mut results: Vec<_> = results
                    .iter()
                    .map(|&(id, placement)| TournamentResultView {
                        participant: id.view(id_map),
                        placement,
                    })
                    .collect();
                results.sort_by_key(|result| result.placement.placement());
                TournamentView::Complete(results)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZero;

    use super::*;
    use crate::{PersistenceError, deserialize_tournament, serialize_tournament};

    fn round_trip(tournament: &Tournament) -> Tournament {
        let json = serialize_tournament("Test Cup", tournament).unwrap();
        deserialize_tournament(&json).unwrap().1
    }

    #[test]
    fn persistence_round_trips_bracket_gauntlet_and_complete() {
        let mut tournament = Tournament::default();
        let racers: Vec<_> = (1..=16)
            .map(|number| {
                tournament
                    .participants
                    .insert(Participant::new(&format!("Player {number}")))
            })
            .collect();

        tournament.phase = TournamentPhase::Bracket(Box::new(Bracket::new(1, &racers).unwrap()));
        assert!(matches!(
            round_trip(&tournament).view(),
            TournamentView::Bracket(_)
        ));

        tournament.phase = TournamentPhase::Gauntlet(Box::new(Gauntlet::new(
            racers[..4].to_vec(),
            racers[4..8].to_vec(),
            NonZero::new(3).unwrap(),
        )));
        tournament.advance_gauntlet().unwrap();
        assert!(matches!(
            round_trip(&tournament).view(),
            TournamentView::Gauntlet(_)
        ));

        tournament.phase = TournamentPhase::Complete(vec![
            (racers[1], Placement::new(2).unwrap()),
            (racers[0], Placement::new(1).unwrap()),
        ]);
        let TournamentView::Complete(results) = round_trip(&tournament).view() else {
            panic!("expected complete tournament view");
        };
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].participant.name, "Player 1");
        assert_eq!(results[0].placement.placement(), 1);
        assert!(matches!(tournament.next_phase(), Ok(())));
    }

    #[test]
    fn persistence_rejects_dangling_pool_participant() {
        let mut tournament = Tournament::default();
        let racers: Vec<_> = (1..=16)
            .map(|number| {
                tournament
                    .participants
                    .insert(Participant::new(&format!("Player {number}")))
            })
            .collect();
        tournament.phase = TournamentPhase::Pools(Box::new(
            Pool::new(8, &racers, tournament.config.seed).unwrap(),
        ));
        tournament.participants.remove(racers[0]);

        let json = serialize_tournament("Test Cup", &tournament).unwrap();
        assert!(matches!(
            deserialize_tournament(&json),
            Err(PersistenceError::InvalidTournament(_))
        ));
    }

    #[test]
    fn gauntlet_facade_runs_race_and_completes_tournament() {
        let mut tournament = Tournament::default();
        let racers: Vec<_> = (1..=2)
            .map(|number| {
                tournament
                    .participants
                    .insert(Participant::new(&format!("Player {number}")))
            })
            .collect();
        tournament.phase = TournamentPhase::Gauntlet(Box::new(Gauntlet::new(
            vec![],
            racers.clone(),
            NonZero::new(1).unwrap(),
        )));

        assert!(!tournament.advance_gauntlet().unwrap());
        assert_eq!(
            tournament.update_gauntlet_race(
                0,
                vec![
                    (racers[0], Some(Placement::new(1).unwrap())),
                    (racers[1], Some(Placement::new(2).unwrap())),
                ],
            ),
            Ok(true)
        );
        assert!(tournament.advance_gauntlet().unwrap());

        tournament.next_phase().unwrap();
        let TournamentView::Complete(results) = tournament.view() else {
            panic!("expected complete tournament view");
        };
        assert_eq!(results[0].participant.id, racers[0]);
        assert_eq!(results[0].placement.placement(), 1);
    }

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
