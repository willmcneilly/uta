//! Uta's audio engine: a control side and an audio-thread processor. See
//! RFC-001, "The audio thread follows strict rules".

/// The crate version, until there is real API to test.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_builds() {
        assert!(!VERSION.is_empty());
    }
}
