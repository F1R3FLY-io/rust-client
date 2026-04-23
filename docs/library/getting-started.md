# Library Usage

Add as a dependency with `default-features = false` to exclude CLI dependencies:

```toml
[dependencies]
node_cli = { git = "https://github.com/F1R3FLY-io/rust-client.git", default-features = false }
```

## ConnectionManager

The primary library API. Handles deploy orchestration, finalization, and data reads.

```rust
use node_cli::{ConnectionConfig, F1r3flyConnectionManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // From environment variables
    let manager = F1r3flyConnectionManager::from_env()?;

    // Or explicit config
    let config = ConnectionConfig::new(
        "localhost".into(), 40401, 40403, "your_private_key_hex".into(),
    );
    let manager = F1r3flyConnectionManager::new(config);

    Ok(())
}
```

### With observer node

```rust
let config = ConnectionConfig::new("localhost".into(), 40412, 40413, key.into())
    .with_observer("localhost".into(), 40452);
```

### Deploy and wait

```rust
let result = manager
    .deploy_and_wait("new x in { x!(1) }", false, 0)
    .await?;

println!("Deploy: {}", result.deploy_id);
println!("Block:  {}", result.block_hash);
println!("Cost:   {:?}", result.cost);
println!("Data:   {:?}", result.data);  // Vec<Par> from deployId channel
```

### Read-only query

```rust
let result = manager.query(r#"new x in { x!(1 + 1) }"#).await?;
```

### Estimate cost

```rust
let cost = manager.estimate_cost(r#"new x in { x!(1 + 1) }"#).await?;
println!("Estimated cost: {} phlogiston", cost);
```

### Transfer

```rust
let transfer = manager.transfer("1111recipient...", 100_000_000).await?;
println!("TX: {} in block {}", transfer.deploy_id, transfer.block_hash);
```

## F1r3flyApi (Low-Level)

For single gRPC/HTTP operations without orchestration:

```rust
use node_cli::F1r3flyApi;

let api = F1r3flyApi::new("private_key_hex", "localhost", 40412)?;

// Deploy only (no waiting)
let deploy_id = api.deploy("new x in { x!(1) }", false, "rholang", 0).await?;

// Exploratory deploy (read-only)
let (data, block_info) = api.exploratory_deploy("new x in { x!(1) }", None, false).await?;

// Read data at deploy ID
let pars = api.get_data_at_deploy_id(&deploy_id, &block_hash).await?;

// Check finalization
let finalized = api.is_finalized(&block_hash, 12, 5).await?;
```

## Types

```rust
use node_cli::{DeployResult, DeployDetail, ProposeResult};

// DeployResult — from deploy_and_wait
pub struct DeployResult {
    pub deploy_id: String,
    pub block_hash: String,
    pub block_number: Option<i64>,
    pub cost: Option<u64>,
    pub errored: bool,
    pub system_deploy_error: Option<String>,
    pub data: Vec<Par>,  // from deployId channel
}

// DeployDetail — from get_deploy_detail (HTTP)
pub struct DeployDetail {
    pub block_hash: String,
    pub block_number: i64,
    pub cost: u64,
    pub errored: bool,
    pub system_deploy_error: String,
    // ... plus deployer, term, phlo, sig, timestamp
}

// ProposeResult — from propose
pub enum ProposeResult {
    Proposed(String),  // block hash
    Skipped(String),   // reason (e.g., NoNewDeploys)
}
```

## Configuration

| Field | Default | Description |
|-------|---------|-------------|
| `node_host` | `localhost` | Node hostname |
| `grpc_port` | `40401` | gRPC port |
| `http_port` | `40403` | HTTP port |
| `signing_key` | required | Private key (hex) |
| `observer_host` | same as node | Observer for finalization |
| `observer_grpc_port` | `40452` | Observer gRPC port |
| `deploy_timeout_secs` | `60` | Max seconds for block inclusion |
| `finalization_timeout_secs` | `30` | Max seconds for finalization |
| `poll_interval_secs` | `2` | Seconds between polls |
