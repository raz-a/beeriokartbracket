use std::collections::HashSet;

use slotmap::new_key_type;

use crate::error::TournamentError;
use crate::participant::{ParticipantId, ParticipantMap, ParticipantView};
use crate::persistence::PersistedState;
use crate::view::Viewable;

new_key_type! { pub struct RaceId; }

pub(crate) const MAX_RACERS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Placement(u8);

impl<'de> serde::Deserialize<'de> for Placement {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        Self::new(value).map_err(|_| serde::de::Error::custom("invalid placement"))
    }
}

impl Placement {
    pub const DISQUALIFIED: Self = Self(u8::MAX);

    pub fn new(val: u8) -> Result<Self, TournamentError> {
        if val != 0 && val <= MAX_RACERS as u8 {
            Ok(Placement(val))
        } else if val == Self::DISQUALIFIED.0 {
            Ok(Self::DISQUALIFIED)
        } else {
            Err(TournamentError::InvalidPlacementValue)
        }
    }

    pub fn points(&self) -> usize {
        // Points awarded are always relative to an 8 person race, even if the race has less than 8 people.
        self.placement_idx()
            .map_or(0, |index| MAX_RACERS - index as usize)
    }

    pub fn placement(&self) -> u8 {
        self.0
    }

    pub(crate) fn placement_idx(&self) -> Option<u8> {
        (!self.is_disqualified()).then(|| self.placement() - 1)
    }

    pub(crate) fn move_up(self) -> Option<Self> {
        self.placement_idx()
            .filter(|&index| index != 0)
            .map(Placement)
    }

    pub fn is_disqualified(&self) -> bool {
        *self == Self::DISQUALIFIED
    }

    pub(crate) fn is_valid_for_race(&self, racer_count: usize) -> bool {
        self.is_disqualified() || self.placement() as usize <= racer_count
    }
}

#[derive(Debug, Default, Copy, Clone, serde::Serialize, serde::Deserialize)]
pub enum RaceRuleset {
    #[default]
    Vanilla,
    Beerio,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct Race {
    racers: Vec<(ParticipantId, Option<Placement>)>,
    ruleset: RaceRuleset,
}

impl PersistedState<ParticipantMap> for Race {
    fn validate_loaded(&self, participants: &ParticipantMap) -> Result<(), &'static str> {
        if self.racers.len() > MAX_RACERS {
            return Err("race exceeds the maximum racer count");
        }

        let mut racers = HashSet::new();
        for (racer, placement) in &self.racers {
            if !participants.contains_key(*racer) {
                return Err("race references a missing participant");
            }
            if !racers.insert(*racer) {
                return Err("race contains a participant more than once");
            }
            if placement.is_some_and(|placement| !placement.is_valid_for_race(self.racers.len())) {
                return Err("race contains an invalid placement");
            }
        }

        Ok(())
    }
}

impl Race {
    pub(crate) fn set_ruleset(&mut self, ruleset: RaceRuleset) {
        self.ruleset = ruleset;
    }

    #[cfg(test)]
    pub(crate) fn ruleset(&self) -> RaceRuleset {
        self.ruleset
    }

    pub(crate) fn add_racers(&mut self, racers: &[ParticipantId]) -> Result<(), TournamentError> {
        if self.racers.len() + racers.len() > MAX_RACERS {
            return Err(TournamentError::RaceIsFull);
        }

        if self.racers.iter().any(|(r, _)| racers.contains(r)) {
            return Err(TournamentError::RacerAlreadyInRace);
        }

        self.racers.extend(racers.iter().map(|&r| (r, None)));
        Ok(())
    }

    fn _remove_racer(&mut self, racer: ParticipantId) -> Result<(), TournamentError> {
        if let Some(idx) = self.racers.iter().position(|(r, _)| *r == racer) {
            self.racers.remove(idx);
            Ok(())
        } else {
            Err(TournamentError::RacerNotInRace)
        }
    }

    pub(crate) fn clear_racers(&mut self) {
        self.racers = vec![];
    }

    pub(crate) fn set_placement(
        &mut self,
        racer: ParticipantId,
        place: Option<Placement>,
    ) -> Result<(), TournamentError> {
        if let Some(p) = place
            && !p.is_valid_for_race(self.racers.len())
        {
            return Err(TournamentError::InvalidPlacementValue);
        }

        // Note: Duplicate placements are allowed in the case of ties.
        if let Some((_, p)) = self.racers.iter_mut().find(|(r, _)| *r == racer) {
            *p = place;
            Ok(())
        } else {
            Err(TournamentError::RacerNotInRace)
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        !self.racers.is_empty() && self.racers.iter().all(|(_, p)| p.is_some())
    }

    pub(crate) fn get_racers(&self) -> impl Iterator<Item = ParticipantId> {
        self.racers.iter().map(|&(p, _)| p)
    }

    pub(crate) fn get_racers_and_placements(&self) -> &[(ParticipantId, Option<Placement>)] {
        &self.racers
    }

    pub(crate) fn contains_racers(&self, racers: &[ParticipantId]) -> bool {
        let mut set: HashSet<_> = self.get_racers().collect();
        for id in racers {
            if !set.remove(id) {
                return false;
            }
        }

        set.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disqualified_has_no_points_or_numeric_index() {
        let placement = Placement::DISQUALIFIED;

        assert_eq!(placement.points(), 0);
        assert_eq!(placement.placement_idx(), None);
        assert!(placement.is_valid_for_race(2));
    }

    #[test]
    fn deserialization_rejects_invalid_placement() {
        assert!(serde_json::from_str::<Placement>("0").is_err());
        assert!(serde_json::from_str::<Placement>("9").is_err());
    }
}

#[derive(Debug)]
pub struct RaceView {
    pub racers: Vec<(ParticipantView, Option<Placement>)>,
    pub ruleset: RaceRuleset,
}

impl Viewable<RaceView> for Race {
    fn view(&self, id_map: &ParticipantMap) -> RaceView {
        RaceView {
            racers: self
                .racers
                .iter()
                .map(|(id, place)| (id.view(id_map), *place))
                .collect(),
            ruleset: self.ruleset,
        }
    }
}
