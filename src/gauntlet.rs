use std::collections::HashMap;

use crate::Placement;
use crate::participant::{ParticipantId, ParticipantMap, ParticipantView};
use crate::race::{Race, RaceView};
use crate::view::Viewable;

#[derive(Debug)]
struct GauntletRacer {
    lives: usize,
    placement: Option<Placement>,
}

#[derive(Debug)]
pub(crate) struct Gauntlet {
    racers: HashMap<ParticipantId, GauntletRacer>,
    races: Vec<Race>,
}

impl Gauntlet {
    pub(crate) fn new(
        winners: Vec<ParticipantId>,
        losers: Vec<ParticipantId>,
        lives: usize,
    ) -> Self {
        Self {
            racers: winners
                .into_iter()
                .map(|id| {
                    (
                        id,
                        GauntletRacer {
                            lives: lives * 2,
                            placement: None,
                        },
                    )
                })
                .chain(losers.into_iter().map(|id| {
                    (
                        id,
                        GauntletRacer {
                            lives,
                            placement: None,
                        },
                    )
                }))
                .collect(),
            races: vec![],
        }
    }

    // TODO: Implement advance, get_current_race, get_race_by_index.
}

#[derive(Debug)]
pub struct GauntletRacerView {
    pub participant: ParticipantView,
    pub lives: usize,
    pub placement: Option<Placement>,
}

#[derive(Debug)]
pub struct GauntletView {
    pub racers: Vec<GauntletRacerView>,
    pub races: Vec<RaceView>,
}

impl Viewable<GauntletView> for Gauntlet {
    fn view(&self, id_map: &ParticipantMap) -> GauntletView {
        let mut racers: Vec<_> = self
            .racers
            .iter()
            .map(|(&id, racer)| GauntletRacerView {
                participant: id.view(id_map),
                lives: racer.lives,
                placement: racer.placement,
            })
            .collect();
        racers.sort_by(|left, right| {
            right
                .lives
                .cmp(&left.lives)
                .then_with(|| left.participant.name.cmp(&right.participant.name))
        });

        GauntletView {
            racers,
            races: self.races.iter().map(|race| race.view(id_map)).collect(),
        }
    }
}
