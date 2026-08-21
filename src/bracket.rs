use std::collections::{HashMap, HashSet};

use slotmap::{SlotMap, new_key_type};

use crate::participant::{ParticipantMap, ParticipantView};
use crate::view::Viewable;
use crate::{
    ParticipantId, RaceRuleset, TournamentError,
    race::{MAX_RACERS, Race, RaceView},
    race_group::RaceGroupTracker,
};

new_key_type! { pub struct BracketSetId; }

#[derive(Debug)]
pub(crate) enum FeederSource {
    Winners,
    Losers,
}

#[derive(Debug)]
pub(crate) struct Feeder {
    id: BracketSetId,
    source: FeederSource,
}

#[derive(Debug)]
pub(crate) struct BracketSet {
    races: Vec<Race>,
    expected_size: usize,
    feeders: Vec<Feeder>,
}

impl BracketSet {
    fn new(
        num_races: usize,
        racer_count: usize,
        feeders: Vec<Feeder>,
    ) -> Result<Self, TournamentError> {
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
            feeders,
        })
    }

    fn racer_count(&self) -> usize {
        self.races[0].get_racers().count()
    }

    fn expected_size(&self) -> usize {
        self.expected_size
    }

    fn is_started(&self) -> bool {
        self.races.iter().any(|race| race.is_complete())
    }

    pub(crate) fn is_completed(&self) -> bool {
        self.is_ready() && self.races.iter().all(|race| race.is_complete())
    }

    fn is_ready(&self) -> bool {
        self.expected_size() == self.racer_count()
    }

    fn add_racers(&mut self, racers: &[ParticipantId]) -> Result<(), TournamentError> {
        if self.is_started() {
            Err(TournamentError::BracketSetAlreadyStarted)
        } else if self.racer_count() + racers.len() > self.expected_size {
            Err(TournamentError::RaceIsFull)
        } else {
            for race in self.races.iter_mut() {
                race.add_racers(racers)?
            }

            Ok(())
        }
    }

    fn clear_racers(&mut self) -> Result<(), TournamentError> {
        for race in self.races.iter_mut() {
            race.clear_racers()?
        }

        Ok(())
    }

    fn contains_racers(&self, racers: &[ParticipantId]) -> bool {
        let mut set: HashSet<_> = self.races[0].get_racers().collect();
        for id in racers {
            if !set.remove(id) {
                return false;
            }
        }

        set.is_empty()
    }

    fn current_race_index(&self) -> usize {
        self.races
            .iter()
            .take_while(|race| race.is_complete())
            .count()
    }

    pub(crate) fn race(&mut self, index: usize) -> Option<&mut Race> {
        self.races.get_mut(index)
    }

    fn get_scores(&self) -> Vec<(ParticipantId, usize)> {
        let mut map = HashMap::new();

        for race in self.races.iter() {
            for &(id, place) in race.get_racers_and_placements() {
                let points = place.map_or(0, |p| p.points());

                map.entry(id).and_modify(|e| *e += points).or_insert(points);
            }
        }

        map.into_iter().collect()
    }

    fn get_winners_losers(&self, winner_count: usize) -> (Vec<ParticipantId>, Vec<ParticipantId>) {
        if !self.is_completed() {
            return (vec![], vec![]);
        }

        let mut scores = self.get_scores();
        scores.sort_by(|(_, score_a), (_, score_b)| score_b.cmp(score_a));

        // TODO: if scores tie across the winner/loser boundary, run a Vanilla
        // tiebreaker race instead of splitting the tie arbitrarily.
        let mut scores = scores.into_iter();

        let winners = scores
            .by_ref()
            .take(winner_count)
            .map(|(id, _)| id)
            .collect();

        let losers = scores.map(|(id, _)| id).collect();

        (winners, losers)
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
pub(crate) struct Bracket {
    winners: Vec<BracketRound>,
    losers: Vec<BracketRound>,
    races_per_set: usize,
    bracket_sets: SlotMap<BracketSetId, BracketSet>,
}

impl Bracket {
    pub(crate) fn new(
        races_per_set: usize,
        racers: &[ParticipantId],
    ) -> Result<Self, TournamentError> {
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

    pub(crate) fn set(&mut self, id: BracketSetId) -> Result<&mut BracketSet, TournamentError> {
        let set = self
            .bracket_sets
            .get_mut(id)
            .ok_or(TournamentError::InvalidBracketId)?;

        if !set.is_ready() {
            return Err(TournamentError::BracketNotReady);
        }

        Ok(set)
    }

    fn active_set(&self) -> Option<BracketSetId> {
        for (id, set) in self.bracket_sets.iter() {
            if set.is_ready() && !set.is_completed() {
                return Some(id);
            }
        }

        None
    }

    pub(crate) fn advance(&mut self) -> Result<bool, TournamentError> {
        // Winners bracket, round by round: reseat each heat whose feeders changed.
        for round_idx in 0..self.winners.len() {
            for set_idx in 0..self.winners[round_idx].sets.len() {
                let Some(advancers) = self.updated_winners_set(round_idx, set_idx) else {
                    continue;
                };

                let set_id = self.winners[round_idx].sets[set_idx];
                let set = &mut self.bracket_sets[set_id];
                set.clear_racers()?;
                set.add_racers(&advancers)?;
            }
        }

        // TODO: advance the losers bracket, then return whether the bracket is complete.
        for round_idx in 0..self.losers.len() {
            let Some(round_pool) = self.updated_losers_round(round_idx) else {
                break;
            };

            // Update losers sets
        }

        Err(TournamentError::NotImplemented)
    }

    fn updated_losers_round(&self, round_idx: usize) -> Option<Vec<ParticipantId>> {
        todo!();
    }

    fn updated_winners_set(&self, round_idx: usize, set_idx: usize) -> Option<Vec<ParticipantId>> {
        let prev = self.winners.get(round_idx.checked_sub(1)?)?;
        let left = &self.bracket_sets[*prev.sets.get(2 * set_idx)?];
        let right = &self.bracket_sets[*prev.sets.get(2 * set_idx + 1)?];

        let (left_winners, _) = left.get_winners_losers(ADVANCERS_PER_SET);
        let (right_winners, _) = right.get_winners_losers(ADVANCERS_PER_SET);
        let advancers = [left_winners, right_winners].concat();

        // Already seeded with exactly these racers ⇒ nothing to do.
        let set = &self.bracket_sets[self.winners[round_idx].sets[set_idx]];
        (!set.contains_racers(&advancers)).then_some(advancers)
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

            let mut bracket = BracketSet::new(races_per_set, group_size, vec![])?;
            bracket.add_racers(&racers[racer_idx..racer_idx + group_size])?;
            winners[0].sets.push(bracket_sets.insert(bracket));

            racer_idx += group_size;

            if !i.is_multiple_of(2) {
                let feeders = vec![
                    Feeder {
                        id: winners[0].sets[i],
                        source: FeederSource::Winners,
                    },
                    Feeder {
                        id: winners[0].sets[i - 1],
                        source: FeederSource::Winners,
                    },
                ];

                winners[1].sets.push(bracket_sets.insert(BracketSet::new(
                    races_per_set,
                    MAX_RACERS,
                    feeders,
                )?));
            }
        }

        // Complete construction of the second round if the first round had a non-power of two sets.
        for _ in 0..second_round_sets {
            let group_size = tracker
                .pop_group()
                .ok_or(TournamentError::InvalidGroupConfigurations)?;

            let mut bracket = BracketSet::new(races_per_set, group_size, vec![])?;
            bracket.add_racers(&racers[racer_idx..racer_idx + group_size])?;
            winners[1].sets.push(bracket_sets.insert(bracket));

            racer_idx += group_size;
        }

        //
        // Construct the rest of the winners bracket rounds as empty.
        //

        for round in 2..num_rounds {
            let set_count = winners[round - 1].sets.len() / 2;
            for i in 0..set_count {
                let feeders = vec![
                    Feeder {
                        id: winners[round - 1].sets[2 * i],
                        source: FeederSource::Winners,
                    },
                    Feeder {
                        id: winners[round - 1].sets[2 * i + 1],
                        source: FeederSource::Winners,
                    },
                ];

                winners[round]
                    .sets
                    .push(bracket_sets.insert(BracketSet::new(
                        races_per_set,
                        MAX_RACERS,
                        feeders,
                    )?));
            }
        }

        Ok(winners)
    }

    /// Builds the losers bracket, pre-built empty: one intake round per winners round
    /// (its droppers + prior survivors), plus consolidation rounds to shrink down to a
    /// single heat of finalists. Every heat is fed by whole 4-groups, so a heat's roster is
    /// a function of its feeders, never of an ordering.
    fn build_losers(
        winners: &[BracketRound],
        races_per_set: usize,
        bracket_sets: &mut SlotMap<BracketSetId, BracketSet>,
    ) -> Result<Vec<BracketRound>, TournamentError> {
        let mut losers = Vec::new();

        // Survivor groups carried from the previous losers round; each is 4 racers.
        let mut carried: Vec<Feeder> = Vec::new();

        for (wb_round, round_sets) in winners.iter().enumerate() {
            // This winners round's droppers join the carried survivors as fresh feeders.
            // TODO: winners round-0 heats of 6/7 drop 2/3-racer groups; packing can leave a
            // sub-4 heat for those fields. v1's 16-racer field only ever drops 4s.
            let mut inputs = carried;
            inputs.extend(round_sets.sets.iter().map(|&id| Feeder {
                id,
                source: FeederSource::Losers,
            }));

            let sets = Self::pack_losers_sets(inputs, races_per_set, bracket_sets)?;
            carried = Self::survivor_feeders(&sets);
            losers.push(BracketRound {
                sets,
                kind: BracketRoundKind::LosersIntake { wb_round },
            });

            // Consolidate survivors until a single heat's worth remains.
            while carried.len() > 1 {
                let sets = Self::pack_losers_sets(carried, races_per_set, bracket_sets)?;
                carried = Self::survivor_feeders(&sets);
                losers.push(BracketRound {
                    sets,
                    kind: BracketRoundKind::LosersConsolidate,
                });
            }
        }

        debug_assert_eq!(
            carried.len(),
            1,
            "losers bracket must reduce to exactly one heat of finalists"
        );

        Ok(losers)
    }

    /// Packs whole feeder groups into empty losers heats of `4..=8`, filling toward 8 and
    /// keeping each source group intact.
    fn pack_losers_sets(
        groups: Vec<Feeder>,
        races_per_set: usize,
        bracket_sets: &mut SlotMap<BracketSetId, BracketSet>,
    ) -> Result<Vec<BracketSetId>, TournamentError> {
        let mut heats: Vec<(usize, Vec<Feeder>)> = Vec::new();
        let mut current: Vec<Feeder> = Vec::new();
        let mut current_size = 0;

        for group in groups {
            let size = Self::feeder_group_size(&group, bracket_sets);
            if !current.is_empty() && current_size + size > MAX_RACERS {
                heats.push((current_size, std::mem::take(&mut current)));
                current_size = 0;
            }

            current.push(group);
            current_size += size;
        }

        if !current.is_empty() {
            heats.push((current_size, current));
        }

        let mut sets = Vec::with_capacity(heats.len());
        for (size, feeders) in heats {
            debug_assert!(
                (MIN_LOSERS_BRACKET_RACE_SIZE..=MAX_RACERS).contains(&size),
                "losers heat size {size} out of range"
            );
            sets.push(bracket_sets.insert(BracketSet::new(races_per_set, size, feeders)?));
        }

        Ok(sets)
    }

    /// The survivors (top `ADVANCERS_PER_SET`) of each heat, as feeders for the next round.
    fn survivor_feeders(sets: &[BracketSetId]) -> Vec<Feeder> {
        sets.iter()
            .map(|&id| Feeder {
                id,
                source: FeederSource::Winners,
            })
            .collect()
    }

    /// Racers a feeder contributes: a heat's survivors are always `ADVANCERS_PER_SET`; its
    /// droppers are everyone else.
    fn feeder_group_size(
        feeder: &Feeder,
        bracket_sets: &SlotMap<BracketSetId, BracketSet>,
    ) -> usize {
        match feeder.source {
            FeederSource::Winners => ADVANCERS_PER_SET,
            FeederSource::Losers => bracket_sets[feeder.id].expected_size() - ADVANCERS_PER_SET,
        }
    }
}

#[derive(Debug)]
pub struct BracketSetView {
    pub expected_size: usize,
    pub racers: Vec<ParticipantView>,
    pub races: Vec<RaceView>,
    /// Index of the next race to run; races before it are complete.
    pub current_race_index: usize,
    /// Whether the heat is fully seeded and can accept results.
    pub is_ready: bool,
}

#[derive(Debug)]
pub struct BracketRoundView {
    /// For a losers round: `Some(r)` = intake fed by winners round `r`; `None` = consolidation.
    pub from_wb_round: Option<usize>,
    pub sets: Vec<(BracketSetId, BracketSetView)>,
}

#[derive(Debug)]
pub struct BracketView {
    pub winners: Vec<BracketRoundView>,
    pub losers: Vec<BracketRoundView>,
    pub active_set: Option<BracketSetId>,
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
            races: self.races.iter().map(|race| race.view(id_map)).collect(),
            current_race_index: self.current_race_index(),
            is_ready: self.is_ready(),
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
                .map(|&id| (id, self.bracket_sets[id].view(id_map)))
                .collect(),
        };

        BracketView {
            winners: self.winners.iter().map(&round_view).collect(),
            losers: self.losers.iter().map(&round_view).collect(),
            active_set: self.active_set(),
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
