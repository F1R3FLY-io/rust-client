use clap::{ArgAction, Args, Parser, Subcommand};
use std::path::PathBuf;

use crate::commands::CliResult;
use crate::connection_manager::{DEFAULT_GRPC_URL, DEFAULT_HTTP_URL};
use crate::endpoint::Endpoint;
use crate::f1r3fly_api::F1r3flyApi;
use crate::node_http::NodeHttp;
use crate::signing_key::SigningKey;

fn parse_endpoint(url: &str) -> Result<Endpoint, String> {
    Endpoint::parse(url).map_err(|e| e.to_string())
}

/// How every endpoint of a command is reached: trusted certificate
/// authorities and timeouts
#[derive(Args, Debug, Clone)]
pub struct TransportArgs {
    /// PEM file of the certificate authorities that https endpoints are
    /// trusted against, in place of the bundled public roots
    #[arg(long = "ca-cert", env = "FIREFLY_CA_CERT")]
    pub ca_cert: Option<PathBuf>,

    /// Seconds a connection to a node may take to establish
    #[arg(
        long = "connect-timeout",
        env = "FIREFLY_CONNECT_TIMEOUT",
        default_value_t = crate::endpoint::DEFAULT_CONNECT_TIMEOUT.as_secs(),
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    pub connect_timeout: u64,

    /// Seconds a single request to a node may take
    #[arg(
        long = "timeout",
        env = "FIREFLY_REQUEST_TIMEOUT",
        default_value_t = crate::endpoint::DEFAULT_REQUEST_TIMEOUT.as_secs(),
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    pub timeout: u64,
}

impl TransportArgs {
    /// `endpoint`, with this command's certificate authorities and timeouts.
    pub fn apply(&self, endpoint: &Endpoint) -> CliResult<Endpoint> {
        let endpoint = endpoint
            .clone()
            .with_connect_timeout(std::time::Duration::from_secs(self.connect_timeout))
            .with_request_timeout(std::time::Duration::from_secs(self.timeout));
        let Some(path) = &self.ca_cert else {
            return Ok(endpoint);
        };
        let pem = std::fs::read(path).map_err(|cause| crate::Error::Io {
            path: path.clone(),
            cause,
        })?;
        Ok(endpoint.with_ca_certificates(pem)?)
    }
}

/// Where a command's signing key comes from. A key is never taken as a flag
/// value, which would leave it in shell history and the process list.
#[derive(Args, Debug, Clone)]
pub struct KeyArgs {
    /// File holding the signing key's hex; it must not be readable by group or
    /// others. Without it, the key is read from FIREFLY_PRIVATE_KEY
    #[arg(long = "private-key-file", env = "FIREFLY_PRIVATE_KEY_FILE")]
    pub private_key_file: Option<PathBuf>,
}

impl KeyArgs {
    pub fn load(&self) -> CliResult<SigningKey> {
        Ok(self.load_optional()?.ok_or(crate::Error::MissingConfig {
            var: "--private-key-file or FIREFLY_PRIVATE_KEY",
        })?)
    }

    /// The key, or `None` when neither source is set. Both set is an error,
    /// since signing with the one not intended would go unnoticed.
    pub fn load_optional(&self) -> CliResult<Option<SigningKey>> {
        if let Some(path) = &self.private_key_file {
            if std::env::var_os("FIREFLY_PRIVATE_KEY").is_some() {
                return Err(crate::Error::invalid(
                    "signing key",
                    format!(
                        "both a key file ({}) and FIREFLY_PRIVATE_KEY are set; use one",
                        path.display()
                    ),
                )
                .into());
            }
            return Ok(Some(SigningKey::from_file(path)?));
        }
        match std::env::var("FIREFLY_PRIVATE_KEY") {
            Ok(hex) => {
                Ok(Some(SigningKey::from_hex(&hex).map_err(|e| {
                    crate::Error::invalid("FIREFLY_PRIVATE_KEY", e)
                })?))
            }
            Err(std::env::VarError::NotPresent) => Ok(None),
            Err(std::env::VarError::NotUnicode(_)) => {
                Err(crate::Error::invalid("FIREFLY_PRIVATE_KEY", "not unicode").into())
            }
        }
    }
}

/// The node a command reaches over gRPC
#[derive(Args, Debug, Clone)]
pub struct GrpcNodeArgs {
    /// Node gRPC endpoint, http:// or https://, with no path
    #[arg(long = "grpc", env = "FIREFLY_GRPC_URL", default_value = DEFAULT_GRPC_URL, value_parser = parse_endpoint)]
    pub grpc: Endpoint,

    #[command(flatten)]
    pub transport: TransportArgs,
}

impl GrpcNodeArgs {
    pub fn api(&self) -> CliResult<F1r3flyApi> {
        Ok(F1r3flyApi::new(self.transport.apply(&self.grpc)?)?)
    }
}

/// The node a command reaches over HTTP
#[derive(Args, Debug, Clone)]
pub struct HttpNodeArgs {
    /// Node HTTP endpoint, http:// or https://, optionally with a base path
    #[arg(long = "http", env = "FIREFLY_HTTP_URL", default_value = DEFAULT_HTTP_URL, value_parser = parse_endpoint)]
    pub http: Endpoint,

    #[command(flatten)]
    pub transport: TransportArgs,
}

impl HttpNodeArgs {
    pub fn node(&self) -> CliResult<NodeHttp> {
        Ok(NodeHttp::new(self.transport.apply(&self.http)?)?)
    }
}

/// A read-only node a command queries over gRPC. There is no built-in
/// default: the default node ports are a validator's, which refuses these
/// queries.
#[derive(Args, Debug, Clone)]
pub struct ReadOnlyGrpcArgs {
    /// gRPC endpoint of a read-only node, http:// or https://, with no path
    #[arg(id = "readonly_grpc", long = "grpc", env = "FIREFLY_READONLY_GRPC_URL", value_parser = parse_endpoint)]
    pub grpc: Endpoint,

    #[command(flatten)]
    pub transport: TransportArgs,
}

impl ReadOnlyGrpcArgs {
    pub fn api(&self) -> CliResult<F1r3flyApi> {
        Ok(F1r3flyApi::new(self.transport.apply(&self.grpc)?)?)
    }
}

/// A read-only node a command queries over HTTP. There is no built-in
/// default: the default node ports are a validator's, which refuses these
/// queries.
#[derive(Args, Debug, Clone)]
pub struct ReadOnlyHttpArgs {
    /// HTTP endpoint of a read-only node, http:// or https://, optionally with a base path
    #[arg(id = "readonly_http", long = "http", env = "FIREFLY_READONLY_HTTP_URL", value_parser = parse_endpoint)]
    pub http: Endpoint,

    #[command(flatten)]
    pub transport: TransportArgs,
}

impl ReadOnlyHttpArgs {
    pub fn node(&self) -> CliResult<NodeHttp> {
        Ok(NodeHttp::new(self.transport.apply(&self.http)?)?)
    }
}

/// The node a command reaches over both gRPC and HTTP
#[derive(Args, Debug, Clone)]
pub struct NodeArgs {
    /// Node gRPC endpoint, http:// or https://, with no path
    #[arg(long = "grpc", env = "FIREFLY_GRPC_URL", default_value = DEFAULT_GRPC_URL, value_parser = parse_endpoint)]
    pub grpc: Endpoint,

    /// Node HTTP endpoint, http:// or https://, optionally with a base path
    #[arg(long = "http", env = "FIREFLY_HTTP_URL", default_value = DEFAULT_HTTP_URL, value_parser = parse_endpoint)]
    pub http: Endpoint,

    #[command(flatten)]
    pub transport: TransportArgs,
}

impl NodeArgs {
    pub fn api(&self) -> CliResult<F1r3flyApi> {
        Ok(F1r3flyApi::new(self.transport.apply(&self.grpc)?)?)
    }

    pub fn node(&self) -> CliResult<NodeHttp> {
        Ok(NodeHttp::new(self.transport.apply(&self.http)?)?)
    }
}

/// Command-line interface for interacting with F1r3fly nodes
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Config file holding named profiles (default: $XDG_CONFIG_HOME/f1r3fly/node_cli.toml,
    /// or ~/.config/f1r3fly/node_cli.toml)
    #[arg(long = "config", env = "FIREFLY_CONFIG", global = true)]
    pub config: Option<PathBuf>,

    /// Profile of the config file to take settings from (default: the file's default_profile)
    #[arg(long = "profile", env = "FIREFLY_PROFILE", global = true)]
    pub profile: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Deploy Rholang code to the F1r3fly network
    Deploy(DeployArgs),

    /// Propose a block to the F1r3fly network
    Propose(ProposeArgs),

    /// Deploy Rholang code, wait for finalization, and read result
    DeployAndWait(DeployAndWaitArgs),

    /// Read data at a deploy ID from a specific block
    GetData(GetDataArgs),

    /// Check if a block is finalized
    IsFinalized(IsFinalizedArgs),

    /// Execute Rholang code without committing to the blockchain (exploratory deployment; needs a read-only node)
    ExploratoryDeploy(ExploratoryDeployArgs),

    /// Estimate phlogiston cost of Rholang code without deploying
    EstimateCost(EstimateCostArgs),

    /// Generate a public key from a private key
    GeneratePublicKey(GeneratePublicKeyArgs),

    /// Generate a new secp256k1 private/public key pair
    GenerateKeyPair(GenerateKeyPairArgs),

    /// Generate a vault address from a public key
    GenerateVaultAddress(GenerateVaultAddressArgs),

    /// Get node status and peer information
    Status(HttpNodeArgs),

    /// Get recent blocks or specific block information
    Blocks(BlocksArgs),

    /// Get current validator bonds from PoS contract (needs a read-only node)
    Bonds(ReadOnlyHttpArgs),

    /// Get active validators from PoS contract (needs a read-only node)
    ActiveValidators(ReadOnlyHttpArgs),

    /// Check wallet balance for a specific address (needs a read-only node)
    WalletBalance(WalletBalanceArgs),

    /// Check if a validator is bonded
    BondStatus(BondStatusArgs),

    /// Get node metrics
    Metrics(HttpNodeArgs),

    /// Bond a new validator to the network (dynamic validator addition)
    BondValidator(BondValidatorArgs),

    /// Withdraw a validator's bond (takes effect at the next epoch boundary)
    UnbondValidator(UnbondValidatorArgs),

    /// Check network health across multiple nodes
    NetworkHealth(NetworkHealthArgs),

    /// Get the last finalized block
    LastFinalizedBlock(HttpNodeArgs),

    /// Get blocks in the main chain
    ShowMainChain(ShowMainChainArgs),

    /// Transfer tokens between addresses
    Transfer(TransferArgs),

    /// Run load test by sending multiple transfers and tracking orphan rate
    LoadTest(LoadTestArgs),

    /// Get a specific deploy by ID
    GetDeploy(GetDeployArgs),

    /// Get current epoch information and status (needs a read-only node)
    EpochInfo(ReadOnlyGrpcArgs),

    /// Check individual validator status: bonded, active, withdrawal progress (needs a read-only node)
    ValidatorStatus(ValidatorStatusArgs),

    /// Get current epoch rewards information (needs a read-only node)
    EpochRewards(ReadOnlyHttpArgs),

    /// Get network-wide consensus health overview (needs a read-only node)
    NetworkConsensus(ReadOnlyHttpArgs),

    /// Get blocks by height range
    GetBlocksByHeight(GetBlocksByHeightArgs),

    /// Extract node ID from TLS private key file
    GetNodeId(GetNodeIdArgs),

    /// Watch real-time node events via WebSocket
    WatchEvents(WatchEventsArgs),

    /// Interactive DAG visualization with real-time updates
    Dag(DagArgs),

    /// Get transfer information from a block's deploys (needs a read-only node)
    BlockTransfers(BlockTransfersArgs),

    /// Check canonical-state finalization status of a deploy by signature
    DeployStatus(DeployStatusArgs),

    /// Inspect the settings commands resolve from flags, environment and config profiles
    Config(ConfigArgs),
}

#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Show each setting's resolved value and where it came from: a flag, the
    /// environment, the selected profile, or the built-in default
    Show(Box<ConfigShowArgs>),
}

/// Every setting a profile can hold, under the same flags and environment
/// variables the commands read, so `config show` resolves them the same way.
#[derive(Args, Debug)]
pub struct ConfigShowArgs {
    #[arg(long = "grpc", env = "FIREFLY_GRPC_URL", default_value = DEFAULT_GRPC_URL, value_parser = parse_endpoint)]
    pub grpc: Endpoint,

    #[arg(long = "http", env = "FIREFLY_HTTP_URL", default_value = DEFAULT_HTTP_URL, value_parser = parse_endpoint)]
    pub http: Endpoint,

    #[arg(long = "observer", env = "FIREFLY_OBSERVER_URL", value_parser = parse_endpoint)]
    pub observer: Option<Endpoint>,

    #[arg(id = "internal_grpc", long = "internal-grpc", env = "FIREFLY_INTERNAL_GRPC_URL", default_value = DEFAULT_INTERNAL_GRPC_URL, value_parser = parse_endpoint)]
    pub internal_grpc: Endpoint,

    #[arg(long = "readonly-grpc", env = "FIREFLY_READONLY_GRPC_URL", value_parser = parse_endpoint)]
    pub readonly_grpc: Option<Endpoint>,

    #[arg(long = "readonly-http", env = "FIREFLY_READONLY_HTTP_URL", value_parser = parse_endpoint)]
    pub readonly_http: Option<Endpoint>,

    #[arg(long = "shard-id", env = "FIREFLY_SHARD_ID", default_value = crate::deploy::DEFAULT_SHARD_ID)]
    pub shard_id: String,

    #[command(flatten)]
    pub transport: TransportArgs,

    #[command(flatten)]
    pub key: KeyArgs,
}

/// Node, finalization-wait and expiration options of commands that deploy and
/// wait for the deploy to finalize
#[derive(Args, Debug)]
pub struct DeployWaitArgs {
    #[command(flatten)]
    pub node: NodeArgs,

    /// HTTP endpoint of a second node, such as the shard's read-only node,
    /// polled alongside the deploy node for the deploy's verdict
    #[arg(long = "observer", env = "FIREFLY_OBSERVER_URL", value_parser = parse_endpoint)]
    pub observer: Option<Endpoint>,

    /// Maximum seconds to wait for the deploy to finalize
    #[arg(long = "max-wait", default_value_t = 300)]
    pub max_wait: u64,

    #[command(flatten)]
    pub deploy: DeployOptionsArgs,
}

/// What a deploying command signs into its deploy, beyond the term and its
/// phlo limit, whose default differs per command
#[derive(Args, Debug)]
pub struct DeployOptionsArgs {
    /// Price per phlo, in the native token's base units
    #[arg(long = "phlo-price", default_value_t = crate::deploy::DEFAULT_PHLO_PRICE)]
    pub phlo_price: i64,

    /// Shard the deploy is signed for: the target node's casper.shard-name
    #[arg(long = "shard-id", env = "FIREFLY_SHARD_ID", default_value = crate::deploy::DEFAULT_SHARD_ID)]
    pub shard_id: String,

    /// Expiration timestamp in milliseconds (Unix epoch). Deploy becomes invalid after this time.
    /// Use 0 or omit for no expiration.
    #[arg(long)]
    pub expiration: Option<i64>,

    /// Expiration duration in seconds from now. Deploy becomes invalid after this duration.
    /// Mutually exclusive with --expiration.
    #[arg(long, conflicts_with = "expiration")]
    pub expires_in: Option<u64>,
}

impl DeployOptionsArgs {
    /// The deploy options these flags describe, with the command's phlo limit.
    pub fn options(&self, phlo_limit: u64) -> crate::deploy::DeployOptions {
        let expiration = match (self.expiration, self.expires_in) {
            (Some(at), _) => Some(at).filter(|at| *at != 0),
            (None, Some(seconds)) => Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("the system clock is set before 1970")
                    .as_millis() as i64
                    + seconds as i64 * 1000,
            ),
            (None, None) => None,
        };
        crate::deploy::DeployOptions {
            phlo_limit,
            phlo_price: self.phlo_price,
            expiration,
            timestamp: None,
        }
    }
}

#[derive(Parser, Debug)]
pub struct DeployAndWaitArgs {
    /// Rholang file to deploy
    #[arg(short, long)]
    pub file: String,

    #[command(flatten)]
    pub key: KeyArgs,

    #[command(flatten)]
    pub wait: DeployWaitArgs,

    /// Most phlo the deploy may spend
    #[arg(long = "phlo-limit", default_value_t = crate::deploy::DEFAULT_PHLO_LIMIT)]
    pub phlo_limit: u64,

    /// Also propose a block after deploy
    #[arg(long, default_value_t = false)]
    pub propose: bool,

    /// Seconds between finalization status polls
    #[arg(long = "check-interval", default_value_t = 2)]
    pub check_interval: u64,
}

#[derive(Parser, Debug)]
pub struct GetDataArgs {
    /// Deploy ID (hex) to read data from
    #[arg(short = 'd', long = "deploy-id")]
    pub deploy_id: String,

    /// Block hash containing the deploy
    #[arg(short = 'b', long = "block-hash")]
    pub block_hash: String,

    #[command(flatten)]
    pub node: GrpcNodeArgs,
}

#[derive(Parser, Debug)]
pub struct GetDeployArgs {
    /// Deploy ID to retrieve
    #[arg(short = 'd', long = "deploy-id")]
    pub deploy_id: String,

    #[command(flatten)]
    pub node: HttpNodeArgs,

    /// Output format (json, pretty, summary)
    #[arg(short = 'f', long = "format", default_value = "pretty")]
    pub format: String,

    /// Show full deploy details
    #[arg(long = "verbose")]
    pub verbose: bool,
}

/// Arguments for deploy-status command
#[derive(Parser)]
pub struct DeployStatusArgs {
    /// Deploy signature in hex (with or without 0x prefix)
    #[arg(short = 's', long = "sig")]
    pub sig: String,

    #[command(flatten)]
    pub node: HttpNodeArgs,

    /// Output format (json, pretty)
    #[arg(short = 'f', long = "format", default_value = "pretty")]
    pub format: String,
}

/// Arguments for deploy and full-deploy commands
#[derive(Parser)]
pub struct DeployArgs {
    /// Path to the Rholang file to deploy
    #[arg(short, long)]
    pub file: PathBuf,

    #[command(flatten)]
    pub key: KeyArgs,

    #[command(flatten)]
    pub node: GrpcNodeArgs,

    /// Most phlo the deploy may spend
    #[arg(long = "phlo-limit", default_value_t = crate::deploy::DEFAULT_PHLO_LIMIT)]
    pub phlo_limit: u64,

    #[command(flatten)]
    pub deploy: DeployOptionsArgs,
}

/// A node's internal gRPC API, the only one that serves propose.
pub const DEFAULT_INTERNAL_GRPC_URL: &str = "http://localhost:40402";

/// Arguments for propose command. Propose is served on the node's internal
/// gRPC API, not the external one deploys use.
#[derive(Parser)]
pub struct ProposeArgs {
    /// Node's internal gRPC endpoint, http:// or https://, with no path
    #[arg(id = "internal_grpc", long = "grpc", env = "FIREFLY_INTERNAL_GRPC_URL", default_value = DEFAULT_INTERNAL_GRPC_URL, value_parser = parse_endpoint)]
    pub grpc: Endpoint,

    #[command(flatten)]
    pub transport: TransportArgs,
}

impl ProposeArgs {
    pub fn api(&self) -> CliResult<F1r3flyApi> {
        Ok(F1r3flyApi::new(self.transport.apply(&self.grpc)?)?)
    }
}

/// Arguments for is-finalized command
#[derive(Parser)]
pub struct IsFinalizedArgs {
    /// Block hash to check
    #[arg(short, long)]
    pub block_hash: String,

    #[command(flatten)]
    pub node: GrpcNodeArgs,

    /// Maximum number of retry attempts
    #[arg(short, long, default_value_t = 12)]
    pub max_attempts: u32,

    /// Delay between retries in seconds
    #[arg(short, long, default_value_t = 5)]
    pub retry_delay: u64,
}

/// Arguments for exploratory-deploy command. Only a read-only node serves it.
#[derive(Parser)]
pub struct ExploratoryDeployArgs {
    /// Path to the Rholang file to execute
    #[arg(short, long)]
    pub file: PathBuf,

    #[command(flatten)]
    pub node: ReadOnlyGrpcArgs,

    /// Block hash to use as reference (optional)
    #[arg(short, long)]
    pub block_hash: Option<String>,

    /// Use pre-state hash instead of post-state hash
    #[arg(short, long, default_value_t = false)]
    pub use_pre_state: bool,
}

/// Arguments for estimate-cost command
#[derive(Parser)]
pub struct EstimateCostArgs {
    #[arg(short, long)]
    pub file: PathBuf,

    #[command(flatten)]
    pub node: HttpNodeArgs,

    /// Block hash to estimate against (optional)
    #[arg(long = "block-hash")]
    pub block_hash: Option<String>,

    /// Hex-encoded 65-byte uncompressed secp256k1 public key (04-prefixed).
    /// Supplying this ensures the cost estimate accounts for identity-dependent
    /// terms (e.g. vault transfers). Without it, the deployer is derived from
    /// the signing key when one is configured, and the estimate may otherwise
    /// be significantly lower than the real deploy cost.
    #[arg(long)]
    pub deployer: Option<String>,

    #[command(flatten)]
    pub key: KeyArgs,
}

/// Arguments for generate-public-key command
#[derive(Parser)]
pub struct GeneratePublicKeyArgs {
    #[command(flatten)]
    pub key: KeyArgs,

    /// Output public key in compressed format (shorter)
    #[arg(short, long, default_value_t = false)]
    pub compressed: bool,
}

/// Arguments for generate-key-pair command
#[derive(Parser)]
pub struct GenerateKeyPairArgs {
    /// Output public key in compressed format (shorter)
    #[arg(short, long, default_value_t = false)]
    pub compressed: bool,

    /// Save keys to files instead of displaying them
    #[arg(short, long, default_value_t = false)]
    pub save: bool,

    /// Output directory for saved keys (default: current directory)
    #[arg(short, long, default_value = ".")]
    pub output_dir: String,
}

/// Arguments for generate-vault-address command
#[derive(Parser)]
pub struct GenerateVaultAddressArgs {
    /// Public key in hex format (uncompressed format preferred). Without it,
    /// the public key is derived from the signing key
    #[arg(short, long, conflicts_with = "private_key_file")]
    pub public_key: Option<String>,

    #[command(flatten)]
    pub key: KeyArgs,
}

/// Arguments for blocks command
#[derive(Parser)]
pub struct BlocksArgs {
    #[command(flatten)]
    pub node: HttpNodeArgs,

    /// Number of recent blocks to fetch (default: 5)
    #[arg(short, long, default_value_t = 5)]
    pub number: u32,

    /// Specific block hash to fetch (optional)
    #[arg(short, long)]
    pub block_hash: Option<String>,
}

/// Arguments for show-main-chain command
#[derive(Parser)]
pub struct ShowMainChainArgs {
    #[command(flatten)]
    pub node: GrpcNodeArgs,

    /// Number of blocks to fetch from main chain (default: 10)
    #[arg(short, long, default_value_t = 10)]
    pub depth: u32,
}

/// Arguments for get-blocks-by-height command
#[derive(Parser)]
pub struct GetBlocksByHeightArgs {
    #[command(flatten)]
    pub node: GrpcNodeArgs,

    /// Start block number (inclusive)
    #[arg(short, long)]
    pub start_block_number: i64,

    /// End block number (inclusive)
    #[arg(short, long)]
    pub end_block_number: i64,
}

/// Arguments for wallet-balance command. Only a read-only node serves it.
#[derive(Parser)]
pub struct WalletBalanceArgs {
    #[command(flatten)]
    pub node: ReadOnlyHttpArgs,

    /// Wallet address to check balance for
    #[arg(short = 'a', long)]
    pub address: String,
}

/// Arguments for bond-status command. Any node serves it.
#[derive(Parser)]
pub struct BondStatusArgs {
    #[command(flatten)]
    pub node: HttpNodeArgs,

    /// Public key to check bond status for
    #[arg(short = 'k', long)]
    pub public_key: String,
}

/// Arguments for bond-validator command
#[derive(Parser)]
pub struct BondValidatorArgs {
    #[command(flatten)]
    pub wait: DeployWaitArgs,

    /// Stake amount for the validator (required)
    #[arg(short, long)]
    pub stake: String,

    /// Most phlo the deploy may spend
    #[arg(long = "phlo-limit", default_value_t = crate::pos::POS_CALL_PHLO_LIMIT)]
    pub phlo_limit: u64,

    #[command(flatten)]
    pub key: KeyArgs,

    /// Also propose a block after bonding
    #[arg(long, default_value_t = false, action = ArgAction::Set, value_parser = clap::value_parser!(bool))]
    pub propose: bool,

    /// Check interval in seconds for deploy status
    #[arg(long = "check-interval", default_value_t = 5)]
    pub check_interval: u64,
}

/// Arguments for unbond-validator command
#[derive(Parser)]
pub struct UnbondValidatorArgs {
    #[command(flatten)]
    pub wait: DeployWaitArgs,

    #[command(flatten)]
    pub key: KeyArgs,

    /// Most phlo the deploy may spend
    #[arg(long = "phlo-limit", default_value_t = crate::pos::POS_CALL_PHLO_LIMIT)]
    pub phlo_limit: u64,

    /// Check interval in seconds for deploy status
    #[arg(long = "check-interval", default_value_t = 5)]
    pub check_interval: u64,
}

/// Arguments for network-health command
#[derive(Parser)]
pub struct NetworkHealthArgs {
    /// HTTP endpoint of a node to check; repeat it for each node. Without any,
    /// the nodes of a single-host Docker shard on localhost are checked.
    #[arg(long = "http", value_parser = parse_endpoint)]
    pub nodes: Vec<Endpoint>,

    #[command(flatten)]
    pub transport: TransportArgs,

    /// Print more details about the results
    #[arg(short, long)]
    pub verbose: bool,

    /// Print underlying HTTP requests and responses
    #[arg(long)]
    pub debug: bool,
}

/// Arguments for transfer command
#[derive(Parser)]
pub struct TransferArgs {
    /// Recipient vault address
    #[arg(short, long)]
    pub to_address: String,

    /// Amount to transfer
    #[arg(short, long)]
    pub amount: String,
    /// Treat --amount as whole tokens, scaled by the native token's decimals from node status (default: base units / dust)
    #[arg(short = 'd', long = "whole-tokens", default_value_t = false)]
    pub whole_tokens: bool,

    #[command(flatten)]
    pub key: KeyArgs,

    #[command(flatten)]
    pub wait: DeployWaitArgs,

    /// Most phlo the deploy may spend
    #[arg(long = "phlo-limit", default_value_t = crate::vault::TRANSFER_PHLO_LIMIT)]
    pub phlo_limit: u64,

    /// Also propose a block after transfer
    #[arg(long, default_value_t = false, action = ArgAction::Set, value_parser = clap::value_parser!(bool))]
    pub propose: bool,

    /// Check interval in seconds for deploy status
    #[arg(long = "check-interval", default_value_t = 5)]
    pub check_interval: u64,
}

/// Arguments for load-test command
#[derive(Parser)]
pub struct LoadTestArgs {
    /// Recipient vault address
    #[arg(long)]
    pub to_address: String,

    /// Number of transfers to send
    #[arg(long, default_value_t = 20)]
    pub num_tests: u32,

    /// Amount per transfer
    #[arg(long, default_value = "1")]
    pub amount: String,

    /// Treat --amount as whole tokens, scaled by the native token's decimals from node status (default: base units / dust)
    #[arg(short = 'd', long = "whole-tokens", default_value_t = false)]
    pub whole_tokens: bool,

    /// Seconds between tests
    #[arg(long, default_value_t = 10)]
    pub interval: u64,

    #[command(flatten)]
    pub key: KeyArgs,

    #[command(flatten)]
    pub node: NodeArgs,

    /// HTTP endpoint of a read-only node, for balance queries
    #[arg(long = "readonly-http", env = "FIREFLY_READONLY_HTTP_URL", value_parser = parse_endpoint)]
    pub readonly_http: Endpoint,

    /// Check interval in seconds for deploy status (fast mode)
    #[arg(long = "check-interval", default_value_t = 1)]
    pub check_interval: u64,

    /// Max depth to check main chain for orphan detection
    #[arg(long = "chain-depth", default_value_t = 200)]
    pub chain_depth: u32,

    /// Maximum time in seconds to wait for block inclusion
    #[arg(long = "inclusion-timeout", default_value_t = 120)]
    pub inclusion_timeout: u64,

    /// Maximum time in seconds to wait for block finalization
    #[arg(long = "finalization-timeout", default_value_t = 120)]
    pub finalization_timeout: u64,

    /// Most phlo each transfer deploy may spend
    #[arg(long = "phlo-limit", default_value_t = crate::vault::TRANSFER_PHLO_LIMIT)]
    pub phlo_limit: u64,

    #[command(flatten)]
    pub deploy: DeployOptionsArgs,
}

/// Arguments for validator-status command
#[derive(Parser)]
pub struct ValidatorStatusArgs {
    /// Validator public key to check (hex format)
    #[arg(short = 'k', long)]
    pub public_key: String,

    /// Most recent heights to look for blocks the validator authored
    #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u32).range(1..))]
    pub window: u32,

    #[command(flatten)]
    pub node: ReadOnlyHttpArgs,
}

/// Arguments for get-node-id command
#[derive(Parser, Debug)]
pub struct GetNodeIdArgs {
    /// Path to the TLS private key file (node.key.pem)
    #[arg(short = 'k', long, conflicts_with = "cert_file")]
    pub key_file: Option<String>,

    /// Path to the TLS certificate file (node.certificate.pem)
    #[arg(short = 'c', long, conflicts_with = "key_file")]
    pub cert_file: Option<String>,

    /// Output format (hex, rnode-url)
    #[arg(short = 'f', long, default_value = "hex")]
    pub format: String,

    /// Node hostname for rnode-url format
    #[arg(short = 'H', long, default_value = "localhost")]
    pub host: String,

    /// Protocol port for rnode-url format
    #[arg(long, default_value_t = 40400)]
    pub protocol_port: u16,

    /// Discovery port for rnode-url format
    #[arg(long, default_value_t = 40404)]
    pub discovery_port: u16,
}

/// Arguments for watch-events command
#[derive(Parser, Debug)]
pub struct WatchEventsArgs {
    #[command(flatten)]
    pub node: HttpNodeArgs,

    /// Filter events by type: created, added, finalized (or finalised), transfers, genesis, lifecycle
    #[arg(short, long)]
    pub filter: Option<String>,

    /// Retry reconnection indefinitely until manually killed (Ctrl+C)
    #[arg(long, default_value_t = false)]
    pub retry_forever: bool,
}

/// Arguments for dag. Blocks are read from, and live events followed on, the
/// one HTTP endpoint.
#[derive(Parser, Debug)]
pub struct DagArgs {
    #[command(flatten)]
    pub node: HttpNodeArgs,

    /// Initial number of blocks to load
    #[arg(short, long, default_value_t = 50)]
    pub depth: usize,

    /// Disable real-time updates (static view)
    #[arg(long, default_value_t = false)]
    pub no_live: bool,

    /// Show deploy counts inline
    #[arg(long, default_value_t = true)]
    pub show_deploys: bool,
}

/// Arguments for block-transfers command
#[derive(Parser, Debug)]
pub struct BlockTransfersArgs {
    /// Block hash to get transfers from
    pub block_hash: String,

    #[command(flatten)]
    pub node: ReadOnlyHttpArgs,

    /// Show all deploys, not just those with transfers
    #[arg(long = "all-deploys", default_value_t = false)]
    pub all_deploys: bool,

    /// Maximum seconds to wait for the node to produce the block's transfer report
    #[arg(long = "max-wait", default_value_t = 60)]
    pub max_wait: u64,
}

#[cfg(test)]
mod bigint_tests {
    use super::*;

    #[test]
    fn transfer_keeps_raw_units_default_and_whole_token_flag() {
        let raw = TransferArgs::try_parse_from([
            "transfer",
            "--to-address",
            "recipient",
            "--amount",
            "18446744073709551616",
        ])
        .unwrap();
        assert_eq!(raw.amount, "18446744073709551616");
        assert!(!raw.whole_tokens);
        let tokens = TransferArgs::try_parse_from([
            "transfer",
            "--to-address",
            "recipient",
            "--amount",
            "1.000000000000000001",
            "--whole-tokens",
        ])
        .unwrap();
        assert_eq!(tokens.amount, "1.000000000000000001");
        assert!(tokens.whole_tokens);
    }
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn every_command_parses_without_conflicting_flags() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }

    fn status(args: &[&str]) -> Result<HttpNodeArgs, clap::Error> {
        let argv = ["node_cli", "status"].iter().chain(args);
        match Cli::try_parse_from(argv)?.command {
            Commands::Status(args) => Ok(args),
            _ => unreachable!("parsed the status command"),
        }
    }

    #[test]
    fn timeouts_reach_the_endpoint_and_zero_is_refused() {
        let args = status(&[
            "--http",
            "https://proxy.example/node1",
            "--connect-timeout",
            "3",
            "--timeout",
            "120",
        ])
        .unwrap();
        let endpoint = args.transport.apply(&args.http).unwrap();
        assert_eq!(endpoint.connect_timeout(), Duration::from_secs(3));
        assert_eq!(endpoint.request_timeout(), Duration::from_secs(120));
        assert_eq!(endpoint.to_string(), "https://proxy.example/node1");

        let defaults = status(&[]).unwrap();
        let endpoint = defaults.transport.apply(&defaults.http).unwrap();
        assert_eq!(
            endpoint.request_timeout(),
            crate::endpoint::DEFAULT_REQUEST_TIMEOUT
        );

        assert!(status(&["--timeout", "0"]).is_err());
    }

    #[test]
    fn a_grpc_endpoint_with_a_path_is_refused() {
        let Commands::Propose(args) =
            Cli::try_parse_from(["node_cli", "propose", "--grpc", "http://localhost:40402/x"])
                .unwrap()
                .command
        else {
            unreachable!("parsed the propose command")
        };
        assert!(args.api().is_err());
    }

    #[test]
    fn propose_defaults_to_the_internal_api() {
        let Commands::Propose(args) = Cli::try_parse_from(["node_cli", "propose"])
            .unwrap()
            .command
        else {
            unreachable!("parsed the propose command")
        };
        assert_eq!(args.grpc.to_string(), DEFAULT_INTERNAL_GRPC_URL);
    }

    #[test]
    fn read_only_queries_need_an_explicit_endpoint() {
        for args in [
            &["node_cli", "bonds"][..],
            &["node_cli", "epoch-info"],
            &["node_cli", "wallet-balance", "-a", "1111x"],
            &["node_cli", "exploratory-deploy", "-f", "q.rho"],
            &["node_cli", "validator-status", "-k", "04ab"],
            &["node_cli", "block-transfers", "abcd"],
        ] {
            assert!(Cli::try_parse_from(args).is_err(), "{args:?} was accepted");
        }
        assert!(
            Cli::try_parse_from(["node_cli", "bonds", "--http", "http://observer:40453"]).is_ok()
        );
    }

    #[test]
    fn a_ca_with_a_plaintext_endpoint_is_refused() {
        let ca = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let dir = std::env::temp_dir().join(format!("node-cli-ca-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ca.pem");
        std::fs::write(&path, ca.cert.pem()).unwrap();

        let args = status(&[
            "--http",
            "http://localhost:40403",
            "--ca-cert",
            path.to_str().unwrap(),
        ])
        .unwrap();
        assert!(args.transport.apply(&args.http).is_err());
        let args = status(&[
            "--http",
            "https://localhost:40403",
            "--ca-cert",
            path.to_str().unwrap(),
        ])
        .unwrap();
        assert!(args.transport.apply(&args.http).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
