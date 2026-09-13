# deploy-and-wait

Deploy Rholang code, wait for canonical-state finalization, then read the result.

This is the primary command for deploying contracts. It handles the full lifecycle:
1. Deploy code via gRPC
2. Poll the deploy signature via `/api/deploy-finalization-status` until terminal state
3. Read the `deployId` channel data from the finalized block (using `latest_block_hash` from the status response)
4. Get deploy execution details (cost, errored)

**Note:** Step 2 queries the observer first, and the deploy node's own HTTP endpoint when the observer does not answer. The status reports whether the deploy's effects are in canonical state, so a deploy dropped during merge of a finalized block is not reported as finalized. The command requires a node that serves the endpoint (f1r3node-rust v0.4.15 or later) and fails immediately otherwise.

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
| `--max-wait` | | `300` | Max seconds to wait for finalization |
| `--check-interval` | | `2` | Seconds between finalization status polls |
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

The deploy must reach a terminal state (`Finalized`, `Failed`, or `Expired`) within `--max-wait` seconds. The status is polled every `--check-interval` seconds.

- `Failed` → command exits with error indicating Rholang execution failure
- `Expired` → command exits with error indicating the deploy never landed canonically
- Timeout while still `Pending` → command exits with error

## Observer Node

Finalization checks run against the observer node (read-only), not the validator. This avoids interfering with block production. Set `--observer-host` and `--observer-port` if the observer is on a different host.

On a standard Docker shard, the observer is at port 40452 (gRPC).
