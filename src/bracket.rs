use crate::{ParticipantId, RaceRuleset, TournamentError, race::Race};

#[derive(Debug, Default)]
pub(crate) struct Bracket;

struct BracketSet {
    races: Vec<Race>,
}

impl BracketSet {
    pub fn new(num_races: usize, racers: &[ParticipantId]) -> Result<Self, TournamentError> {
        let mut races = Vec::with_capacity(num_races);
        for i in 0..num_races {
            let mut race = Race::default();
            race.add_racers(racers)?;

            // Even races are Vanilla. Odd are Beerio.
            let ruleset = if i.is_multiple_of(2) {
                RaceRuleset::Vanilla
            } else {
                RaceRuleset::Beerio
            };

            race.set_ruleset(ruleset);
            races.push(race);
        }

        Ok(Self { races })
    }

    fn is_started(&self) -> bool {
        self.races.iter().any(|race| race.is_complete())
    }

    fn is_completed(&self) -> bool {
        self.races.iter().all(|race| race.is_complete())
    }

    pub fn add_racers(&mut self, racers: &[ParticipantId]) -> Result<(), TournamentError> {
        if self.is_started() {
            Err(TournamentError::BracketSetAlreadyStarted)
        } else {
            for race in self.races.iter_mut() {
                race.add_racers(racers)?
            }

            Ok(())
        }
    }

    pub fn current_race_index(&self) -> usize {
        self.races
            .iter()
            .take_while(|race| race.is_complete())
            .count()
    }

    pub fn race(&mut self, index: usize) -> Option<&mut Race> {
        self.races.get_mut(index)
    }
}
