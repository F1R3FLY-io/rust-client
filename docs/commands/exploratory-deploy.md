# exploratory-deploy

Execute Rholang code without committing to the blockchain. Read-only.

The code runs against the last finalized block's state (or a specified block). Nothing is persisted. Useful for querying contract state, checking balances, or testing code.

## Usage

```bash
node_cli exploratory-deploy -f <FILE> [OPTIONS]
```

## Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--file` | `-f` | required | Rholang file to execute |
| `--private-key` | `-k` | dev key | Signing key |
| `--host` | `-H` | `localhost` | Node hostname |
| `--port` | `-p` | `40412` | gRPC port |
| `--block-hash` | | latest | Execute against a specific block's state |
| `--use-pre-state` | | false | Use pre-state hash instead of post-state |

## Example

```
$ node_cli exploratory-deploy -f ./rho_examples/stdout.rho -H localhost -p 40452

Execution successful!
Cost:    317 phlogiston
Time:    70.37ms
Block hash: 0ffe5c93..., Block number: 48
Result:
No data returned
```

The response now includes the phlogiston cost of execution. For cost-only output, use [estimate-cost](estimate-cost.md).

## Querying native token metadata

The node's `TokenMetadata` contract (registered at `rho:system:tokenMetadata`) can be queried via exploratory deploy. An example Rholang file is included at `rho_examples/query_token_metadata.rho`:

```
$ node_cli exploratory-deploy -H localhost -p 40452 -f rho_examples/query_token_metadata.rho

Execution successful!
Cost:    34218 phlogiston
Time:    107.11ms
Block hash: 579fbb0b..., Block number: 48
Result:
("F1R3CAP", "F1R3", 8)
```

The contract supports four methods:
- `TokenMetadata!("name", *ret)` — returns the full token name as a string
- `TokenMetadata!("symbol", *ret)` — returns the ticker symbol as a string
- `TokenMetadata!("decimals", *ret)` — returns the decimal places as an integer
- `TokenMetadata!("all", *ret)` — returns a `(name, symbol, decimals)` tuple

These values are baked into genesis and are immutable for the lifetime of the network.

## Notes

- Must run against a read-only or observer node (validators may reject exploratory deploys)
- On a standard Docker shard, the observer gRPC port is 40452
- `--block-hash` lets you query historical state at any finalized block
- `--use-pre-state` queries the state BEFORE the block's deploys executed (useful for debugging)
