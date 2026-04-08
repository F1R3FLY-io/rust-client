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

Reading Rholang from: ./rho_examples/stdout.rho
Code size: 62 bytes
Connecting to F1r3fly node at localhost:40452
Executing Rholang code (exploratory deploy)...
Using post-state hash
Execution successful!
Time taken: 80.43ms
Block hash: 3ef7b8d5..., Block number: 364
Result:
No data returned
```

## Notes

- Must run against a read-only or observer node (validators may reject exploratory deploys)
- On a standard Docker shard, the observer gRPC port is 40452
- `--block-hash` lets you query historical state at any finalized block
- `--use-pre-state` queries the state BEFORE the block's deploys executed (useful for debugging)
