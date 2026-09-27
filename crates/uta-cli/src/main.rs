//! The `uta` command. Subcommands (`render`, `play`) arrive with the engine.

fn main() {
    println!("{}", banner());
}

fn banner() -> String {
    format!(
        "uta {} (core {}, engine {})",
        env!("CARGO_PKG_VERSION"),
        uta_core::VERSION,
        uta_engine::VERSION
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_names_every_crate() {
        let banner = banner();
        assert!(banner.starts_with("uta "));
        assert!(banner.contains("core "));
        assert!(banner.contains("engine "));
    }
}
