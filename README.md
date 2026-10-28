# F1r3fly Node CLI

Rust crate for interacting with F1r3fly blockchain nodes. Usable as a **library** or **CLI tool**.

## Quick Start

Node endpoints are URLs: `http://` is plaintext, `https://` is TLS (add `--ca-cert <PEM>` to trust a private CA). Every command defaults to a node's own ports, gRPC `http://localhost:40401` and HTTP `http://localhost:40403`; the examples below target the system-integration single-host Docker shard.

```bash
# Build
cargo build --release

# Check node status and native token info
cargo run --release -- status --http http://localhost:40413

# Deploy to validator1 and wait for the result, also polling the read-only observer
cargo run --release -- deploy-and-wait -f contract.rho \
  --grpc http://localhost:40412 --http http://localhost:40413 \
  --observer http://localhost:40453

# Read-only query
cargo run --release -- exploratory-deploy -f query.rho --grpc http://localhost:40452

# Query native token metadata from on-chain contract
cargo run --release -- exploratory-deploy -f rho_examples/query_token_metadata.rho --grpc http://localhost:40452
```

## Local development

Commands need a signing key, provided via `--private-key` or the `FIREFLY_PRIVATE_KEY` environment variable; if neither is set, the command exits with an error.

```bash
export FIREFLY_PRIVATE_KEY=<your key>
```

Local dev/test Docker setups fund a well-known development key, which ships in
those Docker configs — take it from the node setup you are running against
(`.env.node` in system-integration, `docker/.env` in f1r3node-rust) rather than
from here. Never reuse a key from a README against anything real.

## Documentation

### Commands
- [deploy](docs/commands/deploy.md) -- submit Rholang code to the blockchain
- [deploy-and-wait](docs/commands/deploy-and-wait.md) -- deploy, wait for finalization, read result
- [exploratory-deploy](docs/commands/exploratory-deploy.md) -- read-only Rholang execution
- [estimate-cost](docs/commands/estimate-cost.md) -- estimate phlogiston cost before deploying
- [get-deploy](docs/commands/get-deploy.md) -- get deploy execution details
- [deploy-status](docs/commands/deploy-status.md) -- canonical finalization status of a deploy signature
- [get-data](docs/commands/get-data.md) -- read deploy result data
- [propose](docs/commands/propose.md) -- manually propose a block
- [is-finalized](docs/commands/is-finalized.md) -- check block finalization
- [transfer](docs/commands/transfer.md) -- transfer native tokens
- [Node inspection](docs/commands/inspection.md) -- status, blocks, bonds, balance, etc.
- [Key management](docs/commands/keys.md) -- generate keys, addresses
- [Advanced](docs/commands/advanced.md) -- load-test, watch-events, dag, bond-validator, unbond-validator, PoS queries

### Guides
- [Bonding a validator](docs/guides/bonding-a-validator.md) -- fund, bond, activate, unbond, payout

### Library
- [Getting started](docs/library/getting-started.md) -- ConnectionManager API, config, examples
- [Events](docs/library/events.md) -- WebSocket deploy finalization
- [Architecture](docs/architecture.md) -- module structure, deploy flow, node endpoints

### Testing
- [Testing guide](docs/testing.md) -- unit tests, and the integration suite that runs against a shard it creates

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `FIREFLY_PRIVATE_KEY` | Yes | -- | Signing key (64 hex chars) |
| `FIREFLY_GRPC_URL` | No | `http://localhost:40401` | Deploy node's gRPC endpoint |
| `FIREFLY_HTTP_URL` | No | `http://localhost:40403` | Deploy node's HTTP endpoint |
| `FIREFLY_OBSERVER_URL` | No | none | HTTP endpoint of a second node polled for the deploy's verdict; without it, only the deploy node is polled |
| `FIREFLY_CA_CERT` | No | none | PEM file of the certificate authorities trusted for every `https://` endpoint, in place of the bundled public roots |
| `FIREFLY_FINALIZATION_TIMEOUT` | No | `90` | Max seconds to wait for a deploy to finalize |
| `FIREFLY_SHARD_ID` | No | `root` | Shard deploys are signed for: the node's `casper.shard-name` |

These are read by the library's `ConnectionConfig::from_env`. The CLI reads only `FIREFLY_PRIVATE_KEY` and `FIREFLY_SHARD_ID`; its endpoints come from `--grpc`, `--http`, `--observer` and `--ca-cert`.

An unset variable takes its default; a set but malformed one, such as a URL without an `http` or `https` scheme, is an error.
