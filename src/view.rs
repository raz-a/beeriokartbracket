use crate::bracket::BracketView;
use crate::config::Config;
use crate::participant::{ParticipantMap, ParticipantView};
use crate::pool::{PoolResultView, PoolView};

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
    Pools((PoolView, Option<PoolResultView>)),
    Bracket(BracketView),
    Gauntlet,
    Complete,
}
