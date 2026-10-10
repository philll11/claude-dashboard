//! Tokens by type, as a request uses them and API prices charge for them.

/// The tokens a request used, by type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    /// Tokens written to the prompt cache for five minutes.
    pub cache_write_5m: u64,
    /// Tokens written to the prompt cache for one hour.
    pub cache_write_1h: u64,
}

impl Tokens {
    /// Whether no tokens were used.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Adds another count, stopping at the largest number rather than overflowing.
    pub fn add(&mut self, other: Self) {
        self.input = self.input.saturating_add(other.input);
        self.output = self.output.saturating_add(other.output);
        self.cache_read = self.cache_read.saturating_add(other.cache_read);
        self.cache_write_5m = self.cache_write_5m.saturating_add(other.cache_write_5m);
        self.cache_write_1h = self.cache_write_1h.saturating_add(other.cache_write_1h);
    }

    /// The length of the prompt: its input tokens, including cache reads and cache writes.
    #[must_use]
    pub fn prompt(&self) -> u64 {
        self.input
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_write_5m)
            .saturating_add(self.cache_write_1h)
    }
}

#[cfg(test)]
mod tests {
    use super::Tokens;

    #[test]
    fn the_prompt_counts_every_kind_of_input() {
        let tokens = Tokens {
            input: 3,
            output: 500,
            cache_read: 90_000,
            cache_write_5m: 6_000,
            cache_write_1h: 4_000,
        };
        assert_eq!(tokens.prompt(), 100_003);
    }

    #[test]
    fn adding_stops_at_the_largest_number() {
        let mut tokens = Tokens {
            output: u64::MAX - 1,
            ..Tokens::default()
        };
        tokens.add(Tokens {
            output: 5,
            input: 2,
            ..Tokens::default()
        });
        assert_eq!(tokens.output, u64::MAX);
        assert_eq!(tokens.input, 2);
    }
}
