# F1r3fly Node CLI

Rust crate for interacting with F1r3fly blockchain nodes. Usable as a **library** or **CLI tool**.

## Quick Start

```bash
# Build
cargo build --release

# Check node status and native token info
cargo run --release -- status -H localhost -p 40413

# Deploy and wait for result
cargo run --release -- deploy-and-wait -f contract.rho

# Read-only query
cargo run --release -- exploratory-deploy -f query.rho -H localhost -p 40452

# Query native token metadata from on-chain contract
cargo run --release -- exploratory-deploy -f rho_examples/query_token_metadata.rho -H localhost -p 40452
```

## Documentation

### Commands
- [deploy](docs/commands/deploy.md) -- submit Rholang code to the blockchain
- [deploy-and-wait](docs/commands/deploy-and-wait.md) -- deploy, wait for finalization, read result
- [exploratory-deploy](docs/commands/exploratory-deploy.md) -- read-only Rholang execution
- [estimate-cost](docs/commands/estimate-cost.md) -- estimate phlogiston cost before deploying
- [get-deploy](docs/commands/get-deploy.md) -- get deploy execution details
- [get-data](docs/commands/get-data.md) -- read deploy result data
- [propose](docs/commands/propose.md) -- manually propose a block
- [is-finalized](docs/commands/is-finalized.md) -- check block finalization
- [transfer](docs/commands/transfer.md) -- transfer native tokens
- [Node inspection](docs/commands/inspection.md) -- status, blocks, bonds, balance, etc.
- [Key management](docs/commands/keys.md) -- generate keys, addresses
- [Advanced](docs/commands/advanced.md) -- load-test, watch-blocks, dag, bond-validator

### Library
- [Getting started](docs/library/getting-started.md) -- ConnectionManager API, config, examples
- [Events](docs/library/events.md) -- WebSocket deploy finalization
- [Architecture](docs/architecture.md) -- module structure, deploy flow, node endpoints

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `FIREFLY_PRIVATE_KEY` | Yes | -- | Signing key (64 hex chars) |
| `FIREFLY_HOST` | No | `localhost` | Node hostname |
| `FIREFLY_GRPC_PORT` | No | `40401` | gRPC port |
| `FIREFLY_HTTP_PORT` | No | `40403` | HTTP port |
| `FIREFLY_OBSERVER_HOST` | No | same as host | Observer for finalization |
| `FIREFLY_OBSERVER_GRPC_PORT` | No | `40452` | Observer gRPC port |
| `FIREFLY_DEPLOY_TIMEOUT` | No | `60` | Max seconds for block inclusion |
| `FIREFLY_FINALIZATION_TIMEOUT` | No | `30` | Max seconds for finalization |

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `f1r3fly-models` | git (rust/dev) | Protobuf types |
| `f1r3fly-shared` | git (rust/dev) | Event types |
| `secp256k1` | 0.31 | ECDSA signing |
| `reqwest` | 0.12 | HTTP client |
| `tonic` | 0.14 | gRPC client |
| `tokio-tungstenite` | 0.26 | WebSocket |
| `tracing` | 0.1 | Structured logging |
| `thiserror` | 2.0 | Error derives |
