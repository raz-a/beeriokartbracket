use std::collections::HashMap;

use crate::{
    ParticipantId, Placement, RaceRuleset, TournamentError,
    race::{MAX_RACERS, Race},
};

struct BracketSet {
    races: Vec<Race>,
    expected_size: usize,
}

impl BracketSet {
    pub fn new(num_races: usize, racer_count: usize) -> Result<Self, TournamentError> {
        if racer_count > MAX_RACERS || racer_count < 4 {
            return Err(TournamentError::InvalidBracketSetSize);
        }

        let mut races = Vec::with_capacity(num_races);
        for i in 0..num_races {
            let mut race = Race::default();

            // Even races are Vanilla. Odd are Beerio.
            let ruleset = if i.is_multiple_of(2) {
                RaceRuleset::Vanilla
            } else {
                RaceRuleset::Beerio
            };

            race.set_ruleset(ruleset);
            races.push(race);
        }

        Ok(Self {
            races,
            expected_size: racer_count,
        })
    }

    fn race_count(&self) -> usize {
        self.races
            .first()
            .map_or(0, |race| race.get_racers().count())
    }

    fn is_started(&self) -> bool {
        self.races.iter().any(|race| race.is_complete())
    }

    fn is_completed(&self) -> bool {
        self.races.iter().all(|race| race.is_complete())
    }

    pub fn add_racers(&mut self, racers: &[ParticipantId]) -> Result<(), TournamentError> {
        if self.is_started() {
            Err(TournamentError::BracketSetAlreadyStarted)
        } else if self.race_count() + racers.len() > self.expected_size {
            Err(TournamentError::RaceIsFull)
        } else {
            for race in self.races.iter_mut() {
                race.add_racers(racers)?
            }

            Ok(())
        }
    }

    pub fn current_race_index(&self) -> usize {
        self.races
            .iter()
            .take_while(|race| race.is_complete())
            .count()
    }

    pub fn race(&mut self, index: usize) -> Option<&mut Race> {
        self.races.get_mut(index)
    }

    pub fn get_scores(&self) -> Vec<(ParticipantId, usize)> {
        let mut map = HashMap::new();

        for race in self.races.iter() {
            for &(id, place) in race.get_racers_and_placements() {
                let points = place.map_or(0, |p| p.points());

                map.entry(id).and_modify(|e| *e += points).or_insert(points);
            }
        }

        map.into_iter().collect()
    }
}

type BracketRound = Vec<BracketSet>;

pub struct Bracket {
    winners: Vec<BracketRound>,
    losers: Vec<BracketRound>,
}

impl Bracket {
    pub fn new(racers: &[ParticipantId]) -> Self {

        // Construct groups of winners sets that ensure no races have less than 6 partiticpants and
        // also each race has the same number of participants +/- 1.

        // If the number of winners sets is not a power of two. Construct a round 0 with byes so the
        // first round has a power of two number of
    }
}
