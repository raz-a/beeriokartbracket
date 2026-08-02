use std::cmp::{Ordering, Reverse};
use std::collections::HashMap;
use std::num::NonZero;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

use crate::TournamentError::{self, NotEnoughParticipants};
use crate::participant::{ParticipantId, ParticipantScore, ParticipantView};
use crate::race::{MAX_RACERS, Race, RaceRuleset, RaceView};
use crate::view::{ParticipantMap, Viewable};

/// Pool races are 6-, 7-, or 8-player (per the tournament rules). The tracker
/// splits a bucket into races of size `t` and `t - 1`, so the smallest full
/// race size `t` we allow is one above this floor; counts that cannot be
/// covered by `t` in `{7, 8}` are rejected.
const MIN_POOL_RACE_SIZE: usize = 6;

#[derive(Debug)]
struct RaceGroupTracker {
    full_race_size: usize,
    full_race_count: usize,
    small_race_count: usize,
}

impl RaceGroupTracker {
    fn new(total_participants: usize) -> Result<Self, TournamentError> {
        for i in (MIN_POOL_RACE_SIZE + 1..=MAX_RACERS).rev() {
            if let Some(groups) = Self::possible_race_groups(total_participants, i) {
                return Ok(Self {
                    full_race_size: i,
                    full_race_count: groups.0,
                    small_race_count: groups.1,
                });
            }
        }

        Err(NotEnoughParticipants)
    }

    fn pop_group(&mut self) -> Option<usize> {
        if self.full_race_count > 0 {
            self.full_race_count -= 1;
            Some(self.full_race_size)
        } else if self.small_race_count > 0 {
            self.small_race_count -= 1;
            Some(self.full_race_size - 1)
        } else {
            None
        }
    }

    /// Splits `total_participants` (`N`) into races of `target_groupsize` (`t`)
    /// and `target_groupsize - 1` (`t - 1`) with nobody left over.
    ///
    /// Returns `Some((a, b))`, where `a` is the number of `t`-participant races
    /// and `b` the number of `(t - 1)`-participant races, or `None` when no such
    /// split exists.
    ///
    /// # Derivation
    ///
    /// We need non-negative integers `a` and `b` satisfying:
    ///
    /// ```text
    /// t*a + (t - 1)*b = N
    /// ```
    ///
    /// Introduce the total race count `g = a + b`. Factoring the left-hand side
    /// two ways expresses both `a` and `b` in terms of `g` alone:
    ///
    /// ```text
    /// t*a + (t - 1)*b = (t - 1)*g + a   =>   a = N - (t - 1)*g
    /// t*a + (t - 1)*b = t*g - b         =>   b = t*g - N
    /// ```
    ///
    /// Requiring `a >= 0` and `b >= 0` bounds `g`:
    ///
    /// ```text
    /// a >= 0   =>   g <= N / (t - 1)
    /// b >= 0   =>   g >= N / t
    /// ```
    ///
    /// So any integer `g` in `ceil(N / t) ..= floor(N / (t - 1))` gives a valid
    /// split. We pick the smallest `g` to use the fewest races and keep each as
    /// full as possible.
    fn possible_race_groups(
        total_participants: usize,
        target_groupsize: usize,
    ) -> Option<(usize, usize)> {
        let one_less_groupsize = target_groupsize - 1;

        // Bounds on the total race count, g (see the derivation above).
        let min_groups = total_participants.div_ceil(target_groupsize);
        let max_groups = total_participants / one_less_groupsize;
        if min_groups > max_groups {
            return None;
        }

        // Fewest races, so each race is as full as possible.
        let groups = min_groups;
        let full_races = total_participants - one_less_groupsize * groups;
        let short_races = target_groupsize * groups - total_participants;

        Some((full_races, short_races))
    }
}

#[derive(Debug, Default)]
struct FillingBucket {
    participants: Vec<ParticipantId>,
}

impl FillingBucket {
    fn push_participants(&mut self, participants: &[ParticipantId]) {
        self.participants.extend_from_slice(participants);
    }

    fn len(&self) -> usize {
        self.participants.len()
    }

    fn seal(self) -> Result<DrainingBucket, TournamentError> {
        let tracker = RaceGroupTracker::new(self.participants.len())?;
        Ok(DrainingBucket {
            participants: self.participants,
            tracker,
        })
    }
}

#[derive(Debug)]
struct DrainingBucket {
    participants: Vec<ParticipantId>,
    tracker: RaceGroupTracker,
}

impl DrainingBucket {
    fn is_empty(&self) -> bool {
        self.participants.is_empty()
    }

    fn pop_next_race_candidate(&mut self, rng: &mut impl Rng) -> Option<Vec<ParticipantId>> {
        let group_size = self.tracker.pop_group()?;

        let len = self.participants.len();

        // Shuffle the participants list and pop off the next `group_size` participants.
        let _ = self.participants.partial_shuffle(rng, group_size);

        Some(self.participants.split_off(len - group_size))
    }
}

pub struct PoolResult {
    locked: Vec<ParticipantScore>,
    tied: Vec<ParticipantScore>,
    eliminated: Vec<ParticipantScore>,
    tiebreaker_seats: usize,
}

#[derive(Debug)]
pub(crate) struct Pool {
    current_bucket: DrainingBucket,
    next_bucket: FillingBucket,
    current_round: usize,
    max_round: usize,
    number_of_participants: usize,
    current_race: Option<Race>,
    completed_races: Vec<Race>,
    rng: StdRng,
}

impl Pool {
    pub fn new(
        target_rounds: usize,
        participants: &[ParticipantId],
        seed: u64,
    ) -> Result<Self, TournamentError> {
        let mut first_bucket = FillingBucket::default();
        first_bucket.push_participants(participants);

        let first_bucket = first_bucket.seal()?;

        Ok(Pool {
            current_bucket: first_bucket,
            next_bucket: Default::default(),
            current_round: 0,
            max_round: target_rounds,
            number_of_participants: participants.len(),
            current_race: Default::default(),
            completed_races: Default::default(),
            rng: StdRng::seed_from_u64(seed),
        })
    }

    pub fn advance(&mut self) -> Result<bool, TournamentError> {
        // Record the current in the completed races.
        if let Some(race) = self.current_race.take() {
            if !race.is_complete() {
                self.current_race.replace(race);
                return Err(TournamentError::RaceIsNotComplete);
            }

            // Move the completed race participants to the next bucket.
            let participants: Vec<ParticipantId> = race.get_racers().collect();
            self.next_bucket.push_participants(&participants);
            self.completed_races.push(race);
        }

        debug_assert!(self.current_race.is_none());

        while !self.is_complete() {
            if let Some(racers) = self.current_bucket.pop_next_race_candidate(&mut self.rng) {
                let ruleset = self.get_current_ruleset();
                let race = self.current_race.insert(Race::default());
                race.set_ruleset(ruleset);
                race.add_racers(&racers)
                    .expect("Race was just created and shouldn't have any collisions or overflow");

                return Ok(false);
            }

            debug_assert!(self.current_bucket.is_empty());
            debug_assert_eq!(
                self.next_bucket.len(),
                self.number_of_participants,
                "a bucket must hold all participants before it is sealed"
            );

            self.current_bucket = std::mem::take(&mut self.next_bucket).seal().expect(
                "Next bucket is full with all participants. Bucket sealing should always succeed.",
            );

            self.current_round += 1;
        }

        Ok(true)
    }

    pub fn active_race(&mut self) -> Option<&mut Race> {
        self.current_race.as_mut()
    }

    pub fn is_complete(&self) -> bool {
        self.current_round >= self.max_round
    }

    pub fn get_results(&self, rank: usize) -> Option<PoolResult> {
        if !self.is_complete() {
            return None;
        }

        let mut scores = HashMap::new();

        for race in self.completed_races.iter() {
            debug_assert!(race.is_complete());

            for &(racer, place) in race.get_racers_and_placements() {
                let points = place
                    .expect("Race is complete so placement is valid")
                    .points();

                scores
                    .entry(racer)
                    .and_modify(|score| *score += points)
                    .or_insert(points);
            }
        }

        let mut scores: Vec<ParticipantScore> =
            scores.into_iter().map(ParticipantScore::from).collect();

        scores.sort_by_key(|p| Reverse(p.get_score()));

        let rank = NonZero::new(rank.min(scores.len()))?.get();

        let cutoff_index = rank - 1;
        let cutoff_score = scores[cutoff_index].get_score();

        let (mut locked, mut tied, eliminated) = scores.into_iter().fold(
            (Vec::new(), Vec::new(), Vec::new()),
            |(mut locked, mut tied, mut eliminated), racer| {
                match racer.get_score().cmp(&cutoff_score) {
                    Ordering::Greater => locked.push(racer),
                    Ordering::Equal => tied.push(racer),
                    Ordering::Less => eliminated.push(racer),
                };
                (locked, tied, eliminated)
            },
        );

        let mut tiebreaker_seats = rank - locked.len();
        if tiebreaker_seats == tied.len() {
            locked.append(&mut tied);
            tiebreaker_seats = 0;
        } else {
            debug_assert!(tiebreaker_seats < tied.len());
        }

        Some(PoolResult {
            locked,
            tied,
            eliminated,
            tiebreaker_seats,
        })
    }

    fn get_current_ruleset(&self) -> RaceRuleset {
        if self.current_round.is_multiple_of(2) {
            RaceRuleset::Beerio
        } else {
            RaceRuleset::Vanilla
        }
    }
}

#[derive(Debug)]
pub struct PoolView {
    pub current_round: usize,
    pub max_rounds: usize,
    pub completed_races: Vec<RaceView>,
    pub current_race: Option<RaceView>,
    pub remaining_racers_in_round: Vec<ParticipantView>,
    pub completed_racers_in_round: Vec<ParticipantView>,
}

impl Viewable<PoolView> for Pool {
    fn view(&self, id_map: &ParticipantMap) -> PoolView {
        PoolView {
            current_round: self.current_round,
            max_rounds: self.max_round,
            completed_races: self
                .completed_races
                .iter()
                .map(|race| race.view(id_map))
                .collect(),
            current_race: self.current_race.as_ref().map(|race| race.view(id_map)),
            remaining_racers_in_round: self
                .current_bucket
                .participants
                .iter()
                .map(|racer| racer.view(id_map))
                .collect(),
            completed_racers_in_round: self
                .next_bucket
                .participants
                .iter()
                .map(|racer| racer.view(id_map))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::participant::ParticipantId;
    use crate::race::Placement;
    use slotmap::SlotMap;
    use std::collections::{HashMap, HashSet};

    /// Creates `n` real `ParticipantId`s via a throwaway `SlotMap`.
    fn make_participants(n: usize) -> Vec<ParticipantId> {
        let mut slots: SlotMap<ParticipantId, ()> = SlotMap::with_key();
        (0..n).map(|_| slots.insert(())).collect()
    }

    /// Fills every racer's placement so the race reports complete. The concrete
    /// placements are irrelevant to bucket progression.
    fn complete_race(race: &mut Race) {
        let racers: Vec<ParticipantId> = race.get_racers().collect();
        for (i, id) in racers.iter().enumerate() {
            race.set_placement(*id, Placement::new((i + 1) as u8).unwrap())
                .unwrap();
        }
    }

    #[test]
    fn possible_race_groups_splits_into_eights_and_sevens() {
        // (participants, Some((full 8-player races, short 7-player races)))
        let cases = [
            (7, Some((0, 1))),  // a single short race
            (8, Some((1, 0))),  // a single full race
            (14, Some((0, 2))), // two short races
            (15, Some((1, 1))), // one of each
            (16, Some((2, 0))), // two full races
            (30, Some((2, 2))),
            (56, Some((7, 0))), // g may be 7 or 8; smallest g keeps races fullest
        ];

        for (total, expected) in cases {
            assert_eq!(
                RaceGroupTracker::possible_race_groups(total, 8),
                expected,
                "total = {total}"
            );
        }
    }

    #[test]
    fn possible_race_groups_rejects_unsplittable_counts() {
        // No combination of 7s and 8s sums to these.
        for total in [6, 13] {
            assert_eq!(
                RaceGroupTracker::possible_race_groups(total, 8),
                None,
                "total = {total}"
            );
        }
    }

    #[test]
    fn pool_rejects_counts_that_cannot_form_legal_races() {
        // 10 racers can't be split into only 6/7/8-player races.
        assert!(RaceGroupTracker::new(10).is_err());
        assert!(Pool::new(8, &make_participants(10), 0).is_err());

        // Boundary counts that *can* form legal races.
        assert!(RaceGroupTracker::new(6).is_ok()); // a single 6-player race
        assert!(RaceGroupTracker::new(8).is_ok()); // a single 8-player race
    }

    #[test]
    fn pool_runs_until_every_racer_reaches_the_top_bucket() {
        let ids = make_participants(16);
        let mut pool = Pool::new(8, &ids, 42).unwrap();

        let mut appearances: HashMap<ParticipantId, usize> = HashMap::new();
        while !pool.advance().unwrap() {
            let race = pool
                .active_race()
                .expect("advance() == false guarantees an active race");
            for id in race.get_racers() {
                *appearances.entry(id).or_default() += 1;
            }
            complete_race(race);
        }

        // Each racer runs exactly `target_races` (8) races: one per bucket 0..8.
        assert_eq!(appearances.len(), ids.len());
        assert!(appearances.values().all(|&count| count == 8));

        // 16 racers / 8 per race * 8 buckets = 16 completed races.
        assert_eq!(pool.completed_races.len(), 16);
    }

    #[test]
    fn pool_races_stay_within_legal_size_bounds() {
        let ids = make_participants(15);
        let mut pool = Pool::new(8, &ids, 7).unwrap();

        while !pool.advance().unwrap() {
            let race = pool
                .active_race()
                .expect("advance() == false guarantees an active race");
            let size = race.get_racers().count();
            assert!(
                (6..=8).contains(&size),
                "race size {size} outside the legal 6..=8 range"
            );
            complete_race(race);
        }
    }

    #[test]
    fn bucket_selection_is_seed_reproducible_and_seed_sensitive() {
        let ids = make_participants(16);

        // Draw the first 8-racer group for a given RNG seed.
        let draw = |seed: u64| -> Vec<ParticipantId> {
            let mut filling = FillingBucket::default();
            filling.push_participants(&ids);
            let mut draining = filling.seal().unwrap();
            let mut rng = StdRng::seed_from_u64(seed);
            draining.pop_next_race_candidate(&mut rng).unwrap()
        };

        // Same seed reproduces the same draw.
        assert_eq!(draw(1), draw(1));

        // Different seeds select a different set of opponents (not a reordering).
        let opponents = |seed| -> HashSet<ParticipantId> { draw(seed).into_iter().collect() };
        assert_ne!(opponents(1), opponents(2));
    }

    #[test]
    fn bucket_selection_can_reach_every_participant() {
        // Guards against a structural bias (e.g. always taking a fixed end of
        // the vec) by checking every racer is selectable across many seeds.
        let ids = make_participants(16);

        let mut seen: HashSet<ParticipantId> = HashSet::new();
        for seed in 0..64 {
            let mut filling = FillingBucket::default();
            filling.push_participants(&ids);
            let mut draining = filling.seal().unwrap();
            let mut rng = StdRng::seed_from_u64(seed);
            seen.extend(draining.pop_next_race_candidate(&mut rng).unwrap());
        }

        assert_eq!(
            seen.len(),
            ids.len(),
            "every participant should be selectable"
        );
    }
}
