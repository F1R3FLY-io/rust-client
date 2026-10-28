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
async fn main() -> node_cli::Result<()> {
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
    .with_observer("localhost".into(), 40452, 40453);
```

`with_observer` takes the observer's gRPC port and its HTTP port. The HTTP port
is the one polled for finalization status, so it must be set explicitly rather
than derived.

### Deploy and wait

```rust
let result = manager
    .deploy_and_wait("new x in { x!(1) }", false, 0)
    .await?;

println!("Deploy: {}", result.deploy_id);
println!("Block:  {}", result.block_hash);
println!("Cost:   {:?}", result.cost);
match &result.data {
    Ok(pars) => println!("Data:   {pars:?}"),  // empty when the deploy wrote nothing
    Err(e) => eprintln!("deployId data could not be read: {e}"),
}
```

The boolean maps to a phlo limit of 50k (`false`) or 5B (`true`). When the
deploy's cost falls between those — and the deployer's balance must cover the
limit, so 5B is not a safe default — pass an explicit limit instead:

```rust
let result = manager
    .deploy_and_wait_with_phlo_limit("new x in { x!(1) }", 500_000, 0)
    .await?;
```

### Read-only query

```rust
let result = manager.query(r#"new x in { x!(1 + 1) }"#).await?;
```

### Estimate cost

```rust
// Without deployer identity — may underestimate identity-dependent terms
let cost = manager.estimate_cost(r#"new x in { x!(1 + 1) }", None).await?;

// With deployer public key — accurate for identity-dependent contracts
let cost = manager.estimate_cost(r#"new x in { x!(1 + 1) }", Some(&deployer_pubkey_hex)).await?;
println!("Estimated cost: {} phlogiston", cost);
```

### Transfer

```rust
// amount is in base units (dust); use the CLI --whole-tokens flag for whole-token amounts
let transfer = manager.transfer("1111recipient...", 100_000_000u64.into()).await?;
println!("TX: {} in block {}", transfer.deploy_id, transfer.block_hash);
```

Library transfers use `vault::TRANSFER_PHLO_LIMIT` (500k): the RevVault
transfer contract costs ~250k phlo, above the 50k default deploy limit. The
CLI's `transfer` is different — it defaults `--bigger-phlo` to true and so
deploys with a 5,000,000,000 limit.

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
    pub data: node_cli::Result<Vec<Par>>,  // deployId channel; Err when the read failed
}

// DeployDetail — from get_deploy_detail (HTTP)
pub struct DeployDetail {
    pub block_hash: String,
    pub block_number: i64,
    pub cost: u64,
    pub errored: bool,
    pub system_deploy_error: Option<String>,
    // ... plus deployer, term, phlo, sig, timestamp
}

// ProposeResult — from propose
pub enum ProposeResult {
    Proposed(String),  // block hash
    Skipped(String),   // reason (e.g., NoNewDeploys)
}
```

## Errors

Every library call returns `node_cli::Result<T>`, whose error is
`node_cli::Error`. It is `Send + Sync`, so futures that hold one stay `Send`,
and it is `#[non_exhaustive]`, so a `match` needs a wildcard arm. Branch on the
variant rather than the message:

| Variant | Meaning | Retry? |
|---|---|---|
| `Transport` | The request did not reach the node or the answer did not come back | For reads, yes. A deploy or transfer may already have been submitted: do not resend it blindly |
| `Http` | The node answered HTTP with an error status; `code` carries its error code, such as `readonly_node_required` | Depends on `code` |
| `Grpc` | A gRPC call ended with a non-OK status | Depends on the status |
| `NodeRejected` | The node handled the request and refused it, such as a deploy with a bad signature | No |
| `Decode` | The node's answer did not have the expected shape | No |
| `InvalidInput`, `MissingConfig` | An argument or environment variable was wrong or missing | No |
| `Io` | A local file could not be read or written | No |
| `DeployFailed`, `DeployExpired` | The deploy's terminal verdict | No; a new deploy is a new transaction |
| `DeployResultUnreadable` | The deploy finalized, but its result could not be read | Read the result again by deploy ID; never resubmit, its effects are final |
| `FinalizationTimeout` | No verdict within the wait; the deploy may still finalize | Wait again, do not redeploy |
| `ConflictingVerdicts`, `NodeTooOld` | The nodes disagree, or cannot serve the call | No |
| `ContractRejected` | The vault or PoS contract executed the call and refused it | No |

```rust
match manager.transfer("1111recipient...", amount).await {
    Ok(transfer) => println!("sent in {}", transfer.block_hash),
    Err(node_cli::Error::ContractRejected { reason, .. }) => eprintln!("refused: {reason}"),
    // The transfer may have landed: look before sending it again.
    Err(node_cli::Error::Transport { .. }) => { /* check the recipient's balance */ }
    Err(other) => return Err(other),
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
| `observer_http_port` | `40453` | Observer HTTP port polled for finalization status |
| `finalization_timeout_secs` | `90` | Max seconds to wait for a deploy to finalize |
| `poll_interval_secs` | `2` | Seconds between finalization status polls |

### Exact token amounts

Vault transfer arguments and `TransferResult.amount_dust` use
`num_bigint::BigUint`. Parse raw units with `vault::parse_amount(text)`;
convert decimal tokens with `vault::tokens_to_dust(text, decimals)`.
`vault::dust_to_tokens(&amount, decimals)` and
`TransferResult::amount_tokens(decimals)` return exact decimal strings.
These conversions return errors for excess precision or amounts above
`2^256 - 1`. The fixed-eight-decimal `DUST_FACTOR` export has been removed.

`pos::PosSnapshot` bond values and `pos::Withdrawal.stake` also use `BigUint`.
`vault::build_transfer_rholang` and `pos::build_bond_rholang` take borrowed
`BigUint` amounts and return `node_cli::Result<String>` after checking bounds.
Quarantine heights remain ordinary integers.

Transfer reports describe attempted amounts, including rejected transfers.
With the `cli` feature, `commands::events::TransferEvent.amount` uses signed
`num_bigint::BigInt` to preserve negative and oversized attempts. Outgoing
amounts and stored balances still use unsigned uint256 validation.
