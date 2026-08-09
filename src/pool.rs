use std::cmp::{Ordering, Reverse};
use std::collections::HashMap;
use std::num::NonZero;

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

use crate::TournamentError;
use crate::participant::{ParticipantId, ParticipantMap, ParticipantScore, ParticipantView};
use crate::race::{MAX_RACERS, Race, RaceId, RaceMap, RaceRuleset, RaceView};
use crate::race_group::RaceGroupTracker;
use crate::view::Viewable;

/// Pool races are 6-, 7-, or 8-player (per the tournament rules). The tracker
/// splits a bucket into races of size `t` and `t - 1`, so the smallest full
/// race size `t` we allow is one above this floor; counts that cannot be
/// covered by `t` in `{7, 8}` are rejected.
const MIN_POOL_RACE_SIZE: usize = 6;

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
        let tracker = RaceGroupTracker::new(self.participants.len(), MIN_POOL_RACE_SIZE)?;
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

#[derive(Debug)]
pub struct PoolResult {
    advanced: Vec<ParticipantScore>,
    eliminated: Vec<ParticipantScore>,
}

#[derive(Debug)]
pub(crate) struct Pool {
    current_bucket: DrainingBucket,
    next_bucket: FillingBucket,
    current_round: usize,
    max_round: usize,
    number_of_participants: usize,
    current_race: Option<Race>,
    completed_races: RaceMap,
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
            self.completed_races.insert(race);
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

    pub fn completed_race(&mut self, id: RaceId) -> Option<&mut Race> {
        self.completed_races.get_mut(id)
    }

    pub fn is_complete(&self) -> bool {
        self.current_round >= self.max_round
    }

    pub fn get_results(&self, rank: usize) -> Option<PoolResult> {
        if !self.is_complete() {
            return None;
        }

        let mut scores = HashMap::new();

        for (_, race) in self.completed_races.iter() {
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

        // Determine the cutoff score. Any racer with a score less than this is guaranteed eliminated.
        // Any racer with a score higher than this is guaranteed to have advanced.
        let cutoff_index = rank - 1;
        let cutoff_score = scores[cutoff_index].get_score();

        let (mut advanced, mut tied, mut eliminated) = scores.into_iter().fold(
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

        // If there are any tied participants in open seats, then attempt to fill the seats using
        // countback-tiers.

        let mut countback_profile: HashMap<ParticipantId, [usize; MAX_RACERS]> = tied
            .iter()
            .map(|participant| (participant.get_id(), [0; MAX_RACERS]))
            .collect();

        for (_, race) in &self.completed_races {
            for (racer, placement) in race.get_racers_and_placements() {
                let placement = placement.expect("Race is complete so placement is valid");

                if let Some(entry) = countback_profile.get_mut(racer) {
                    entry[placement.placement_idx() as usize] += 1;
                }
            }
        }

        tied.sort_by(|a, b| {
            countback_profile
                .get(&a.get_id())
                .unwrap()
                .cmp(countback_profile.get(&b.get_id()).unwrap())
                .reverse()
        });

        let tiebreaker_seats = rank - advanced.len();
        let mut tied = tied.into_iter();
        advanced.extend(tied.by_ref().take(tiebreaker_seats));
        eliminated.extend(tied);

        Some(PoolResult {
            advanced,
            eliminated,
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
    pub completed_races: Vec<(RaceId, RaceView)>,
    pub current_race: Option<RaceView>,
    pub remaining_racers_in_round: Vec<ParticipantView>,
    pub completed_racers_in_round: Vec<ParticipantView>,
}

impl PoolView {
    pub fn is_complete(&self) -> bool {
        self.current_round >= self.max_rounds
    }
}

impl Viewable<PoolView> for Pool {
    fn view(&self, id_map: &ParticipantMap) -> PoolView {
        PoolView {
            current_round: self.current_round,
            max_rounds: self.max_round,
            completed_races: self
                .completed_races
                .iter()
                .map(|(id, race)| (id, race.view(id_map)))
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

#[derive(Debug)]
pub struct PoolResultView {
    pub advanced: Vec<(ParticipantView, usize)>,
    pub eliminated: Vec<(ParticipantView, usize)>,
}

impl Viewable<PoolResultView> for PoolResult {
    fn view(&self, id_map: &ParticipantMap) -> PoolResultView {
        let to_views = |scores: &[ParticipantScore]| -> Vec<(ParticipantView, usize)> {
            let mut views: Vec<(ParticipantView, usize)> = scores
                .iter()
                .map(|s| (s.get_id().view(id_map), s.get_score()))
                .collect();
            // Highest score first, then by name, for a stable display order.
            views.sort_by(|(a, a_score), (b, b_score)| {
                b_score.cmp(a_score).then_with(|| a.name.cmp(&b.name))
            });
            views
        };

        PoolResultView {
            advanced: to_views(&self.advanced),
            eliminated: to_views(&self.eliminated),
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
            race.set_placement(*id, Some(Placement::new((i + 1) as u8).unwrap()))
                .unwrap();
        }
    }

    /// Builds a completed race from `(racer, placement)` pairs.
    fn make_race(results: &[(ParticipantId, u8)]) -> Race {
        let mut race = Race::default();
        let ids: Vec<ParticipantId> = results.iter().map(|&(id, _)| id).collect();
        race.add_racers(&ids).unwrap();
        for &(id, place) in results {
            race.set_placement(id, Some(Placement::new(place).unwrap()))
                .unwrap();
        }
        race
    }

    /// A pool marked complete holding just `races`; bucket state is irrelevant to `get_results`.
    fn completed_pool(races: Vec<Race>) -> Pool {
        let mut pool = Pool::new(1, &make_participants(8), 0).unwrap();
        pool.current_round = pool.max_round;
        for race in races {
            pool.completed_races.insert(race);
        }
        pool
    }

    fn id_set(scores: &[ParticipantScore]) -> HashSet<ParticipantId> {
        scores.iter().map(|s| s.get_id()).collect()
    }

    #[test]
    fn countback_orders_by_most_wins() {
        let ids = make_participants(3);
        let (a, b, c) = (ids[0], ids[1], ids[2]);

        // All three total 14 points, but B finished 2nd twice (no wins) while
        // A and C each took a 1st — so B loses the count-back for the last seats.
        let pool = completed_pool(vec![
            make_race(&[(a, 1), (b, 2), (c, 3)]),
            make_race(&[(a, 3), (b, 2), (c, 1)]),
        ]);

        let result = pool.get_results(2).unwrap();
        assert_eq!(id_set(&result.advanced), HashSet::from([a, c]));
        assert_eq!(id_set(&result.eliminated), HashSet::from([b]));
    }

    #[test]
    fn countback_uses_deeper_positions_when_wins_tie() {
        let ids = make_participants(4);
        let (x, y, p, q) = (ids[0], ids[1], ids[2], ids[3]);

        // P advances on score (22), Q is out on score (16). X and Y both total
        // 20 with one win each, but X has a 2nd and Y doesn't — X takes the seat.
        let pool = completed_pool(vec![
            make_race(&[(x, 1), (y, 3), (p, 2), (q, 4)]),
            make_race(&[(x, 2), (y, 3), (p, 1), (q, 4)]),
            make_race(&[(x, 4), (y, 1), (p, 2), (q, 3)]),
        ]);

        let result = pool.get_results(2).unwrap();
        assert_eq!(id_set(&result.advanced), HashSet::from([p, x]));
        assert_eq!(id_set(&result.eliminated), HashSet::from([q, y]));
    }

    #[test]
    fn tied_group_all_advances_when_seats_match() {
        let ids = make_participants(3);
        let (a, b, c) = (ids[0], ids[1], ids[2]);

        // Three-way tie at 14; with 3 seats, count-back eliminates no one.
        let pool = completed_pool(vec![
            make_race(&[(a, 1), (b, 2), (c, 3)]),
            make_race(&[(a, 3), (b, 2), (c, 1)]),
        ]);

        let result = pool.get_results(3).unwrap();
        assert_eq!(id_set(&result.advanced), HashSet::from([a, b, c]));
        assert!(result.eliminated.is_empty());
    }

    #[test]
    fn pool_rejects_counts_that_cannot_form_legal_races() {
        // 10 racers can't be split into only 6/7/8-player races.
        assert!(RaceGroupTracker::new(10, MIN_POOL_RACE_SIZE).is_err());
        assert!(Pool::new(8, &make_participants(10), 0).is_err());

        // Boundary counts that *can* form legal races.
        assert!(RaceGroupTracker::new(6, MIN_POOL_RACE_SIZE).is_ok()); // a single 6-player race
        assert!(RaceGroupTracker::new(8, MIN_POOL_RACE_SIZE).is_ok()); // a single 8-player race
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
