use slotmap::new_key_type;

new_key_type! { pub struct ParticipantId; }

#[derive(Debug)]
pub struct Participant {
    name: String,
    seed: usize,
}

impl Participant {
    pub(crate) fn new(name: &str, seed: usize) -> Self {
        Self {
            name: name.to_string(),
            seed,
        }
    }
}
