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
mod tournament;
mod view;

pub use config::Config;
pub use error::TournamentError;
pub use participant::ParticipantId;
pub use race::{Placement, RaceRuleset};
pub use tournament::Tournament;
pub use view::TournamentView;
