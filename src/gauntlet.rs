use std::collections::HashMap;

use crate::participant::{ParticipantId, ParticipantMap, ParticipantView};
use crate::view::Viewable;

#[derive(Debug)]
pub(crate) struct Gauntlet {
    racers: HashMap<ParticipantId, usize>,
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
                .map(|id| (id, lives * 2))
                .chain(losers.into_iter().map(|id| (id, lives)))
                .collect(),
        }
    }
}

#[derive(Debug)]
pub struct GauntletView {
    pub racers: Vec<(ParticipantView, usize)>,
}

impl Viewable<GauntletView> for Gauntlet {
    fn view(&self, id_map: &ParticipantMap) -> GauntletView {
        let mut racers: Vec<_> = self
            .racers
            .iter()
            .map(|(&id, &lives)| (id.view(id_map), lives))
            .collect();
        racers.sort_by(|(left, _), (right, _)| left.name.cmp(&right.name));

        GauntletView { racers }
    }
}
