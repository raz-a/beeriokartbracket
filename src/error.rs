#[derive(Debug, PartialEq)]
pub enum TournamentError {
    NoParticipants,
    NonExistentParticipant,
    RaceIsFull,
    ParticipantAlreadyInRace,
    InvalidPlacementValue,
}
