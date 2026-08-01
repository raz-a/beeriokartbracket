// TODO: Remove this
#![allow(dead_code)]

// TODO: Add tests.
// TODO: Connect Pools logic to tournament.

mod error;
mod participant;
mod pool;
mod race;
mod tournament;

pub use error::TournamentError;
pub use participant::{Participant, ParticipantId};
pub use pool::Pool;
pub use race::{Placement, Race, RaceRuleset};
pub use tournament::{Config, Tournament, TournamentView};
