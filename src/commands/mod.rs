/// Errors from CLI commands: library [`crate::Error`]s, boxed alongside the
/// CLI's own messages. Thread-safe so a held error never makes a future `!Send`.
pub type CliError = Box<dyn std::error::Error + Send + Sync>;
pub type CliResult<T = ()> = std::result::Result<T, CliError>;

pub mod crypto;
pub mod dag;
pub mod events;
pub mod load_test;
pub mod network;
pub mod query;

// Re-export all command functions for convenience
pub use crypto::*;
pub use dag::*;
pub use events::*;
pub use load_test::*;
pub use network::*;
pub use query::*;
