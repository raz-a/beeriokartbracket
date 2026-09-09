use std::collections::HashMap;

use slotmap::{SlotMap, new_key_type};

use crate::participant::{ParticipantMap, ParticipantView};
use crate::view::Viewable;
use crate::{
    ParticipantId, Placement, RaceRuleset, TournamentError,
    race::{MAX_RACERS, Race, RaceView},
    race_group::RaceGroupTracker,
};

new_key_type! { pub struct BracketSetId; }

#[derive(Debug, Clone, Copy)]
pub enum FeederSource {
    Winners,
    Losers,
}

struct Feeder {
    id: BracketSetId,
    source: FeederSource,
}

pub(crate) struct BracketSet {
    races: Vec<Race>,
    resolution: Option<BracketResolution>,
    expected_size: usize,
    feeders: Vec<Feeder>,
}

enum BracketResolution {
    Decided {
        winners: Vec<ParticipantId>,
        losers: Vec<ParticipantId>,
    },
    Tiebreak {
        race: Race,
        open_seats: usize,
        locked_winners: Vec<ParticipantId>,
        locked_losers: Vec<ParticipantId>,
    },
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
            resolution: None,
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
        if !self.is_ready() || !self.races.iter().all(|race| race.is_complete()) {
            return false;
        }

        match &self.resolution {
            Some(BracketResolution::Decided { .. }) => true,
            Some(BracketResolution::Tiebreak {
                race, open_seats, ..
            }) => Self::tiebreaker_is_resolved(race, *open_seats),
            None => false,
        }
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

    fn clear_racers(&mut self) {
        for race in self.races.iter_mut() {
            race.clear_racers()
        }
        self.resolution = None;
    }

    fn contains_racers(&self, racers: &[ParticipantId]) -> bool {
        self.races[0].contains_racers(racers)
    }

    fn current_race_index(&self) -> usize {
        let completed_regulation_races = self
            .races
            .iter()
            .take_while(|race| race.is_complete())
            .count();

        let tiebreaker_resolved = matches!(
            &self.resolution,
            Some(BracketResolution::Tiebreak {
                race,
                open_seats,
                ..
            }) if Self::tiebreaker_is_resolved(race, *open_seats)
        );

        if completed_regulation_races == self.races.len() && tiebreaker_resolved {
            completed_regulation_races + 1
        } else {
            completed_regulation_races
        }
    }

    pub(crate) fn race(&mut self, index: usize) -> Option<&mut Race> {
        if index < self.races.len() {
            self.resolution = None;
            self.races.get_mut(index)
        } else if index == self.races.len() {
            match &mut self.resolution {
                Some(BracketResolution::Tiebreak { race, .. }) => Some(race),
                _ => None,
            }
        } else {
            None
        }
    }

    fn get_scores(&self) -> Vec<(ParticipantId, usize)> {
        let mut totals = HashMap::new();

        for race in self.races.iter() {
            for &(id, place) in race.get_racers_and_placements() {
                let points = place.map_or(0, |p| p.points());

                totals
                    .entry(id)
                    .and_modify(|e| *e += points)
                    .or_insert(points);
            }
        }

        self.races[0]
            .get_racers()
            .map(|id| (id, totals[&id]))
            .collect()
    }

    fn resolve_regulation(&self, winner_count: usize) -> Option<BracketResolution> {
        let mut scores = self.get_scores();
        scores.sort_by(|(_, score_a), (_, score_b)| score_b.cmp(score_a));

        let cutoff_score = scores.get(winner_count.checked_sub(1)?)?.1;
        let mut locked_winners = Vec::new();
        let mut contenders = Vec::new();
        let mut locked_losers = Vec::new();

        for (id, score) in scores {
            match score.cmp(&cutoff_score) {
                std::cmp::Ordering::Greater => locked_winners.push(id),
                std::cmp::Ordering::Equal => contenders.push(id),
                std::cmp::Ordering::Less => locked_losers.push(id),
            }
        }

        let open_seats = winner_count - locked_winners.len();
        if contenders.len() == open_seats {
            locked_winners.extend(contenders);
            Some(BracketResolution::Decided {
                winners: locked_winners,
                losers: locked_losers,
            })
        } else {
            let mut race = Race::default();
            race.set_ruleset(RaceRuleset::Vanilla);
            race.add_racers(&contenders).ok()?;
            Some(BracketResolution::Tiebreak {
                race,
                open_seats,
                locked_winners,
                locked_losers,
            })
        }
    }

    fn tiebreaker_is_resolved(race: &Race, open_seats: usize) -> bool {
        if !race.is_complete() {
            return false;
        }

        let mut placements: Vec<_> = race
            .get_racers_and_placements()
            .iter()
            .map(|(_, placement)| placement.unwrap())
            .collect();
        placements.sort_by_key(Placement::placement);

        placements[open_seats - 1] != placements[open_seats]
    }

    fn prepare_resolution(&mut self, winner_count: usize) -> Result<(), TournamentError> {
        if !self.is_ready() || !self.races.iter().all(|race| race.is_complete()) {
            self.resolution = None;
            return Ok(());
        }

        if self.resolution.is_none() {
            self.resolution = self.resolve_regulation(winner_count);
        }

        if matches!(
            &self.resolution,
            Some(BracketResolution::Tiebreak {
                race,
                open_seats,
                ..
            }) if race.is_complete() && !Self::tiebreaker_is_resolved(race, *open_seats)
        ) {
            return Err(TournamentError::BracketTiebreakUnresolved);
        }

        Ok(())
    }

    fn get_winners_losers(&self, _winner_count: usize) -> (Vec<ParticipantId>, Vec<ParticipantId>) {
        if !self.is_completed() {
            return (vec![], vec![]);
        }

        match self.resolution.as_ref().unwrap() {
            BracketResolution::Decided { winners, losers } => (winners.clone(), losers.clone()),
            BracketResolution::Tiebreak {
                race,
                open_seats,
                locked_winners,
                locked_losers,
            } => {
                let mut tiebreak_results: Vec<_> = race
                    .get_racers_and_placements()
                    .iter()
                    .map(|(id, placement)| (*id, placement.unwrap()))
                    .collect();
                tiebreak_results.sort_by_key(|(_, placement)| placement.placement());

                let mut winners = locked_winners.clone();
                winners.extend(tiebreak_results.iter().take(*open_seats).map(|(id, _)| *id));

                let mut losers: Vec<_> = tiebreak_results
                    .iter()
                    .skip(*open_seats)
                    .map(|(id, _)| *id)
                    .collect();
                losers.extend(locked_losers);
                (winners, losers)
            }
        }
    }
}

/// How a losers-bracket round is fed.
enum BracketRoundKind {
    Winners,
    LosersIntake { wb_round: usize },
    LosersConsolidate,
}

struct BracketRound {
    sets: Vec<BracketSetId>,
    kind: BracketRoundKind,
}

const MIN_WINNERS_BRACKET_RACE_SIZE: usize = 6;
const MIN_LOSERS_BRACKET_RACE_SIZE: usize = 4;

// Each heat advances its top 4 finishers.
const ADVANCERS_PER_SET: usize = 4;

pub(crate) struct Bracket {
    winners: Vec<BracketRound>,
    losers: Vec<BracketRound>,
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
        let round_count = self.winners.len().max(self.losers.len());

        (0..round_count)
            .flat_map(|round_index| {
                self.winners
                    .get(round_index)
                    .into_iter()
                    .chain(self.losers.get(round_index))
            })
            .flat_map(|round| round.sets.iter().copied())
            .find(|&id| {
                let set = &self.bracket_sets[id];
                set.is_ready() && !set.is_completed()
            })
    }

    pub(crate) fn is_complete(&self) -> bool {
        let &w_id = self.winners.last().unwrap().sets.first().unwrap();
        let &l_id = self.losers.last().unwrap().sets.first().unwrap();

        self.bracket_sets[w_id].is_completed() && self.bracket_sets[l_id].is_completed()
    }

    pub(crate) fn advance(&mut self) -> Result<bool, TournamentError> {
        // Winners bracket, round by round: reseat each heat whose feeders changed.
        for round_idx in 0..self.winners.len() {
            for set_idx in 0..self.winners[round_idx].sets.len() {
                let id = self.winners[round_idx].sets[set_idx];
                self.update_set(id)?;
                self.bracket_sets[id].prepare_resolution(ADVANCERS_PER_SET)?;
            }
        }

        // Losers bracket, round by round: reseat each heat whose feeders changed.
        for round_idx in 0..self.losers.len() {
            for set_idx in 0..self.losers[round_idx].sets.len() {
                let id = self.losers[round_idx].sets[set_idx];
                self.update_set(id)?;
                self.bracket_sets[id].prepare_resolution(ADVANCERS_PER_SET)?;
            }
        }

        Ok(self.is_complete())
    }

    pub(crate) fn get_results(&self) -> Option<(Vec<ParticipantId>, Vec<ParticipantId>)> {
        if !self.is_complete() {
            return None;
        }

        let winners_finals = &self.bracket_sets[*self.winners.last()?.sets.first()?];
        let losers_finals = &self.bracket_sets[*self.losers.last()?.sets.first()?];

        let (winners, _) = winners_finals.get_winners_losers(ADVANCERS_PER_SET);
        let (losers, _) = losers_finals.get_winners_losers(ADVANCERS_PER_SET);

        Some((winners, losers))
    }

    fn update_set(&mut self, set_id: BracketSetId) -> Result<(), TournamentError> {
        let set = &self.bracket_sets[set_id];

        // If there are no feeders, then this is a starting set. No need to update anything here.
        if set.feeders.is_empty() {
            return Ok(());
        }

        let mut participants = vec![];
        // Incomplete feeders contribute no racers, intentionally cascading an upstream reset
        // through every dependent heat.
        for feeder in set.feeders.iter() {
            let feeder_set = &self.bracket_sets[feeder.id];

            let (mut winners, mut losers) = feeder_set.get_winners_losers(ADVANCERS_PER_SET);
            match feeder.source {
                FeederSource::Winners => participants.append(&mut winners),
                FeederSource::Losers => participants.append(&mut losers),
            }
        }

        let set = &mut self.bracket_sets[set_id];
        if !set.contains_racers(&participants) {
            // An upstream correction changes this heat's roster, invalidating all existing results.
            set.clear_racers();
            set.add_racers(&participants)?;
        }

        Ok(())
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
        // TODO: an odd count of 4-groups leaves a degenerate 4-heat (eliminates no one), and
        // 6/7-player winners heats drop 2/3-groups that can pack below 4; both want byes.
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
            if !(MIN_LOSERS_BRACKET_RACE_SIZE..=MAX_RACERS).contains(&size) {
                return Err(TournamentError::InvalidBracketSetSize);
            }
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
    pub feeders: Vec<BracketFeederView>,
    /// Index of the next race to run; races before it are complete.
    pub current_race_index: usize,
    /// Whether the heat is fully seeded and can accept results.
    pub is_ready: bool,
}

#[derive(Debug)]
pub struct BracketFeederView {
    pub set_id: BracketSetId,
    pub source: FeederSource,
    pub racer_count: usize,
    pub is_resolved: bool,
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
    pub winners_finalists: Vec<ParticipantView>,
    pub losers_finalists: Vec<ParticipantView>,
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
            races: self
                .races
                .iter()
                .chain(
                    self.resolution
                        .iter()
                        .filter_map(|resolution| match resolution {
                            BracketResolution::Tiebreak { race, .. } => Some(race),
                            BracketResolution::Decided { .. } => None,
                        }),
                )
                .map(|race| race.view(id_map))
                .collect(),
            feeders: Vec::new(),
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
                .map(|&id| {
                    let mut view = self.bracket_sets[id].view(id_map);
                    view.feeders = self.bracket_sets[id]
                        .feeders
                        .iter()
                        .map(|feeder| {
                            let source_set = &self.bracket_sets[feeder.id];
                            BracketFeederView {
                                set_id: feeder.id,
                                source: feeder.source,
                                racer_count: Self::feeder_group_size(feeder, &self.bracket_sets),
                                is_resolved: source_set.is_completed(),
                            }
                        })
                        .collect();
                    (id, view)
                })
                .collect(),
        };

        let (winners_finalists, losers_finalists) = self.get_results().map_or_else(
            || (Vec::new(), Vec::new()),
            |(winners, losers)| {
                (
                    winners.iter().map(|id| id.view(id_map)).collect(),
                    losers.iter().map(|id| id.view(id_map)).collect(),
                )
            },
        );

        BracketView {
            winners: self.winners.iter().map(&round_view).collect(),
            losers: self.losers.iter().map(&round_view).collect(),
            active_set: self.active_set(),
            winners_finalists,
            losers_finalists,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::Placement;
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

    fn complete_set(set: &mut BracketSet) {
        let racers: Vec<_> = set.races[0].get_racers().collect();
        for race in &mut set.races {
            for (index, &racer) in racers.iter().enumerate() {
                race.set_placement(racer, Some(Placement::new((index + 1) as u8).unwrap()))
                    .unwrap();
            }
        }
    }

    fn complete_race(race: &mut Race, results: &[(ParticipantId, u8)]) {
        for &(racer, placement) in results {
            race.set_placement(racer, Some(Placement::new(placement).unwrap()))
                .unwrap();
        }
    }

    fn tiebreaker_race(set: &mut BracketSet) -> &mut Race {
        match set.resolution.as_mut().unwrap() {
            BracketResolution::Tiebreak { race, .. } => race,
            BracketResolution::Decided { .. } => panic!("expected tiebreak resolution"),
        }
    }

    fn set_with_cutoff_tie() -> (BracketSet, Vec<ParticipantId>) {
        let racers = make_participants(8);
        let mut set = BracketSet::new(1, racers.len(), vec![]).unwrap();
        set.add_racers(&racers).unwrap();
        complete_race(
            &mut set.races[0],
            &[
                (racers[0], 1),
                (racers[1], 2),
                (racers[2], 3),
                (racers[3], 4),
                (racers[4], 4),
                (racers[5], 6),
                (racers[6], 7),
                (racers[7], 8),
            ],
        );
        (set, racers)
    }

    #[test]
    fn cutoff_tie_creates_vanilla_race_for_contenders() {
        let (mut set, racers) = set_with_cutoff_tie();

        set.prepare_resolution(ADVANCERS_PER_SET).unwrap();

        let tiebreaker = tiebreaker_race(&mut set);
        assert!(matches!(tiebreaker.ruleset(), RaceRuleset::Vanilla));
        assert!(tiebreaker.contains_racers(&[racers[3], racers[4]]));
        complete_race(tiebreaker, &[(racers[3], 2), (racers[4], 1)]);

        set.prepare_resolution(ADVANCERS_PER_SET).unwrap();
        assert!(set.is_completed());
        let (winners, losers) = set.get_winners_losers(ADVANCERS_PER_SET);
        assert_eq!(winners, vec![racers[0], racers[1], racers[2], racers[4]]);
        assert_eq!(losers[0], racers[3]);
    }

    #[test]
    fn tied_tiebreak_race_returns_an_error() {
        let (mut set, racers) = set_with_cutoff_tie();
        set.prepare_resolution(ADVANCERS_PER_SET).unwrap();
        complete_race(tiebreaker_race(&mut set), &[(racers[3], 1), (racers[4], 1)]);

        assert_eq!(
            set.prepare_resolution(ADVANCERS_PER_SET),
            Err(TournamentError::BracketTiebreakUnresolved)
        );
        assert!(!set.is_completed());
        assert_eq!(set.current_race_index(), set.races.len());
        assert_eq!(set.get_winners_losers(ADVANCERS_PER_SET), (vec![], vec![]));
    }

    #[test]
    fn regulation_correction_removes_obsolete_tiebreak() {
        let (mut set, racers) = set_with_cutoff_tie();
        set.prepare_resolution(ADVANCERS_PER_SET).unwrap();
        assert!(matches!(
            set.resolution,
            Some(BracketResolution::Tiebreak { .. })
        ));

        set.race(0)
            .unwrap()
            .set_placement(racers[4], Some(Placement::new(5).unwrap()))
            .unwrap();
        set.prepare_resolution(ADVANCERS_PER_SET).unwrap();

        assert!(matches!(
            set.resolution,
            Some(BracketResolution::Decided { .. })
        ));
        assert!(set.is_completed());
    }

    #[test]
    fn advance_routes_opening_heat_results_to_both_brackets() {
        let racers = make_participants(16);
        let mut bracket = Bracket::new(3, &racers).unwrap();
        let first_winners_sets = bracket.winners[0].sets.clone();

        for &set_id in &first_winners_sets {
            complete_set(&mut bracket.bracket_sets[set_id]);
        }

        assert!(!bracket.advance().unwrap());

        let winners_final = &bracket.bracket_sets[bracket.winners[1].sets[0]];
        let first_losers_set = &bracket.bracket_sets[bracket.losers[0].sets[0]];
        let expected_winners = [&racers[0..4], &racers[8..12]].concat();
        let expected_losers = [&racers[4..8], &racers[12..16]].concat();

        assert!(winners_final.contains_racers(&expected_winners));
        assert!(first_losers_set.contains_racers(&expected_losers));
    }

    #[test]
    fn upstream_correction_resets_every_dependent_heat() {
        let racers = make_participants(16);
        let mut bracket = Bracket::new(3, &racers).unwrap();
        let opening_sets = bracket.winners[0].sets.clone();

        for &set_id in &opening_sets {
            complete_set(&mut bracket.bracket_sets[set_id]);
        }
        bracket.advance().unwrap();

        let winners_final_id = bracket.winners[1].sets[0];
        let first_losers_id = bracket.losers[0].sets[0];
        complete_set(&mut bracket.bracket_sets[winners_final_id]);
        complete_set(&mut bracket.bracket_sets[first_losers_id]);
        bracket.advance().unwrap();

        let last_losers_id = bracket.losers[1].sets[0];
        complete_set(&mut bracket.bracket_sets[last_losers_id]);
        bracket.advance().unwrap();
        assert!(bracket.bracket_sets[last_losers_id].is_completed());

        // Swap an advancing racer with an eliminated racer in an opening heat.
        let corrected_set = &mut bracket.bracket_sets[opening_sets[0]];
        for race_index in 0..corrected_set.races.len() {
            let race = corrected_set.race(race_index).unwrap();
            race.set_placement(racers[0], Some(Placement::new(5).unwrap()))
                .unwrap();
            race.set_placement(racers[4], Some(Placement::new(1).unwrap()))
                .unwrap();
        }

        assert!(!bracket.advance().unwrap());

        let corrected_winners = [
            &[racers[4], racers[1], racers[2], racers[3]],
            &racers[8..12],
        ]
        .concat();
        let corrected_losers = [
            &[racers[0], racers[5], racers[6], racers[7]],
            &racers[12..16],
        ]
        .concat();
        assert!(bracket.bracket_sets[winners_final_id].contains_racers(&corrected_winners));
        assert!(bracket.bracket_sets[first_losers_id].contains_racers(&corrected_losers));
        assert!(
            bracket.bracket_sets[winners_final_id]
                .races
                .iter()
                .all(|race| race
                    .get_racers_and_placements()
                    .iter()
                    .all(|(_, placement)| placement.is_none()))
        );
        assert_eq!(bracket.bracket_sets[last_losers_id].racer_count(), 0);
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

    #[test]
    fn invalid_losers_heat_size_returns_an_error() {
        assert!(matches!(
            Bracket::new(3, &make_participants(25)),
            Err(TournamentError::InvalidBracketSetSize)
        ));
    }
}
