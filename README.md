# F1r3fly Node CLI

A Rust crate for interacting with F1r3fly nodes — usable as both a **library** and a **CLI tool**.

## Using as a Library

Add `node_cli` as a dependency with `default-features = false` to avoid pulling in CLI dependencies (`clap`, `ratatui`, `crossterm`):

```toml
[dependencies]
node_cli = { git = "https://github.com/F1R3FLY-io/rust-client.git", default-features = false }
```

### Library Modules

| Module | Description |
|--------|-------------|
| `connection_manager` | High-level async API with deploy orchestration, observer support, and finalization |
| `grpc` | gRPC client split into focused submodules (deploy, query, blocks, http) |
| `events` | WebSocket event streaming for real-time deploy finalization via `/ws/events` |
| `vault` | Native token transfer and balance operations |
| `registry` | Cryptographic functions for `rho:registry:insertSigned:secp256k1` |
| `rholang_helpers` | Parsing Rholang expression responses into plain JSON |
| `signing` | Deploy data signing (Blake2b-256 + secp256k1 ECDSA) |
| `f1r3fly_api` | Core types (`DeployDetail`, `DeployResult`, `ProposeResult`) and re-exports |
| `error` | Error types using `thiserror` |
| `utils` | Cryptographic utilities (key derivation, vault address generation) |

### Quick Start

```rust
use node_cli::{ConnectionConfig, F1r3flyConnectionManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure from environment variables
    let manager = F1r3flyConnectionManager::from_env()?;

    // Or configure explicitly
    let config = ConnectionConfig::new(
        "localhost".to_string(),
        40401,   // gRPC port
        40403,   // HTTP port
        "your_private_key_hex".to_string(),
    );
    let manager = F1r3flyConnectionManager::new(config);

    // Read-only query (exploratory deploy)
    let result = manager.query(r#"new x in { x!(1 + 1) }"#).await?;

    // Deploy and wait for finalization
    let (deploy_id, block_hash) = manager
        .deploy_and_wait(r#"new x in { x!("hello") }"#, false, 0)
        .await?;

    // Deploy, wait, and read result data
    let result = manager
        .full_deploy_and_wait(r#"new deployId(`rho:system:deployId`) in { deployId!(42) }"#, false, 0)
        .await?;
    println!("Cost: {:?}, Data: {:?}", result.cost, result.data);

    // Transfer native tokens (amount in dust; 1 token = 100,000,000 dust)
    let transfer = manager
        .transfer("1111recipient_address_here", 100_000_000)
        .await?;

    Ok(())
}
```

### WebSocket Events

Listen for deploy finalization in real-time without polling:

```rust
use node_cli::NodeEvents;
use std::time::Duration;

let events = NodeEvents::connect("ws://localhost:40403");

// Wait for a specific deploy to be finalized (up to 2 minutes)
if let Some(event) = events.wait_for_deploy(&deploy_id, Duration::from_secs(120)).await {
    println!("Deploy finalized! Cost: {}, Errored: {}", event.cost, event.errored);
}
```

### Observer Node Support

For finalization checks on shards, configure an observer node:

```rust
let config = ConnectionConfig::new("localhost".into(), 40412, 40413, key.into())
    .with_observer("localhost".into(), 40452);
let manager = F1r3flyConnectionManager::new(config);
```

### Re-exports

```rust
use node_cli::{
    ConnectionConfig, ConnectionError, F1r3flyConnectionManager,
    F1r3flyApi, DeployDetail, DeployResult, ProposeResult,
    NodeEvents, TransferResult, DUST_FACTOR,
};
```

### Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `FIREFLY_PRIVATE_KEY` | Yes | -- | Private key for signing deploys (64 hex chars) |
| `FIREFLY_HOST` | No | `localhost` | Node hostname |
| `FIREFLY_GRPC_PORT` | No | `40401` | gRPC port for deploy/propose |
| `FIREFLY_HTTP_PORT` | No | `40403` | HTTP port for status/query |
| `FIREFLY_OBSERVER_HOST` | No | same as host | Observer node for finalization checks |
| `FIREFLY_OBSERVER_GRPC_PORT` | No | `40452` | Observer gRPC port |
| `FIREFLY_DEPLOY_TIMEOUT` | No | `300` | Max seconds to wait for deploy inclusion |

## CLI Usage

### Prerequisites

- [Running Node](https://github.com/F1R3FLY-io/f1r3fly/tree/rust/dev?tab=readme-ov-file#running)

> The commands work out of the box with the Docker setup in the [f1r3node Docker README](https://github.com/F1R3FLY-io/f1r3node/blob/main/docker/README.md).

### Building

```bash
cargo build --release
```

### Deploy Commands

```bash
# Deploy only
cargo run -- deploy -f contract.rho

# Deploy + propose
cargo run -- full-deploy -f contract.rho

# Deploy, wait for block inclusion and finalization
cargo run -- deploy-and-wait -f contract.rho

# Deploy, wait for finalization, and read result data
cargo run -- full-deploy-and-wait -f contract.rho

# With observer node for finalization
cargo run -- deploy-and-wait -f contract.rho --observer-host localhost --observer-port 40452

# With bigger phlo limit
cargo run -- deploy -f contract.rho -b
```

### Read Data

```bash
# Read-only query (exploratory deploy)
cargo run -- exploratory-deploy -f query.rho

# Read deploy result data by deploy ID and block hash
cargo run -- get-data -d DEPLOY_ID -b BLOCK_HASH

# Get deploy execution details (cost, errored, block number)
cargo run -- get-deploy -d DEPLOY_ID

# Output formats: pretty (default), json, summary
cargo run -- get-deploy -d DEPLOY_ID --format json
```

### Block and Network

```bash
# Propose a block
cargo run -- propose

# Check if a block is finalized
cargo run -- is-finalized -b BLOCK_HASH

# Get node status
cargo run -- status

# Get recent blocks
cargo run -- blocks -n 10

# Get last finalized block
cargo run -- last-finalized-block

# Watch real-time block events
cargo run -- watch-blocks
```

### Key Management

```bash
# Generate a new key pair
cargo run -- generate-key-pair --save

# Generate public key from private key
cargo run -- generate-public-key --private-key YOUR_KEY

# Generate vault address
cargo run -- generate-vault-address
```

### Token Operations

```bash
# Transfer tokens
cargo run -- transfer --to-address 1111... --amount 100

# Check wallet balance
cargo run -- wallet-balance --address 1111...

# Check validator bonds
cargo run -- bonds
```

### Network Health

```bash
# Check network health across nodes
cargo run -- network-health

# Recursive peer discovery
cargo run -- network-health --recursive --depth 3
```

### Advanced

```bash
# Load test with orphan detection
cargo run -- load-test --count 10 --amount 1

# Interactive DAG visualization
cargo run -- dag

# Get blocks by height range
cargo run -- get-blocks-by-height --start 1 --end 10

# Get block transfer details
cargo run -- block-transfers BLOCK_HASH

# Extract node ID from TLS key
cargo run -- get-node-id --cert-file node.certificate.pem
```

## Architecture

See [docs/architecture.md](docs/architecture.md) for detailed documentation on deploy flows, node API endpoints, tip sampling, and event types.

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `f1r3fly-models` | git (rust/dev) | Protobuf types (DeployDataProto, LightBlockInfo, Par) |
| `f1r3fly-shared` | git (rust/dev) | Event types (F1r3flyEvent, DeployEvent) |
| `f1r3fly-crypto` | git (rust/dev) | Key derivation |
| `f1r3fly-rholang` | git (rust/dev) | Vault address generation |
| `secp256k1` | 0.31 | ECDSA signing |
| `reqwest` | 0.12 | HTTP client |
| `tonic` | 0.14 | gRPC client |
| `tokio-tungstenite` | 0.26 | WebSocket client |
| `tracing` | 0.1 | Structured logging |
| `thiserror` | 2.0 | Error derives |
