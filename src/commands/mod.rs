/// Errors from CLI commands: library [`crate::Error`]s, boxed alongside the
/// CLI's own messages. Thread-safe so a held error never makes a future `!Send`.
pub type CliError = Box<dyn std::error::Error + Send + Sync>;
pub type CliResult<T = ()> = std::result::Result<T, CliError>;

/// A datum as Rholang source, or why it is not a value.
pub(crate) fn show(par: &f1r3fly_models::rhoapi::Par) -> String {
    match crate::rho_value::RhoValue::try_from(par) {
        Ok(value) => value.to_string(),
        Err(err) => format!("<{err}>"),
    }
}

/// The native token's decimals, for an amount given in whole tokens. A node
/// that reports none cannot convert the amount.
pub(crate) fn whole_token_decimals(
    parameters: &crate::f1r3fly_api::DeployParameters,
) -> CliResult<u32> {
    match parameters.native_token_decimals {
        0 => Err("node did not report valid token decimals; cannot convert whole tokens".into()),
        decimals => Ok(decimals),
    }
}

pub mod config;
pub mod crypto;
pub mod dag;
pub mod events;
pub mod load_test;
pub mod network;
pub mod query;

// Re-export all command functions for convenience
pub use config::*;
pub use crypto::*;
pub use dag::*;
pub use events::*;
pub use load_test::*;
pub use network::*;
pub use query::*;
