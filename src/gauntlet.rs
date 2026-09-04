use std::collections::HashMap;
use std::num::NonZero;

use crate::participant::{ParticipantId, ParticipantMap, ParticipantView};
use crate::race::{Race, RaceView};
use crate::view::Viewable;
use crate::{Placement, RaceRuleset, TournamentError};

#[derive(Debug)]
struct GauntletRacer {
    starting_lives: usize,
    current_lives: usize,
    placement: Option<Placement>,
}

#[derive(Debug)]
pub(crate) struct Gauntlet {
    racers: HashMap<ParticipantId, GauntletRacer>,
    races: Vec<Race>,
    beerio_interval: usize,
}

impl Gauntlet {
    pub(crate) fn new(
        winners: Vec<ParticipantId>,
        losers: Vec<ParticipantId>,
        lives: NonZero<usize>,
    ) -> Self {
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
            races: vec![],
            beerio_interval: lives.get(),
        }
    }

    pub(crate) fn active_race(&mut self) -> Option<&mut Race> {
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

            let results = race
                .get_racers_and_placements()
                .iter()
                .map(|&(id, place)| (id, place.unwrap()));

            let cutoff_placement = (results.len() / 2) as u8;

            let mut eliminated_players = 0;

            // Subtract lives based on placement.
            for (id, place) in results {
                if place.placement() <= cutoff_placement {
                    continue;
                }

                self.racers.entry(id).and_modify(|racer| {
                    racer.current_lives -= 1;

                    if racer.current_lives == 0 {
                        // Racer is out, give them a placement.
                        racer.placement = Some(current_place);
                        eliminated_players += 1;
                    }
                });
            }

            // Update current place based on eliminations.
            for _ in 0..eliminated_players {
                current_place = current_place
                    .move_up()
                    .ok_or(TournamentError::ResultsDontMatchRace)?;
            }

            // If current place is first, then we have only one survivor. Crown them as the winner.
            if current_place.placement() == 1 {
                let winner = self.surviving_racers()[0];
                self.racers
                    .entry(winner)
                    .and_modify(|racer| racer.placement = Some(current_place));

                assert!(self.is_complete());

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
        self.racers
            .iter()
            .filter_map(|(&id, state)| {
                if state.placement.is_none() {
                    Some(id)
                } else {
                    None
                }
            })
            .collect()
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
        let mut racers: Vec<_> = self
            .racers
            .iter()
            .map(|(&id, racer)| GauntletRacerView {
                participant: id.view(id_map),
                lives: racer.current_lives,
                placement: racer.placement,
            })
            .collect();
        racers.sort_by(|left, right| {
            right
                .lives
                .cmp(&left.lives)
                .then_with(|| left.participant.name.cmp(&right.participant.name))
        });

        GauntletView {
            racers,
            races: self.races.iter().map(|race| race.view(id_map)).collect(),
        }
    }
}
