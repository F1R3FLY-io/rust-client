//! The library's error type.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

/// Everything a library call can fail with.
///
/// Variants describe what went wrong for the caller, not which function
/// noticed it, so code can branch on them: a [`Transport`](Error::Transport)
/// failure is worth retrying, an [`InvalidInput`](Error::InvalidInput) is not.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The request did not reach the node, or its answer did not come back.
    #[error("cannot reach {endpoint}: {source}")]
    Transport {
        endpoint: String,
        #[source]
        source: TransportError,
    },

    /// The node answered an HTTP request with a non-success status.
    #[error("{url} answered HTTP {status}{}: {message}", with_code(.code))]
    Http {
        url: String,
        status: u16,
        /// The node's machine-readable error code, such as `readonly_node_required`.
        code: Option<String>,
        message: String,
    },

    /// A gRPC call ended with a non-OK status.
    #[error("{endpoint} answered gRPC {}: {}", .status.code(), .status.message())]
    Grpc {
        endpoint: String,
        #[source]
        status: Box<tonic::Status>,
    },

    /// The node handled the request and refused it.
    #[error("the node refused {operation}: {message}")]
    NodeRejected {
        operation: &'static str,
        message: String,
    },

    /// The node's answer did not have the expected shape.
    #[error("unexpected {what} from the node: {detail}")]
    Decode { what: &'static str, detail: String },

    /// An argument was malformed or out of range.
    #[error("invalid {what}: {detail}")]
    InvalidInput { what: &'static str, detail: String },

    /// A required environment variable is not set.
    #[error("{var} is not set")]
    MissingConfig { var: &'static str },

    /// A local file could not be read or written.
    #[error("{}: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The deploy executed and failed.
    #[error("deploy {deploy_id} failed{}", describe(.execution))]
    DeployFailed {
        deploy_id: String,
        /// The node's account of the failed execution, when it gave one.
        execution: Option<String>,
    },

    /// The deploy's validity window closed without it landing canonically.
    #[error("deploy {deploy_id} expired without canonical inclusion")]
    DeployExpired { deploy_id: String },

    /// No node reported a verdict for the deploy within the wait budget.
    #[error("deploy {deploy_id} reached no verdict within {}s{}", .waited.as_secs(), describe(.execution))]
    FinalizationTimeout {
        deploy_id: String,
        waited: Duration,
        /// A failed execution the deploy node already reported, if any.
        execution: Option<String>,
    },

    /// Two nodes reported different terminal verdicts for the same deploy.
    #[error(
        "deploy {deploy_id}: the deploy node reports {node} but the observer reports {observer}"
    )]
    ConflictingVerdicts {
        deploy_id: String,
        node: String,
        observer: String,
    },

    /// The node does not serve an endpoint the call needs.
    #[error("{endpoint} does not serve {feature} (requires {requirement})")]
    NodeTooOld {
        endpoint: String,
        feature: &'static str,
        requirement: &'static str,
    },

    /// A system contract executed the call and answered with a rejection.
    #[error("{contract} rejected the call: {reason}")]
    ContractRejected { contract: Contract, reason: String },
}

/// Why a request did not complete at the transport level.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TransportError {
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Grpc(#[from] tonic::transport::Error),
    #[error(transparent)]
    WebSocket(Box<tokio_tungstenite::tungstenite::Error>),
    #[error("no answer within {0:?}")]
    TimedOut(Duration),
}

impl From<tokio_tungstenite::tungstenite::Error> for TransportError {
    fn from(error: tokio_tungstenite::tungstenite::Error) -> Self {
        TransportError::WebSocket(Box::new(error))
    }
}

/// A system contract that can reject a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Contract {
    Vault,
    ProofOfStake,
}

impl fmt::Display for Contract {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Contract::Vault => "the vault",
            Contract::ProofOfStake => "PoS",
        })
    }
}

fn with_code(code: &Option<String>) -> String {
    code.as_ref()
        .map(|code| format!(" ({code})"))
        .unwrap_or_default()
}

fn describe(execution: &Option<String>) -> String {
    execution
        .as_ref()
        .map(|execution| format!(": {execution}"))
        .unwrap_or_default()
}

impl Error {
    pub(crate) fn transport(
        endpoint: impl Into<String>,
        source: impl Into<TransportError>,
    ) -> Self {
        Error::Transport {
            endpoint: endpoint.into(),
            source: source.into(),
        }
    }

    pub(crate) fn grpc(endpoint: impl Into<String>, status: tonic::Status) -> Self {
        Error::Grpc {
            endpoint: endpoint.into(),
            status: Box::new(status),
        }
    }

    pub(crate) fn decode(what: &'static str, detail: impl fmt::Display) -> Self {
        Error::Decode {
            what,
            detail: detail.to_string(),
        }
    }

    pub(crate) fn invalid(what: &'static str, detail: impl fmt::Display) -> Self {
        Error::InvalidInput {
            what,
            detail: detail.to_string(),
        }
    }
}

/// Result type of the library.
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_thread_safe<T: std::error::Error + Send + Sync + 'static>() {}

    #[test]
    fn errors_cross_threads_and_await_points() {
        assert_thread_safe::<Error>();
        assert_thread_safe::<TransportError>();
    }

    #[test]
    fn errors_stay_small_enough_to_return_by_value() {
        assert!(
            std::mem::size_of::<Error>() <= 128,
            "{} bytes",
            std::mem::size_of::<Error>()
        );
    }

    #[test]
    fn deploy_outcomes_name_the_execution_failure_only_when_known() {
        let failed = Error::DeployFailed {
            deploy_id: "abc".into(),
            execution: Some("block 7 (cost 0): Insufficient funds".into()),
        };
        assert_eq!(
            failed.to_string(),
            "deploy abc failed: block 7 (cost 0): Insufficient funds"
        );
        let timeout = Error::FinalizationTimeout {
            deploy_id: "abc".into(),
            waited: Duration::from_secs(300),
            execution: None,
        };
        assert_eq!(
            timeout.to_string(),
            "deploy abc reached no verdict within 300s"
        );
    }

    #[test]
    fn http_errors_name_the_node_error_code() {
        let error = Error::Http {
            url: "http://n:40403/api/estimate-cost".into(),
            status: 400,
            code: Some("rholang_bad_term".into()),
            message: "syntax error".into(),
        };
        assert_eq!(
            error.to_string(),
            "http://n:40403/api/estimate-cost answered HTTP 400 (rholang_bad_term): syntax error"
        );
    }
}
