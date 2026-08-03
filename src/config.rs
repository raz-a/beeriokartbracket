use std::num::NonZero;

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub pool_rounds: NonZero<usize>,
    pub bracket_size: NonZero<usize>,
    pub bracket_races_per_round: NonZero<usize>,
    pub seed: u64,
}

const DEFAULT_POOL_ROUNDS: NonZero<usize> = NonZero::new(8).unwrap();
const DEFAULT_BRACKET_SIZE: NonZero<usize> = NonZero::new(16).unwrap();
const DEFAULT_BRACKET_RACES_COUNT: NonZero<usize> = NonZero::new(3).unwrap();

impl Default for Config {
    fn default() -> Self {
        Self {
            pool_rounds: DEFAULT_POOL_ROUNDS,
            bracket_size: DEFAULT_BRACKET_SIZE,
            bracket_races_per_round: DEFAULT_BRACKET_RACES_COUNT,
            // Random by default so real tournaments differ; set explicitly to reproduce a run.
            seed: rand::random(),
        }
    }
}
