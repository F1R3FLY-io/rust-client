//! The suite's Rholang fixtures, as paths for the CLI and as terms for the
//! library.

/// Writes to its `deployId` channel, so `deploy-and-wait` has data to read.
pub const STDOUT_TERM: &str = include_str!("../contracts/stdout.rho");

/// Writes nothing to `deployId`.
pub const SILENT_TERM: &str = include_str!("../contracts/silent.rho");

/// Path to a fixture, for commands taking `--file`.
pub fn path(name: &str) -> &'static str {
    match name {
        "stdout.rho" => concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/integration/contracts/stdout.rho"
        ),
        "silent.rho" => concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/integration/contracts/silent.rho"
        ),
        "broken.rho" => concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/integration/contracts/broken.rho"
        ),
        other => panic!("no contract named {other}"),
    }
}
