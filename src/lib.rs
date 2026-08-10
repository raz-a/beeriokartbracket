// TODO: Remove this
#![allow(dead_code)]

// TODO: Add tests.
// TODO: Connect Pools logic to tournament.

mod bracket;
mod config;
mod error;
mod participant;
mod pool;
mod race;
mod race_group;
mod tournament;
mod view;

pub use bracket::{BracketRoundView, BracketSetView, BracketView};
pub use config::Config;
pub use error::TournamentError;
pub use participant::{ParticipantId, ParticipantView};
pub use pool::{PoolResultView, PoolView};
pub use race::{Placement, RaceId, RaceRuleset, RaceView};
pub use tournament::Tournament;
pub use view::{RegistrationView, TournamentView};
