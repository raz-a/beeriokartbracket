use slotmap::new_key_type;

use crate::error::TournamentError;
use crate::participant::{ParticipantId, ParticipantMap, ParticipantView};
use crate::view::Viewable;

new_key_type! { pub struct RaceId; }

pub(crate) const MAX_RACERS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement(u8);

impl Placement {
    pub fn new(val: u8) -> Result<Self, TournamentError> {
        if val == 0 || val > MAX_RACERS as u8 {
            return Err(TournamentError::InvalidPlacementValue);
        }

        Ok(Placement(val))
    }

    pub fn points(&self) -> usize {
        // Points awarded are always relative to an 8 person race, even if the race has less than 8 people.
        MAX_RACERS - self.placement_idx() as usize
    }

    pub fn placement(&self) -> u8 {
        self.0
    }

    pub fn placement_idx(&self) -> u8 {
        self.placement() - 1
    }
}

#[derive(Debug, Default, Copy, Clone)]
pub enum RaceRuleset {
    #[default]
    Vanilla,
    Beerio,
}

#[derive(Debug, Default)]
pub(crate) struct Race {
    racers: Vec<(ParticipantId, Option<Placement>)>,
    ruleset: RaceRuleset,
}

impl Race {
    pub fn set_ruleset(&mut self, ruleset: RaceRuleset) {
        self.ruleset = ruleset;
    }

    pub fn add_racers(&mut self, racers: &[ParticipantId]) -> Result<(), TournamentError> {
        if self.racers.len() + racers.len() > MAX_RACERS {
            return Err(TournamentError::RaceIsFull);
        }

        if self.racers.iter().any(|(r, _)| racers.contains(r)) {
            return Err(TournamentError::RacerAlreadyInRace);
        }

        self.racers.extend(racers.iter().map(|&r| (r, None)));
        Ok(())
    }

    pub fn remove_racer(&mut self, racer: ParticipantId) -> Result<(), TournamentError> {
        if let Some(idx) = self.racers.iter().position(|(r, _)| *r == racer) {
            self.racers.remove(idx);
            Ok(())
        } else {
            Err(TournamentError::RacerNotInRace)
        }
    }

    pub fn clear_racers(&mut self) -> Result<(), TournamentError> {
        self.racers = vec![];
        Ok(())
    }

    pub fn set_placement(
        &mut self,
        racer: ParticipantId,
        place: Option<Placement>,
    ) -> Result<(), TournamentError> {
        if let Some(p) = place
            && p.placement() > self.racers.len() as u8
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

    pub fn is_complete(&self) -> bool {
        !self.racers.is_empty() && self.racers.iter().all(|(_, p)| p.is_some())
    }

    pub fn get_racers(&self) -> impl Iterator<Item = ParticipantId> {
        self.racers.iter().map(|&(p, _)| p)
    }

    pub fn get_racers_and_placements(&self) -> &[(ParticipantId, Option<Placement>)] {
        &self.racers
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
