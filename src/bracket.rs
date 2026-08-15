use std::collections::HashMap;

use slotmap::{SlotMap, new_key_type};

use crate::participant::{ParticipantMap, ParticipantView};
use crate::view::Viewable;
use crate::{
    ParticipantId, RaceRuleset, TournamentError,
    race::{MAX_RACERS, Race},
    race_group::RaceGroupTracker,
};

new_key_type! { pub struct BracketSetId; }

#[derive(Debug)]
struct BracketSet {
    races: Vec<Race>,
    expected_size: usize,
}

impl BracketSet {
    pub fn new(num_races: usize, racer_count: usize) -> Result<Self, TournamentError> {
        if !(4..=MAX_RACERS).contains(&racer_count) {
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

    fn expected_size(&self) -> usize {
        self.expected_size
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

/// How a losers-bracket round is fed.
#[derive(Debug)]
enum BracketRoundKind {
    Winners,
    LosersIntake { wb_round: usize },
    LosersConsolidate,
}

#[derive(Debug)]
struct BracketRound {
    sets: Vec<BracketSetId>,
    kind: BracketRoundKind,
}

const MIN_WINNERS_BRACKET_RACE_SIZE: usize = 6;
const MIN_LOSERS_BRACKET_RACE_SIZE: usize = 4;

// Each heat advances its top 4 finishers.
const ADVANCERS_PER_SET: usize = 4;

#[derive(Debug)]
pub struct Bracket {
    winners: Vec<BracketRound>,
    losers: Vec<BracketRound>,
    races_per_set: usize,
    bracket_sets: SlotMap<BracketSetId, BracketSet>,
}

impl Bracket {
    pub fn new(races_per_set: usize, racers: &[ParticipantId]) -> Result<Self, TournamentError> {
        let mut bracket_sets = SlotMap::with_key();
        let winners = Self::build_winners(races_per_set, racers, &mut bracket_sets)?;
        let losers = Self::build_losers(&winners, races_per_set, &mut bracket_sets)?;

        Ok(Self {
            winners,
            losers,
            races_per_set,
            bracket_sets,
        })
    }

    /// Builds the winners bracket, seeding round-one heats with `racers` and pre-building the
    /// remaining rounds as empty.
    fn build_winners(
        races_per_set: usize,
        racers: &[ParticipantId],
        bracket_sets: &mut SlotMap<BracketSetId, BracketSet>,
    ) -> Result<Vec<BracketRound>, TournamentError> {
        // Construct groups of winners sets that ensure no races have less than 6 partiticpants and
        // also each race has the same number of participants +/- 1.
        //
        // If the number of winners sets is not a power of two. Construct a first round with byes so
        // the next round has a power of two number of sets.
        let mut tracker = RaceGroupTracker::new(racers.len(), MIN_WINNERS_BRACKET_RACE_SIZE)?;

        let set_count = tracker.get_group_count();
        let num_rounds = set_count.next_power_of_two().trailing_zeros() as usize + 1;

        let first_round_sets = if set_count.is_power_of_two() {
            set_count
        } else {
            2 * (set_count - (set_count.next_power_of_two() / 2))
        };

        let second_round_sets = set_count - first_round_sets;

        let mut winners = Vec::with_capacity(num_rounds);
        winners.resize_with(num_rounds, || BracketRound {
            sets: Vec::new(),
            kind: BracketRoundKind::Winners,
        });

        let mut racer_idx = 0;

        // Construct the winners first round and second round - in case this is a non-power of two
        // round.
        for i in 0..first_round_sets {
            let group_size = tracker
                .pop_group()
                .ok_or(TournamentError::InvalidGroupConfigurations)?;

            let mut bracket = BracketSet::new(races_per_set, group_size)?;
            bracket.add_racers(&racers[racer_idx..racer_idx + group_size])?;
            winners[0].sets.push(bracket_sets.insert(bracket));

            racer_idx += group_size;

            if i.is_multiple_of(2) && winners.len() > 1 {
                winners[1]
                    .sets
                    .push(bracket_sets.insert(BracketSet::new(races_per_set, MAX_RACERS)?));
            }
        }

        // Complete construction of the second round if the first round had a non-power of two sets.
        for _ in 0..second_round_sets {
            let group_size = tracker
                .pop_group()
                .ok_or(TournamentError::InvalidGroupConfigurations)?;

            let mut bracket = BracketSet::new(races_per_set, group_size)?;
            bracket.add_racers(&racers[racer_idx..racer_idx + group_size])?;
            winners[1].sets.push(bracket_sets.insert(bracket));

            racer_idx += group_size;
        }

        //
        // Construct the rest of the winners bracket rounds as empty.
        //

        for round in 2..num_rounds {
            let set_count = winners[round - 1].sets.len() / 2;
            for _ in 0..set_count {
                winners[round]
                    .sets
                    .push(bracket_sets.insert(BracketSet::new(races_per_set, MAX_RACERS)?));
            }
        }

        Ok(winners)
    }

    /// Builds the losers bracket, pre-built empty: one intake round per winners round
    /// (its droppers + prior survivors), plus consolidation rounds to shrink
    /// down to a single heat of finalists. All sizes are a function of the field.
    fn build_losers(
        winners: &[BracketRound],
        races_per_set: usize,
        bracket_sets: &mut SlotMap<BracketSetId, BracketSet>,
    ) -> Result<Vec<BracketRound>, TournamentError> {
        let mut losers = Vec::new();
        let mut carried: usize = 0;

        for (wb_round, round_sets) in winners.iter().enumerate() {
            let droppers: usize = round_sets
                .sets
                .iter()
                .map(|&id| bracket_sets[id].expected_size() - ADVANCERS_PER_SET)
                .sum();

            let sets = Self::split_losers_sets(carried + droppers, races_per_set, bracket_sets)?;

            carried = ADVANCERS_PER_SET * sets.len();
            losers.push(BracketRound {
                sets,
                kind: BracketRoundKind::LosersIntake { wb_round },
            });

            while carried > ADVANCERS_PER_SET {
                let sets = Self::split_losers_sets(carried, races_per_set, bracket_sets)?;
                carried = ADVANCERS_PER_SET * sets.len();
                losers.push(BracketRound {
                    sets,
                    kind: BracketRoundKind::LosersConsolidate,
                });
            }
        }

        debug_assert_eq!(
            carried, ADVANCERS_PER_SET,
            "losers bracket must reduce to exactly one heat of finalists"
        );

        Ok(losers)
    }

    /// Splits `pool` racers into empty losers-bracket sets of `4..=8`, fewest heats.
    fn split_losers_sets(
        pool: usize,
        races_per_set: usize,
        bracket_sets: &mut SlotMap<BracketSetId, BracketSet>,
    ) -> Result<Vec<BracketSetId>, TournamentError> {
        let mut tracker = RaceGroupTracker::new(pool, MIN_LOSERS_BRACKET_RACE_SIZE)?;
        let mut sets = Vec::with_capacity(tracker.get_group_count());
        while let Some(size) = tracker.pop_group() {
            sets.push(bracket_sets.insert(BracketSet::new(races_per_set, size)?));
        }
        Ok(sets)
    }
}

#[derive(Debug)]
pub struct BracketSetView {
    pub expected_size: usize,
    pub racers: Vec<ParticipantView>,
}

#[derive(Debug)]
pub struct BracketRoundView {
    /// For a losers round: `Some(r)` = intake fed by winners round `r`; `None` = consolidation.
    pub from_wb_round: Option<usize>,
    pub sets: Vec<BracketSetView>,
}

#[derive(Debug)]
pub struct BracketView {
    pub winners: Vec<BracketRoundView>,
    pub losers: Vec<BracketRoundView>,
}

impl Viewable<BracketSetView> for BracketSet {
    fn view(&self, id_map: &ParticipantMap) -> BracketSetView {
        // All races in a set share one roster; read it from the first race.
        let racers = self.races.first().map_or_else(Vec::new, |race| {
            race.get_racers().map(|id| id.view(id_map)).collect()
        });

        BracketSetView {
            expected_size: self.expected_size,
            racers,
        }
    }
}

impl Viewable<BracketView> for Bracket {
    fn view(&self, id_map: &ParticipantMap) -> BracketView {
        let round_view = |round: &BracketRound| BracketRoundView {
            from_wb_round: match round.kind {
                BracketRoundKind::LosersIntake { wb_round } => Some(wb_round),
                BracketRoundKind::Winners | BracketRoundKind::LosersConsolidate => None,
            },
            sets: round
                .sets
                .iter()
                .map(|&id| self.bracket_sets[id].view(id_map))
                .collect(),
        };

        BracketView {
            winners: self.winners.iter().map(&round_view).collect(),
            losers: self.losers.iter().map(&round_view).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use slotmap::SlotMap;

    fn make_participants(n: usize) -> Vec<ParticipantId> {
        let mut slots: SlotMap<ParticipantId, ()> = SlotMap::with_key();
        (0..n).map(|_| slots.insert(())).collect()
    }

    fn set_sizes(bracket: &Bracket, sets: &[BracketSetId]) -> Vec<usize> {
        sets.iter()
            .map(|&id| bracket.bracket_sets[id].expected_size())
            .collect()
    }

    fn lb_finalists(bracket: &Bracket) -> usize {
        ADVANCERS_PER_SET * bracket.losers.last().unwrap().sets.len()
    }

    #[test]
    fn losers_bracket_for_16_is_two_intake_rounds() {
        let bracket = Bracket::new(3, &make_participants(16)).unwrap();

        // Winners: two 8-heats, then a single 8-heat final.
        assert_eq!(set_sizes(&bracket, &bracket.winners[0].sets), vec![8, 8]);
        assert_eq!(set_sizes(&bracket, &bracket.winners[1].sets), vec![8]);

        // Losers: one intake round per winners round, each a single 8-heat, no consolidation.
        assert_eq!(bracket.losers.len(), 2);
        assert!(matches!(
            bracket.losers[0].kind,
            BracketRoundKind::LosersIntake { wb_round: 0 }
        ));
        assert!(matches!(
            bracket.losers[1].kind,
            BracketRoundKind::LosersIntake { wb_round: 1 }
        ));
        assert_eq!(set_sizes(&bracket, &bracket.losers[0].sets), vec![8]);
        assert_eq!(set_sizes(&bracket, &bracket.losers[1].sets), vec![8]);
        assert_eq!(lb_finalists(&bracket), 4);
    }

    #[test]
    fn losers_bracket_for_24_uses_a_consolidation_round() {
        let bracket = Bracket::new(3, &make_participants(24)).unwrap();

        // 24 -> 3 winners sets -> a depth-3 bracket that needs one minor round.
        let kinds: Vec<&BracketRoundKind> =
            bracket.losers.iter().map(|round| &round.kind).collect();
        assert!(matches!(
            kinds[0],
            BracketRoundKind::LosersIntake { wb_round: 0 }
        ));
        assert!(matches!(
            kinds[1],
            BracketRoundKind::LosersIntake { wb_round: 1 }
        ));
        assert!(matches!(kinds[2], BracketRoundKind::LosersConsolidate));
        assert!(matches!(
            kinds[3],
            BracketRoundKind::LosersIntake { wb_round: 2 }
        ));

        assert_eq!(lb_finalists(&bracket), 4);
    }

    #[test]
    fn every_supported_field_size_yields_four_lb_finalists() {
        for n in 12..=16 {
            let bracket = Bracket::new(3, &make_participants(n)).unwrap();
            assert_eq!(lb_finalists(&bracket), 4, "n = {n}");
        }
    }
}
