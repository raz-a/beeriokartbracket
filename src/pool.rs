use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

use crate::participant::ParticipantId;
use crate::race::{MAX_RACERS, Race, RaceRuleset};

#[derive(Debug)]
pub struct RaceGroupTracker {
    full_race_size: usize,
    full_race_count: usize,
    small_race_count: usize,
}

impl RaceGroupTracker {
    pub fn new(total_participants: usize) -> Option<Self> {
        for i in (2..=MAX_RACERS).rev() {
            if let Some(groups) = Self::possible_race_groups(total_participants, i) {
                return Some(Self {
                    full_race_size: i,
                    full_race_count: groups.0,
                    small_race_count: groups.1,
                });
            }
        }

        None
    }

    pub fn pop_group(&mut self) -> Option<usize> {
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

#[derive(Debug)]
pub struct Bucket {
    participants: Vec<ParticipantId>,
    tracker: RaceGroupTracker,
}

impl Bucket {
    pub fn new(max_participants: usize) -> Option<Self> {
        RaceGroupTracker::new(max_participants).map(|tracker| Self {
            participants: Vec::with_capacity(max_participants),
            tracker,
        })
    }

    pub fn push_participants(&mut self, participants: &[ParticipantId]) {
        self.participants.extend_from_slice(participants);
    }

    pub fn pop_next_race_candidate(&mut self, rng: &mut impl Rng) -> Option<Vec<ParticipantId>> {
        let group_size = self.tracker.pop_group()?;

        let len = self.participants.len();

        // Shuffle the participants list and pop off the next `group_size` participants.
        let _ = self.participants.partial_shuffle(rng, group_size);

        Some(self.participants.split_off(len - group_size))
    }
}

#[derive(Debug)]
pub struct Pool {
    buckets: Box<[Bucket]>,
    current_bucket: usize,
    races: Vec<Race>,
    rng: StdRng,
    // TODO: Add an Option "CurrentRace" that will hold the race currently being run in pools.
    // TODO Add a getCurrentRace function to get a reference to the race.
    // TODO Add a complete race function that moves the racers to the next bucket and moves the current race to the races vec.
}

impl Pool {
    pub fn new(target_races: usize, participants: &[ParticipantId], seed: u64) -> Option<Self> {
        // The number of races determines the number of buckets.
        let mut buckets = vec![];
        buckets.resize_with(target_races + 1, || Bucket::new(participants.len()));

        let mut buckets = buckets
            .into_iter()
            .collect::<Option<Vec<Bucket>>>()?
            .into_boxed_slice();

        // Put all racers in the first bucket.
        buckets[0].push_participants(participants);

        Some(Pool {
            buckets,
            current_bucket: 0,
            races: Default::default(),
            rng: StdRng::seed_from_u64(seed),
        })
    }

    pub fn create_next_race(&mut self) -> Option<Race> {
        while self.current_bucket < self.buckets.len() - 1 {
            if let Some(racers) =
                self.buckets[self.current_bucket].pop_next_race_candidate(&mut self.rng)
            {
                let mut race = Race::default();

                race.set_ruleset(self.get_current_ruleset());
                race.add_racers(&racers)
                    .expect("Race was just created and shouldn't have any collisions or overflow");

                return Some(race);
            }

            self.current_bucket += 1;
        }

        None
    }

    fn get_current_ruleset(&self) -> RaceRuleset {
        if self.current_bucket.is_multiple_of(2) {
            RaceRuleset::Beerio
        } else {
            RaceRuleset::Vanilla
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
