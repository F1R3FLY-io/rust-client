// Library modules (always available)
pub mod connection_manager;
pub mod error;
pub mod f1r3fly_api;
pub mod http_client;
pub mod registry;
pub mod rholang_helpers;
pub mod signing;
pub mod utils;
pub mod vault;

// CLI modules (behind "cli" feature)
#[cfg(feature = "cli")]
pub mod args;
#[cfg(feature = "cli")]
pub mod commands;
#[cfg(feature = "cli")]
pub mod dag;
#[cfg(feature = "cli")]
pub mod dispatcher;

// Re-export commonly used types
pub use connection_manager::{ConnectionConfig, ConnectionError, F1r3flyConnectionManager};
pub use vault::{TransferResult, DUST_FACTOR};
