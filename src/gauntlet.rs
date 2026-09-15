use std::collections::{HashMap, HashSet};
use std::num::NonZero;

use crate::participant::{ParticipantId, ParticipantMap, ParticipantView};
use crate::persistence::PersistedState;
use crate::race::{Race, RaceView};
use crate::view::Viewable;
use crate::{Placement, RaceRuleset, TournamentError};

#[derive(serde::Serialize, serde::Deserialize)]
struct GauntletRacer {
    starting_lives: usize,
    current_lives: usize,
    placement: Option<Placement>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Gauntlet {
    #[serde(with = "gauntlet_racers")]
    racers: HashMap<ParticipantId, GauntletRacer>,
    #[serde(default)]
    racer_order: Vec<ParticipantId>,
    races: Vec<Race>,
    beerio_interval: usize,
}

mod gauntlet_racers {
    use std::collections::HashMap;

    use serde::{Deserialize, Serialize};

    use super::{GauntletRacer, ParticipantId};

    pub(super) fn serialize<S>(
        racers: &HashMap<ParticipantId, GauntletRacer>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        racers.iter().collect::<Vec<_>>().serialize(serializer)
    }

    pub(super) fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<HashMap<ParticipantId, GauntletRacer>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let racers = Vec::<(ParticipantId, GauntletRacer)>::deserialize(deserializer)?;
        let mut result = HashMap::with_capacity(racers.len());
        for (id, racer) in racers {
            if result.insert(id, racer).is_some() {
                return Err(serde::de::Error::custom("duplicate gauntlet racer"));
            }
        }
        Ok(result)
    }
}

impl PersistedState<ParticipantMap> for Gauntlet {
    fn validate_loaded(&self, participants: &ParticipantMap) -> Result<(), &'static str> {
        if self.racers.is_empty() || self.racers.len() > crate::race::MAX_RACERS {
            return Err("gauntlet has an invalid racer count");
        }
        if self.beerio_interval == 0 {
            return Err("gauntlet Beerio interval cannot be zero");
        }
        for (id, racer) in &self.racers {
            if !participants.contains_key(*id) {
                return Err("gauntlet references a missing participant");
            }
            if racer.starting_lives == 0 || racer.current_lives > racer.starting_lives {
                return Err("gauntlet racer has invalid lives");
            }
        }
        if !self.racer_order.is_empty()
            && (self.racer_order.len() != self.racers.len()
                || self.racer_order.iter().collect::<HashSet<_>>().len() != self.racers.len()
                || self
                    .racer_order
                    .iter()
                    .any(|id| !self.racers.contains_key(id)))
        {
            return Err("gauntlet racer order does not match its racers");
        }
        for race in &self.races {
            race.validate_loaded(participants)?;
        }

        Ok(())
    }
}

impl Gauntlet {
    pub(crate) fn new(
        winners: Vec<ParticipantId>,
        losers: Vec<ParticipantId>,
        lives: NonZero<usize>,
    ) -> Self {
        let racer_order = winners.iter().chain(&losers).copied().collect();
        Self {
            racers: winners
                .into_iter()
                .map(|id| {
                    (
                        id,
                        GauntletRacer {
                            starting_lives: lives.get() * 2,
                            current_lives: lives.get() * 2,
                            placement: None,
                        },
                    )
                })
                .chain(losers.into_iter().map(|id| {
                    (
                        id,
                        GauntletRacer {
                            starting_lives: lives.get(),
                            current_lives: lives.get(),
                            placement: None,
                        },
                    )
                }))
                .collect(),
            racer_order,
            races: vec![],
            beerio_interval: lives.get(),
        }
    }

    fn active_race(&mut self) -> Option<&mut Race> {
        self.races.last_mut()
    }

    pub(crate) fn race_by_id(&mut self, id: usize) -> Option<&mut Race> {
        self.races.get_mut(id)
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.racers
            .iter()
            .all(|(_, racer)| racer.placement.is_some())
    }

    pub(crate) fn results(&self) -> Result<Vec<(ParticipantId, Placement)>, TournamentError> {
        if !self.is_complete() {
            return Err(TournamentError::GauntletNotCompleted);
        }

        Ok(self
            .racers
            .iter()
            .map(|(id, info)| (*id, info.placement.unwrap()))
            .collect())
    }

    pub(crate) fn advance(&mut self) -> Result<bool, TournamentError> {
        // Reset racer state to recalculate.
        for racer in self.racers.values_mut() {
            racer.current_lives = racer.starting_lives;
            racer.placement = None;
        }

        let mut current_place = Placement::new(self.racers.len() as u8)?;

        let mut trim_size = None;

        // Go through each race and calculate lives and placements.
        for (index, race) in self.races.iter().enumerate() {
            if !race.contains_racers(&self.surviving_racers()) {
                // Found a race that does not contain the right racers. Clear the state from here
                // and beyond.
                trim_size = Some(index);
                break;
            }

            if !race.is_complete() {
                // Found an incomplete race, no other races should exist after here.
                trim_size = Some(index + 1);
                break;
            }

            let mut results: Vec<_> = race
                .get_racers_and_placements()
                .iter()
                .map(|&(id, place)| (id, place.unwrap()))
                .collect();

            results.sort_by_key(|(_, a)| std::cmp::Reverse(a.placement()));
            let cutoff_placement = (results.len() / 2) as u8;

            let has_survivor = results.iter().any(|(id, place)| {
                place.placement() <= cutoff_placement || self.racers[id].current_lives > 1
            });
            if !has_survivor {
                return Err(TournamentError::GauntletHasNoSurvivor);
            }

            // Subtract lives based on placement.
            for (id, place) in results {
                if place.placement() <= cutoff_placement {
                    continue;
                }

                let racer = self
                    .racers
                    .get_mut(&id)
                    .ok_or(TournamentError::RacerNotInRace)?;

                racer.current_lives -= 1;

                if racer.current_lives == 0 {
                    // Racer is out, give them a placement.
                    racer.placement = Some(current_place);
                    current_place = current_place
                        .move_up()
                        .ok_or(TournamentError::ResultsDontMatchRace)?;
                }
            }

            // If current place is first, then we have only one survivor. Crown them as the winner.
            if current_place.placement() == 1 {
                let winner = self.surviving_racers()[0];
                self.racers
                    .entry(winner)
                    .and_modify(|racer| racer.placement = Some(current_place));

                assert!(self.is_complete());
                self.races.truncate(index + 1);

                return Ok(true);
            }
        }

        // Trim now invalid races.
        if let Some(trim_size) = trim_size {
            self.races.truncate(trim_size);
        }

        // If active race is complete, or there is no active race, create the next active race.
        if let Some(race) = self.active_race()
            && !race.is_complete()
        {
            return Ok(false);
        }

        let racers = self.surviving_racers();
        let ruleset = if self.races.len() % self.beerio_interval == self.beerio_interval - 1 {
            RaceRuleset::Beerio
        } else {
            RaceRuleset::Vanilla
        };

        let race = self.races.push_mut(Race::default());
        race.add_racers(&racers)?;
        race.set_ruleset(ruleset);
        Ok(false)
    }

    fn surviving_racers(&self) -> Vec<ParticipantId> {
        self.ordered_racers()
            .filter(|id| {
                let state = &self.racers[id];
                state.placement.is_none()
            })
            .collect()
    }

    fn ordered_racers(&self) -> impl Iterator<Item = ParticipantId> + '_ {
        let order = if self.racer_order.is_empty() {
            self.races.first().map_or_else(
                || self.racers.keys().copied().collect(),
                |race| race.get_racers().collect(),
            )
        } else {
            self.racer_order.clone()
        };
        order.into_iter()
    }
}

#[derive(Debug)]
pub struct GauntletRacerView {
    pub participant: ParticipantView,
    pub lives: usize,
    pub placement: Option<Placement>,
}

#[derive(Debug)]
pub struct GauntletView {
    pub racers: Vec<GauntletRacerView>,
    pub races: Vec<RaceView>,
}

impl Viewable<GauntletView> for Gauntlet {
    fn view(&self, id_map: &ParticipantMap) -> GauntletView {
        let racers = self
            .ordered_racers()
            .map(|id| {
                let racer = &self.racers[&id];
                GauntletRacerView {
                    participant: id.view(id_map),
                    lives: racer.current_lives,
                    placement: racer.placement,
                }
            })
            .collect();

        GauntletView {
            racers,
            races: self.races.iter().map(|race| race.view(id_map)).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::participant::Participant;

    fn make_racers(count: usize) -> (ParticipantMap, Vec<ParticipantId>) {
        let mut participants = ParticipantMap::with_key();
        let racers = (1..=count)
            .map(|number| participants.insert(Participant::new(&format!("Player {number}"))))
            .collect();
        (participants, racers)
    }

    fn set_results(race: &mut Race, results: &[(ParticipantId, u8)]) {
        for &(racer, placement) in results {
            race.set_placement(racer, Some(Placement::new(placement).unwrap()))
                .unwrap();
        }
    }

    #[test]
    fn winners_start_with_twice_as_many_lives() {
        let (_, racers) = make_racers(4);
        let gauntlet = Gauntlet::new(
            racers[..2].to_vec(),
            racers[2..].to_vec(),
            NonZero::new(3).unwrap(),
        );

        assert_eq!(gauntlet.racers[&racers[0]].current_lives, 6);
        assert_eq!(gauntlet.racers[&racers[1]].current_lives, 6);
        assert_eq!(gauntlet.racers[&racers[2]].current_lives, 3);
        assert_eq!(gauntlet.racers[&racers[3]].current_lives, 3);
    }

    #[test]
    fn configured_lives_determine_beerio_interval() {
        let (participants, racers) = make_racers(2);
        let mut gauntlet = Gauntlet::new(vec![], racers.clone(), NonZero::new(2).unwrap());

        assert!(!gauntlet.advance().unwrap());
        assert!(matches!(
            gauntlet.races[0].view(&participants).ruleset,
            RaceRuleset::Vanilla
        ));

        set_results(
            gauntlet.active_race().unwrap(),
            &[(racers[0], 1), (racers[1], 1)],
        );
        assert!(!gauntlet.advance().unwrap());
        assert!(matches!(
            gauntlet.races[1].view(&participants).ruleset,
            RaceRuleset::Beerio
        ));
    }

    #[test]
    fn last_survivor_is_crowned_the_winner() {
        let (_, racers) = make_racers(2);
        let mut gauntlet = Gauntlet::new(vec![], racers.clone(), NonZero::new(1).unwrap());

        assert!(!gauntlet.advance().unwrap());
        set_results(
            gauntlet.active_race().unwrap(),
            &[(racers[0], 1), (racers[1], 2)],
        );

        assert!(gauntlet.advance().unwrap());
        assert_eq!(
            gauntlet.racers[&racers[0]].placement,
            Some(Placement::new(1).unwrap())
        );
        assert_eq!(
            gauntlet.racers[&racers[1]].placement,
            Some(Placement::new(2).unwrap())
        );
    }

    #[test]
    fn eliminating_every_survivor_returns_an_error() {
        let (_, racers) = make_racers(2);
        let mut gauntlet = Gauntlet::new(vec![], racers.clone(), NonZero::new(1).unwrap());

        assert!(!gauntlet.advance().unwrap());
        set_results(
            gauntlet.active_race().unwrap(),
            &[(racers[0], 2), (racers[1], 2)],
        );

        assert_eq!(
            gauntlet.advance(),
            Err(TournamentError::GauntletHasNoSurvivor)
        );
        assert!(!gauntlet.is_complete());
        assert_eq!(gauntlet.surviving_racers().len(), 2);
    }

    #[test]
    fn disqualified_racer_loses_a_life() {
        let (_, racers) = make_racers(4);
        let mut gauntlet = Gauntlet::new(vec![], racers.clone(), NonZero::new(2).unwrap());

        assert!(!gauntlet.advance().unwrap());
        let race = gauntlet.active_race().unwrap();
        set_results(
            race,
            &[
                (racers[0], 1),
                (racers[1], 2),
                (racers[2], 3),
                (racers[3], 4),
            ],
        );
        race.set_placement(racers[3], Some(Placement::DISQUALIFIED))
            .unwrap();

        assert!(!gauntlet.advance().unwrap());
        assert_eq!(gauntlet.racers[&racers[0]].current_lives, 2);
        assert_eq!(gauntlet.racers[&racers[1]].current_lives, 2);
        assert_eq!(gauntlet.racers[&racers[2]].current_lives, 1);
        assert_eq!(gauntlet.racers[&racers[3]].current_lives, 1);
    }

    #[test]
    fn correction_that_finishes_earlier_truncates_later_races() {
        let (_, racers) = make_racers(3);
        let mut gauntlet = Gauntlet::new(vec![], racers.clone(), NonZero::new(2).unwrap());

        assert!(!gauntlet.advance().unwrap());
        set_results(
            gauntlet.active_race().unwrap(),
            &[(racers[0], 1), (racers[1], 2), (racers[2], 3)],
        );
        assert!(!gauntlet.advance().unwrap());
        set_results(
            gauntlet.active_race().unwrap(),
            &[(racers[0], 3), (racers[1], 1), (racers[2], 2)],
        );
        assert!(!gauntlet.advance().unwrap());
        assert_eq!(gauntlet.races.len(), 3);

        set_results(
            gauntlet.race_by_id(0).unwrap(),
            &[(racers[0], 2), (racers[1], 1), (racers[2], 3)],
        );

        assert!(gauntlet.advance().unwrap());
        assert_eq!(gauntlet.races.len(), 2);
    }

    #[test]
    fn correction_with_same_eliminations_preserves_later_races() {
        let (_, racers) = make_racers(4);
        let mut gauntlet = Gauntlet::new(vec![], racers.clone(), NonZero::new(2).unwrap());

        assert!(!gauntlet.advance().unwrap());
        set_results(
            gauntlet.active_race().unwrap(),
            &[
                (racers[0], 1),
                (racers[1], 2),
                (racers[2], 3),
                (racers[3], 4),
            ],
        );
        assert!(!gauntlet.advance().unwrap());
        set_results(
            gauntlet.active_race().unwrap(),
            &[
                (racers[0], 3),
                (racers[1], 4),
                (racers[2], 1),
                (racers[3], 2),
            ],
        );
        assert!(!gauntlet.advance().unwrap());
        set_results(
            gauntlet.active_race().unwrap(),
            &[
                (racers[0], 1),
                (racers[1], 1),
                (racers[2], 1),
                (racers[3], 1),
            ],
        );
        assert!(!gauntlet.advance().unwrap());
        assert_eq!(gauntlet.races.len(), 4);

        set_results(
            gauntlet.race_by_id(0).unwrap(),
            &[
                (racers[0], 2),
                (racers[1], 1),
                (racers[2], 3),
                (racers[3], 4),
            ],
        );

        assert!(!gauntlet.advance().unwrap());
        assert_eq!(gauntlet.races.len(), 4);
        assert!(gauntlet.races[3].contains_racers(&racers));
    }

    #[test]
    fn view_and_race_order_stay_aligned_after_eliminations() {
        let (participants, racers) = make_racers(4);
        let mut gauntlet = Gauntlet::new(
            racers[..2].to_vec(),
            racers[2..].to_vec(),
            NonZero::new(1).unwrap(),
        );
        let initial_order: Vec<_> = gauntlet
            .view(&participants)
            .racers
            .into_iter()
            .map(|racer| racer.participant.id)
            .collect();
        assert_eq!(initial_order, racers);

        assert!(!gauntlet.advance().unwrap());
        assert_eq!(
            gauntlet.races[0].get_racers().collect::<Vec<_>>(),
            initial_order
        );
        set_results(
            gauntlet.active_race().unwrap(),
            &[
                (racers[0], 1),
                (racers[1], 2),
                (racers[2], 3),
                (racers[3], 4),
            ],
        );
        assert!(!gauntlet.advance().unwrap());
        assert_eq!(
            gauntlet.races[1].get_racers().collect::<Vec<_>>(),
            racers[..2]
        );

        let updated_order: Vec<_> = gauntlet
            .view(&participants)
            .racers
            .into_iter()
            .map(|racer| racer.participant.id)
            .collect();
        assert_eq!(updated_order, initial_order);
    }
}
