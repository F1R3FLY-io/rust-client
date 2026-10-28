# deploy-and-wait

Deploy Rholang code, wait for canonical-state finalization, then read the result.

This is the primary command for deploying contracts. It handles the full lifecycle:
1. Deploy code via gRPC
2. Poll the deploy signature via `/api/deploy-finalization-status` until terminal state
3. Read the `deployId` channel data from the finalized block (using `latest_block_hash` from the status response)
4. Get deploy execution details (cost, errored) from the node whose status was the verdict

**Note:** Step 2 queries the deploy node, and the observer when `--observer` is given, on every poll, and stops at the first terminal state either reports; a node never changes a verdict once written. The status reports whether the deploy's effects are in canonical state, so a deploy dropped during merge of a finalized block is not reported as finalized. The command requires a deploy node that serves the endpoint (f1r3node-rust v0.4.15 or later) and fails immediately otherwise, unless the observer answers.

## Usage

```bash
node_cli deploy-and-wait -f <FILE> [OPTIONS]
```

## Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--file` | `-f` | required | Rholang file to deploy |
| `--private-key` | `-k` | dev key | Signing key (64 hex chars) |
| `--grpc` | | `http://localhost:40401` | Deploy node's gRPC endpoint |
| `--http` | | `http://localhost:40403` | Deploy node's HTTP endpoint, polled for finalization status |
| `--ca-cert` | | none | PEM file of the certificate authorities trusted for `https://` endpoints, in place of the bundled public roots |
| `--phlo-limit` | | `50000` | Most phlo the deploy may spend |
| `--phlo-price` | | `1` | Price per phlo, in the native token's base units |
| `--shard-id` | | `root` | Shard the deploy is signed for: the node's `casper.shard-name` (env `FIREFLY_SHARD_ID`) |
| `--propose` | | false | Also propose a block after deploy; `--grpc` must then be the node's internal API |
| `--max-wait` | | `300` | Max seconds to wait for finalization |
| `--check-interval` | | `2` | Seconds between finalization status polls |
| `--observer` | | none | HTTP endpoint of a second node, such as the shard's read-only node, polled alongside the deploy node for the verdict |
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

When the deploy finalizes but its `deployId` data or execution details cannot be read, the line shows `unreadable (<error>)` and the command exits non-zero, so a failed read is never mistaken for a contract that wrote nothing or ran cleanly.

## Example: With propose flag

```
$ node_cli deploy-and-wait -f contract.rho --propose --grpc http://localhost:40402

Deploying and waiting for finalization...
Deploy ID:    ...
Block hash:   ...
Total time:   15.32s
Block proposed: abc123...
```

## Timeouts

The deploy must reach a terminal state (`Finalized`, `Failed`, or `Expired`) within `--max-wait` seconds. The status is polled every `--check-interval` seconds.

- `Failed` → command exits with an error naming the block, cost and execution error
- `Expired` → command exits with error indicating the deploy never landed canonically
- Timeout while still `Pending` → command exits with error; if a node that saw the deploy in a block reports a failed execution, the error names it

## Observer Node

With `--observer`, finalization checks run against both the deploy node and that node. Without it, only the deploy node is polled.

On the system-integration single-host Docker shard, deploy to validator1 and observe from the read-only node:

```bash
node_cli deploy-and-wait -f contract.rho \
  --grpc http://localhost:40412 --http http://localhost:40413 \
  --observer http://localhost:40453
```

## TLS

An `https://` endpoint connects over TLS, trusting the bundled public roots, or only the certificate authorities in `--ca-cert` when given. `http://` endpoints ignore `--ca-cert`.
