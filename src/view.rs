use slotmap::SlotMap;

use crate::config::Config;
use crate::participant::{Participant, ParticipantId, ParticipantView};
use crate::pool::PoolView;

pub(crate) type ParticipantMap = SlotMap<ParticipantId, Participant>;

pub(crate) trait Viewable<View> {
    fn view(&self, id_map: &ParticipantMap) -> View;
}

#[derive(Debug)]
pub struct RegistrationView {
    pub participants: Vec<ParticipantView>,
    pub config: Config,
}

#[derive(Debug)]
pub enum TournamentView {
    Registration(RegistrationView),
    Pools(PoolView),
    Bracket,
    Gauntlet,
    Complete,
}
