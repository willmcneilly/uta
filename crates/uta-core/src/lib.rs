//! Uta's project core: owns the project, applies commands and keeps undo
//! history. See RFC-001, "The project core owns the project".

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
