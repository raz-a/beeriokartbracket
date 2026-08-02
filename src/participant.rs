use slotmap::{SlotMap, new_key_type};

use crate::view::Viewable;

new_key_type! { pub struct ParticipantId; }

pub(crate) type ParticipantMap = SlotMap<ParticipantId, Participant>;

impl Viewable<ParticipantView> for ParticipantId {
    fn view(&self, id_map: &ParticipantMap) -> ParticipantView {
        ParticipantView {
            name: id_map.get(*self).unwrap().name.clone(),
            id: *self,
        }
    }
}

#[derive(Debug)]
pub(crate) struct Participant {
    name: String,
}

impl Participant {
    pub(crate) fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Debug)]
pub(crate) struct ParticipantScore {
    id: ParticipantId,
    score: usize,
}

impl ParticipantScore {
    pub fn get_id(&self) -> ParticipantId {
        self.id
    }

    pub fn get_score(&self) -> usize {
        self.score
    }
}

impl From<(ParticipantId, usize)> for ParticipantScore {
    fn from((id, score): (ParticipantId, usize)) -> Self {
        Self { id, score }
    }
}

#[derive(Debug)]
pub struct ParticipantView {
    pub name: String,
    pub id: ParticipantId,
}
