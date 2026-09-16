pub mod chain;
pub mod cli;
pub mod keys;
pub mod node;
pub mod shard;

use std::sync::OnceLock;
use tokio::runtime::Runtime;

/// One runtime for the whole suite: the trials are synchronous, and the client
/// library is not.
pub fn rt() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| Runtime::new().expect("tokio runtime"))
}
