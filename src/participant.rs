use slotmap::new_key_type;

new_key_type! { pub struct ParticipantId; }

#[derive(Debug)]
pub struct Participant {
    name: String,
}

impl Participant {
    pub(crate) fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

pub struct ParticipantScore {
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
