use crate::TournamentError::{self, NotEnoughParticipants};
use crate::race::MAX_RACERS;

/// Splits a set of participants into races whose sizes differ by at most one,
/// so every race stays as full as the size bounds allow. Shared by the pool
/// qualifier and the bracket, which pass different minimum race sizes.
#[derive(Debug)]
pub(crate) struct RaceGroupTracker {
    full_race_size: usize,
    full_race_count: usize,
    small_race_count: usize,
}

impl RaceGroupTracker {
    /// Builds a tracker that splits `total_participants` into races of size
    /// `min_race_size..=MAX_RACERS`, preferring the largest full size `t` that
    /// divides cleanly into races of size `t` and `t - 1`.
    pub(crate) fn new(
        total_participants: usize,
        min_race_size: usize,
    ) -> Result<Self, TournamentError> {
        for i in (min_race_size + 1..=MAX_RACERS).rev() {
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

    pub(crate) fn get_group_count(&self) -> usize {
        self.full_race_count + self.small_race_count
    }

    pub(crate) fn pop_group(&mut self) -> Option<usize> {
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
