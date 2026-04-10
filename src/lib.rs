// Library modules
pub mod connection_manager;
pub mod error;
pub mod events;
pub mod f1r3fly_api;
pub mod grpc;
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

// Re-export primary types
pub use connection_manager::{ConnectionConfig, ConnectionError, F1r3flyConnectionManager};
pub use error::{NodeCliError, Result};
pub use events::NodeEvents;
pub use f1r3fly_api::{DeployDetail, DeployResult, F1r3flyApi, ProposeResult};
pub use grpc::query::extract_par_data;
pub use vault::{TransferResult, DUST_FACTOR};
