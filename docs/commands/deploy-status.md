# deploy-status

Check canonical-state finalization status of a deploy by signature.

Block-hash polling is insufficient for tracking deploy finalization: a block can finalize while some of its deploys' effects are dropped during merge. `deploy-status` polls the `/api/deploy-finalization-status/{sig}` endpoint and reports whether the deploy's effects are in canonical state.

## Usage

```bash
node_cli deploy-status --sig <HEX> [OPTIONS]
```

## Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--sig` | `-s` | required | Deploy signature in hex (with or without `0x` prefix) |
| `--host` | `-H` | `localhost` | Node hostname |
| `--http-port` | | `40413` | HTTP port (use the readonly observer port for shards) |
| `--format` | `-f` | `pretty` | Output format: `pretty`, `json` |

## Example

```
$ node_cli deploy-status -s 304502210085f163...

Deploy Finalization Status
----------------------------------------
Sig:              304502210085f163934f0de4c8eadb177d62b9527997100ff3b80d868dbfa702c2...
State:            Finalized
Rejection count:  0
Latest block:     79d3560b36998644d139ba0f73a3883274f28f22b6f2016e973f3606be38bb56
Query time:       12.4ms
```

## States

| State | Terminal | Meaning |
|-------|----------|---------|
| `Finalized` | yes | Clean inclusion in canonical-finalized block; effects are in canonical state |
| `Failed` | yes | Rholang execution itself failed (insufficient phlo, contract error, etc.); effects will never apply |
| `Pending` | no | Not yet canonical-finalized and not expired; keep polling |
| `Expired` | yes | `valid_after_block_number + deploy_lifespan` elapsed without canonical inclusion |

## Response fields

| Field | Description |
|-------|-------------|
| `state` | One of the four states above |
| `rejection_count` | Number of finalized blocks where the sig appears in `body.rejected_deploys`; non-zero means the deploy is contending |
| `latest_block_hash` | Highest-block-number canonical block containing the sig (clean or rejected); `null` until first inclusion |

## Polling guidance

Block production is typically 5–30s. Poll every 2–5s until a terminal state is observed. Unknown sigs (just submitted, or never deployed) return `Pending` with `latest_block_hash: null` — keep polling rather than treating this as an error.

## See also

- [`deploy-and-wait`](deploy-and-wait.md) — uses this endpoint internally for sig-level finalization detection
- [`get-deploy`](get-deploy.md) — historical lookup for already-finalized deploys (returns full execution detail)
- [`is-finalized`](is-finalized.md) — block-level finalization check
