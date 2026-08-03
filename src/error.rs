#[derive(Debug, PartialEq)]
pub enum TournamentError {
    NoParticipants,
    NonExistentParticipant,
    RaceIsFull,
    RacerAlreadyInRace,
    RacerNotInRace,
    InvalidPlacementValue,
    RaceIsNotComplete,
    WrongPhase,
    NotEnoughParticipants,
    PoolsNotCompleted,
    RaceNotFound,
    ResultsDontMatchRace,
    BracketSetAlreadyStarted,
}
