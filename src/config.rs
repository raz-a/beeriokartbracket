use std::num::NonZero;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolRaceFormat {
    #[default]
    AlternatingSingles,
    BeerioVanillaPairs,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub pool_rounds: NonZero<usize>,
    #[serde(default)]
    pub pool_race_format: PoolRaceFormat,
    pub bracket_size: NonZero<usize>,
    pub bracket_races_per_round: NonZero<usize>,
    pub gauntlet_lives: NonZero<usize>,
    pub seed: u64,
}

const DEFAULT_POOL_ROUNDS: NonZero<usize> = NonZero::new(8).unwrap();
const DEFAULT_BRACKET_SIZE: NonZero<usize> = NonZero::new(16).unwrap();
const DEFAULT_BRACKET_RACES_COUNT: NonZero<usize> = NonZero::new(3).unwrap();
const DEFAULT_GAUNTLET_LIVES: NonZero<usize> = NonZero::new(3).unwrap();

impl Default for Config {
    fn default() -> Self {
        Self {
            pool_rounds: DEFAULT_POOL_ROUNDS,
            pool_race_format: PoolRaceFormat::default(),
            bracket_size: DEFAULT_BRACKET_SIZE,
            bracket_races_per_round: DEFAULT_BRACKET_RACES_COUNT,
            gauntlet_lives: DEFAULT_GAUNTLET_LIVES,
            // Random by default so real tournaments differ; set explicitly to reproduce a run.
            seed: rand::random(),
        }
    }
}
