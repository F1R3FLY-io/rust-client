# Library Usage

Add as a dependency with `default-features = false` to exclude CLI dependencies:

```toml
[dependencies]
node_cli = { git = "https://github.com/F1R3FLY-io/rust-client.git", default-features = false }
```

## ConnectionManager

The primary library API. Handles deploy orchestration, finalization, and data reads.

```rust
use node_cli::{ConnectionConfig, Endpoint, F1r3flyConnectionManager};

#[tokio::main]
async fn main() -> node_cli::Result<()> {
    // From environment variables
    let manager = F1r3flyConnectionManager::from_env()?;

    // Or explicit config
    let config = ConnectionConfig::new(
        Endpoint::parse("http://localhost:40401")?,
        Endpoint::parse("http://localhost:40403")?,
        "your_private_key_hex".into(),
    );
    let manager = F1r3flyConnectionManager::new(config)?;

    Ok(())
}
```

The manager builds its gRPC and HTTP clients once and reuses their connections
across calls.

### With observer node

```rust
let config = ConnectionConfig::new(
    Endpoint::parse("http://localhost:40412")?,
    Endpoint::parse("http://localhost:40413")?,
    key.into(),
)
.with_observer(Endpoint::parse("http://localhost:40453")?);
```

`with_observer` takes the HTTP endpoint of a second node, such as the shard's
read-only node, which is polled alongside the deploy node for the deploy's
verdict. Without an observer, only the deploy node is polled.

### Connecting over TLS

An `https://` endpoint connects over TLS; `http://` is plaintext. An `https`
endpoint trusts the bundled public root certificates unless it is given its own
certificate authorities, in which case it trusts only those:

```rust
let grpc = Endpoint::parse("https://node.example:40401")?
    .with_ca_certificates(std::fs::read("ca.pem")?)?;
```

gRPC, HTTP and WebSocket connections all honor the scheme and the endpoint's
certificate authorities. `with_ca_certificates` refuses an `http://` endpoint,
since it is plaintext. `from_env` applies `FIREFLY_CA_CERT` to every endpoint it
reads, so it fails if any of them is `http://`.

Each endpoint also carries a connect timeout and a request timeout
(`endpoint::DEFAULT_CONNECT_TIMEOUT`, 10 s, and `DEFAULT_REQUEST_TIMEOUT`, 30 s);
an open WebSocket stream is bounded only by the connect timeout. An HTTP
endpoint may carry a base path, for a node served under a prefix behind a
proxy; requests and the event WebSocket go under it. A gRPC endpoint takes no
path, and `F1r3flyApi::new` refuses one (check with `has_path()`).

```rust
use std::time::Duration;

let http = Endpoint::parse("https://proxy.example/node1")?
    .with_connect_timeout(Duration::from_secs(5))
    .with_request_timeout(Duration::from_secs(60));
```

### Deploy and wait

```rust
use node_cli::DeployOptions;

let result = manager
    .deploy_and_wait("new x in { x!(1) }", &DeployOptions::default())
    .await?;

println!("Deploy: {}", result.deploy_id);
println!("Block:  {}", result.block_hash);
let detail = result.detail?;            // block, cost, and whether execution failed
println!("Cost:   {}", detail.cost);
let pars = result.data?;                // empty when the deploy wrote nothing
```

`DeployOptions` carries what the deploy signs besides its term: `phlo_limit`
(default `deploy::DEFAULT_PHLO_LIMIT`, 50,000), `phlo_price` (default 1),
`expiration` (Unix ms, or `None`) and `timestamp` (`None` for automatic; set it
for contracts such as `insertSigned` that check the deploy's timestamp). The
deployer's vault must cover `phlo_limit × phlo_price`, so set the limit to what
the deploy needs rather than to something very large:

```rust
let options = DeployOptions::default().with_phlo_limit(500_000);
let result = manager.deploy_and_wait("new x in { x!(1) }", &options).await?;
```

The shard a deploy is signed for is a connection setting, `shard_id` (env
`FIREFLY_SHARD_ID`, default `root`); it must match the node's `casper.shard-name`.

### Sign now, send later

A deploy's ID is its signature, so it is known before any node sees the deploy.
Sign it, keep the bytes, and send them as often as needed with
`submit_and_wait`, so a deploy whose fate is unknown after a timeout or a crash
can be resent without deploying twice. While the deploy's validity window is
open, a node that already holds it answers with the same ID. Once the window
closes the node refuses it as expired, and `submit_and_wait` then looks the
deploy up on the deploy node and the observer: a deploy in a block is waited on
as usual, one in neither is reported refused. The low-level
`F1r3flyApi::submit` makes only the first promise.

```rust
let api = manager.api();
let unsigned = api.prepare("new x in { x!(1) }", &DeployOptions::default()).await?;
let signed = unsigned.sign(&secret_key);            // or with_signature(..) from a hardware key
store(signed.id(), signed.to_bytes());              // the ID is final from here on

let stored = node_cli::SignedDeploy::from_bytes(&bytes)?;   // verified on load
let result = manager.submit_and_wait(&stored).await?;
```

`UnsignedDeploy::digest()` is what an external signer signs (DER secp256k1
ECDSA); `with_signature(public_key, der)` verifies the signature before
accepting it, and replaces an upper-half S value, which signers such as cloud
KMS can produce and nodes reject, with its lower-half equivalent.
`SignedDeploy::from_bytes` rejects an upper-half S signature instead, since
normalizing it would change the ID it was stored under. Automatic timestamps never repeat within a process, so two
identical deploys sent at once get different IDs.

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

Transfers, from the library and the CLI alike, use `vault::TRANSFER_PHLO_LIMIT`
(500k): the RevVault transfer contract costs ~250k phlo, above the 50k default
deploy limit. Bonds and withdrawals use `pos::POS_CALL_PHLO_LIMIT` (5B).

## F1r3flyApi (Low-Level)

For single gRPC operations without orchestration. Without a signing key the
client reads and sends pre-signed deploys:

```rust
use node_cli::{Endpoint, F1r3flyApi};

let api = F1r3flyApi::new(Endpoint::parse("http://localhost:40401")?)?
    .with_signing_key("private_key_hex")?;

// Deploy only (no waiting)
let deploy_id = api.deploy("new x in { x!(1) }", &DeployOptions::default()).await?;

// Exploratory deploy (read-only)
let (data, block_info) = api.exploratory_deploy("new x in { x!(1) }", None, false).await?;

// Read data at deploy ID
let pars = api.get_data_at_deploy_id(&deploy_id, &block_hash).await?;

// Check finalization
let finalized = api.is_finalized(&block_hash, 12, 5).await?;
```

`F1r3flyApi` is gRPC only. A node's HTTP API is `NodeHttp`:

```rust
use node_cli::NodeHttp;

let http = NodeHttp::new(Endpoint::parse("http://localhost:40403")?)?;

let detail = http.get_deploy_detail(&deploy_id).await?;        // Option<DeployDetail>
let status = http.deploy_finalization_status(&deploy_id).await?;
let cost = http.estimate_cost(term, Some(&deployer_pubkey_hex), None).await?;
let result = http.explore_deploy(term).await?;                 // retries 503 capacity rejections
let status_json = http.get_json("/api/status").await?;
```

It also has `get_deploy`, `get_deploy_block_hash` and `get_text`.

## Types

```rust
use node_cli::{DeployResult, DeployDetail, ProposeResult};

// DeployResult — from deploy_and_wait and submit_and_wait: a finalized deploy
pub struct DeployResult {
    pub deploy_id: String,
    pub block_hash: String,
    pub detail: node_cli::Result<DeployDetail>,  // execution; Err when the read failed
    pub data: node_cli::Result<Vec<Par>>,        // deployId channel; Err when the read failed
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
| `Transport` | The request did not reach the node or the answer did not come back | For reads, yes. For a deploy, resend the same `SignedDeploy` with `submit_and_wait`; a new deploy could execute twice |
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
| `grpc` | `http://localhost:40401` | Deploy node's gRPC endpoint |
| `http` | `http://localhost:40403` | Deploy node's HTTP endpoint: finalization status and deploy details |
| `observer` | none | HTTP endpoint of a second node polled for the deploy's verdict |
| `signing_key` | required | Private key (hex) |
| `finalization_timeout_secs` | `90` | Max seconds to wait for a deploy to finalize |
| `poll_interval_secs` | `2` | Seconds between finalization status polls |
| `shard_id` | `root` | Shard deploys are signed for: the node's `casper.shard-name` |

The defaults are `connection_manager::DEFAULT_GRPC_URL` and `DEFAULT_HTTP_URL`.
Certificate authorities for `https` endpoints are set on each `Endpoint` with
`with_ca_certificates`, or by `FIREFLY_CA_CERT` for `from_env`. Timeouts are set
on each `Endpoint` with `with_connect_timeout` and `with_request_timeout`, or by
`FIREFLY_CONNECT_TIMEOUT` and `FIREFLY_REQUEST_TIMEOUT` (whole seconds, at least
1; defaults 10 and 30) for `from_env`. `from_env` fails if a removed variable
(`FIREFLY_HOST`, `FIREFLY_GRPC_PORT`, `FIREFLY_HTTP_PORT`,
`FIREFLY_OBSERVER_HOST`, `FIREFLY_OBSERVER_GRPC_PORT`,
`FIREFLY_OBSERVER_HTTP_PORT`) is set, naming its replacement.

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
