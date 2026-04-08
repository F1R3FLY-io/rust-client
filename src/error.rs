use std::error::Error;

/// Error types for the f1r3fly client library and CLI
#[derive(Debug, thiserror::Error)]
pub enum NodeCliError {
 #[error("Network error: {0}")]
 Network(#[from] NetworkError),

 #[error("Crypto error: {0}")]
 Crypto(#[from] CryptoError),

 #[error("File error: {0}")]
 File(#[from] FileError),

 #[error("API error: {0}")]
 Api(#[from] ApiError),

 #[error("Configuration error: {0}")]
 Config(#[from] ConfigError),

 #[error("{0}")]
 General(String),
}

#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
 #[error("Connection failed: {0}")]
 ConnectionFailed(String),

 #[error("HTTP {0} error: {1}")]
 HttpError(u16, String),

 #[error("Request timed out: {0}")]
 Timeout(String),

 #[error("Invalid URL: {0}")]
 InvalidUrl(String),

 #[error("Request failed: {0}")]
 RequestFailed(String),
}

#[derive(Debug, thiserror::Error)]
pub enum CryptoError {
 #[error("Invalid private key: {0}")]
 InvalidPrivateKey(String),

 #[error("Invalid public key: {0}")]
 InvalidPublicKey(String),

 #[error("Key generation failed: {0}")]
 KeyGenerationFailed(String),

 #[error("Signing failed: {0}")]
 SigningFailed(String),

 #[error("Address generation failed: {0}")]
 AddressGenerationFailed(String),

 #[error("Hex decode failed: {0}")]
 HexDecodeFailed(String),
}

#[derive(Debug, thiserror::Error)]
pub enum FileError {
 #[error("Failed to read file '{0}': {1}")]
 ReadFailed(String, String),

 #[error("Failed to write file '{0}': {1}")]
 WriteFailed(String, String),

 #[error("File not found: {0}")]
 NotFound(String),

 #[error("Permission denied: {0}")]
 PermissionDenied(String),

 #[error("Invalid path: {0}")]
 InvalidPath(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
 #[error("gRPC error: {0}")]
 GrpcError(String),

 #[error("Parse error: {0}")]
 ParseError(String),

 #[error("Response error: {0}")]
 ResponseError(String),

 #[error("Invalid response: {0}")]
 InvalidResponse(String),

 #[error("Service unavailable: {0}")]
 ServiceUnavailable(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
 #[error("Missing required field: {0}")]
 MissingRequired(String),

 #[error("Invalid value for '{0}': {1}")]
 InvalidValue(String, String),

 #[error("Conflicting options: {0}")]
 ConflictingOptions(String),

 #[error("Invalid format: {0}")]
 InvalidFormat(String),
}

// --- From conversions for external error types ---

impl From<std::io::Error> for NodeCliError {
 fn from(err: std::io::Error) -> Self {
 match err.kind() {
 std::io::ErrorKind::NotFound => {
 NodeCliError::File(FileError::NotFound(err.to_string()))
 }
 std::io::ErrorKind::PermissionDenied => {
 NodeCliError::File(FileError::PermissionDenied(err.to_string()))
 }
 _ => NodeCliError::File(FileError::ReadFailed(
 "unknown".to_string(),
 err.to_string(),
 )),
 }
 }
}

impl From<reqwest::Error> for NodeCliError {
 fn from(err: reqwest::Error) -> Self {
 if err.is_timeout() {
 NodeCliError::Network(NetworkError::Timeout(err.to_string()))
 } else if err.is_connect() {
 NodeCliError::Network(NetworkError::ConnectionFailed(err.to_string()))
 } else {
 NodeCliError::Network(NetworkError::RequestFailed(err.to_string()))
 }
 }
}

impl From<serde_json::Error> for NodeCliError {
 fn from(err: serde_json::Error) -> Self {
 NodeCliError::Api(ApiError::ParseError(err.to_string()))
 }
}

impl From<secp256k1::Error> for NodeCliError {
 fn from(err: secp256k1::Error) -> Self {
 NodeCliError::Crypto(CryptoError::InvalidPrivateKey(err.to_string()))
 }
}

impl From<hex::FromHexError> for NodeCliError {
 fn from(err: hex::FromHexError) -> Self {
 NodeCliError::Crypto(CryptoError::HexDecodeFailed(err.to_string()))
 }
}

impl From<String> for NodeCliError {
 fn from(err: String) -> Self {
 NodeCliError::General(err)
 }
}

impl From<&str> for NodeCliError {
 fn from(err: &str) -> Self {
 NodeCliError::General(err.to_string())
 }
}

impl From<Box<dyn Error>> for NodeCliError {
 fn from(err: Box<dyn Error>) -> Self {
 NodeCliError::General(err.to_string())
 }
}

impl From<tonic::Status> for NodeCliError {
 fn from(err: tonic::Status) -> Self {
 NodeCliError::Api(ApiError::GrpcError(format!(
 "{}: {}",
 err.code(),
 err.message()
 )))
 }
}

/// Result type alias for convenience
pub type Result<T> = std::result::Result<T, NodeCliError>;

/// Helper functions for creating specific error types
impl NodeCliError {
 pub fn network_connection_failed(msg: &str) -> Self {
 NodeCliError::Network(NetworkError::ConnectionFailed(msg.to_string()))
 }

 pub fn network_http_error(code: u16, msg: &str) -> Self {
 NodeCliError::Network(NetworkError::HttpError(code, msg.to_string()))
 }

 pub fn crypto_invalid_private_key(msg: &str) -> Self {
 NodeCliError::Crypto(CryptoError::InvalidPrivateKey(msg.to_string()))
 }

 pub fn crypto_invalid_public_key(msg: &str) -> Self {
 NodeCliError::Crypto(CryptoError::InvalidPublicKey(msg.to_string()))
 }

 pub fn file_read_failed(path: &str, msg: &str) -> Self {
 NodeCliError::File(FileError::ReadFailed(path.to_string(), msg.to_string()))
 }

 pub fn file_write_failed(path: &str, msg: &str) -> Self {
 NodeCliError::File(FileError::WriteFailed(path.to_string(), msg.to_string()))
 }

 pub fn config_missing_required(field: &str) -> Self {
 NodeCliError::Config(ConfigError::MissingRequired(field.to_string()))
 }

 pub fn config_invalid_value(field: &str, msg: &str) -> Self {
 NodeCliError::Config(ConfigError::InvalidValue(
 field.to_string(),
 msg.to_string(),
 ))
 }

 pub fn http_error(msg: &str) -> Self {
 NodeCliError::Network(NetworkError::RequestFailed(msg.to_string()))
 }

 pub fn websocket_error(msg: &str) -> Self {
 NodeCliError::Network(NetworkError::ConnectionFailed(msg.to_string()))
 }

 pub fn parse_error(msg: &str) -> Self {
 NodeCliError::Api(ApiError::ParseError(msg.to_string()))
 }

 pub fn io_error(msg: &str) -> Self {
 NodeCliError::File(FileError::ReadFailed("io".to_string(), msg.to_string()))
 }
}
