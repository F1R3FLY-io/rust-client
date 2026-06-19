# deploy-and-wait

Deploy Rholang code, wait for canonical-state finalization, then read the result.

This is the primary command for deploying contracts. It handles the full lifecycle:
1. Deploy code via gRPC
2. Poll the deploy signature via `/api/deploy-finalization-status` until terminal state (preferred path)
3. Read the `deployId` channel data from the finalized block (using `latest_block_hash` from the status response)
4. Get deploy execution details (cost, errored)

**Note:** Step 2 queries the observer first. On 404 (endpoint missing) or network error, it falls back to the deploy node's own HTTP endpoint. If both lack the endpoint (404 from both), the legacy path is used with a deprecation warning. Block-level polling can misreport success when a deploy is dropped during merge of a finalized block; sig-level polling is correct in that case.

## Usage

```bash
node_cli deploy-and-wait -f <FILE> [OPTIONS]
```

## Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--file` | `-f` | required | Rholang file to deploy |
| `--private-key` | `-k` | dev key | Signing key (64 hex chars) |
| `--host` | `-H` | `localhost` | Node hostname |
| `--port` | `-p` | `40412` | gRPC port |
| `--http-port` | | `40413` | HTTP port for deploy details |
| `--bigger-phlo` | | false | Use 5B phlo limit instead of 50K |
| `--propose` | | false | Also propose a block after deploy |
| `--max-wait` | | `60` | Max seconds to wait for block inclusion |
| `--finalization-timeout` | | `30` | Max seconds to wait for finalization |
| `--check-interval` | | `2` | Seconds between block inclusion polls |
| `--observer-host` | | same as host | Observer node for finalization checks |
| `--observer-port` | | `40452` | Observer gRPC port |
| `--expiration` | | none | Expiration timestamp (ms) |
| `--expires-in` | | none | Expiration duration (seconds) |

## Example: Contract that returns data

```
$ node_cli deploy-and-wait -f ./rho_examples/deploy_id_test.rho

Deploying and waiting for finalization...
Deploy ID:    3044022075e51b8f4a4b873344e276336c77ce9b91672bec10c8a327b6151142fd727b85022064eb25090cfd5135fdc316b77dbe0bd717ed5402bc30e8068bc9a5a0b4055e0d
Block hash:   0519f656624c26e8a406ed4fd7f1fa9327f48128a0ad7758289bc09b8f646419
Block number: 314
Cost:         316
Data[0]:      42
Total time:   22.97s
```

The contract `deployId!(42)` writes `42` to the deployId channel. The command reads it back after finalization.

## Example: Contract with no return data

```
$ node_cli deploy-and-wait -f ./rho_examples/stdout.rho

Deploying and waiting for finalization...
Deploy ID:    3045022100...
Block hash:   e60a8e78b1b30338...
Block number: 310
Cost:         62
Data:         (none)
Total time:   26.00s
```

The `stdout!("hello")` contract writes to stdout (visible in node logs) but nothing to `deployId`, so Data shows `(none)`.

## Example: With propose flag

```
$ node_cli deploy-and-wait -f contract.rho --propose

Deploying and waiting for finalization...
Deploy ID:    ...
Block hash:   ...
Total time:   15.32s
Block proposed: abc123...
```

## Timeouts

On the preferred (sig-level) path, `--max-wait` and `--finalization-timeout` are summed into a single budget for sig polling at 2s intervals. The deploy must reach a terminal state (`Finalized`, `Failed`, or `Expired`) within that combined budget.

- `Failed` → command exits with error indicating Rholang execution failure
- `Expired` → command exits with error indicating the deploy never landed canonically
- Timeout while still `Pending` → command exits with error

On the legacy (block-hash) fallback path, the two timeouts apply separately as before:

1. **Block inclusion** (`--max-wait`): polls `findDeploy` gRPC every `--check-interval` seconds until the deploy appears in a block. Default: 60s.
2. **Finalization** (`--finalization-timeout`): polls `isFinalized` gRPC every 5 seconds on the observer node until the block is finalized. Default: 30s.

## Observer Node

Finalization checks run against the observer node (read-only), not the validator. This avoids interfering with block production. Set `--observer-host` and `--observer-port` if the observer is on a different host.

On a standard Docker shard, the observer is at port 40452 (gRPC).
