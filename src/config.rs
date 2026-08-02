use std::num::NonZero;

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub pool_rounds: NonZero<usize>,
    pub bracket_size: NonZero<usize>,
}

const DEFAULT_POOL_ROUNDS: NonZero<usize> = NonZero::new(8).unwrap();
const DEFAULT_BRACKET_SIZE: NonZero<usize> = NonZero::new(16).unwrap();

impl Default for Config {
    fn default() -> Self {
        Self {
            pool_rounds: DEFAULT_POOL_ROUNDS,
            bracket_size: DEFAULT_BRACKET_SIZE,
        }
    }
}
