// Library modules
pub mod connection_manager;
pub mod deploy;
pub mod endpoint;
pub mod error;
pub mod events;
pub mod f1r3fly_api;
pub mod grpc;
pub mod node_http;
pub mod pos;
pub mod registry;
pub mod rholang_helpers;
pub mod signing_key;
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
#[cfg(feature = "cli")]
pub mod settings;

// Re-export primary types
pub use connection_manager::{ConnectionConfig, F1r3flyConnectionManager};
pub use deploy::{DeployOptions, SignedDeploy, UnsignedDeploy};
pub use endpoint::Endpoint;
pub use error::{Contract, Error, Result, TransportError};
pub use events::NodeEvents;
pub use f1r3fly_api::{DeployDetail, DeployResult, F1r3flyApi, ProposeResult};
pub use grpc::query::extract_par_data;
pub use node_http::NodeHttp;
pub use signing_key::SigningKey;
pub use vault::TransferResult;
