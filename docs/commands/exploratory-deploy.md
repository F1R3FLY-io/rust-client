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

## Notes

- Must run against a read-only or observer node (validators may reject exploratory deploys)
- On a standard Docker shard, the observer gRPC port is 40452
- `--block-hash` lets you query historical state at any finalized block
- `--use-pre-state` queries the state BEFORE the block's deploys executed (useful for debugging)
