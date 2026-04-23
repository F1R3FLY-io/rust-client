# Testing

## Integration Tests (`tests/smoke.rs`)

Rust integration tests that verify all HTTP API endpoints against a running shard. These test the API responses directly using `reqwest`, with structured assertions on JSON field types, values, and relationships.

### Running

```bash
# Requires a running shard (docker compose or standalone)
cargo test --test smoke --release

# With custom ports
F1R3FLY_HTTP_PORT=40403 F1R3FLY_OBSERVER_HTTP=40453 cargo test --test smoke --release

# Single test
cargo test --test smoke test_status_fields --release
```

Tests skip gracefully if no shard is reachable — `cargo test` always succeeds, tests just return `Ok(())`.

### Environment Variables

| Variable | Default | Description |
|---|---|---|
| `F1R3FLY_HOST` | `localhost` | Node hostname |
| `F1R3FLY_HTTP_PORT` | `40413` | Validator HTTP port |
| `F1R3FLY_OBSERVER_HTTP` | `40453` | Readonly HTTP port |

### Test Coverage (24 tests)

| Category | Tests | What's verified |
|---|---|---|
| Status | 1 | All 17 fields, types, `isReady=true`, `epochLength>0` |
| Blocks (single) | 3 | Full/summary views, `isFinalized`, hash match |
| Blocks (list) | 3 | Summary default (no deploys), full view, height range |
| is-finalized | 1 | Returns `true` for LFB |
| prepare-deploy | 1 | `seqNumber`, `names` present |
| explore-deploy | 1 | `cost>0`, `expr`, `block` |
| Epoch | 2 | All fields, derived `currentEpoch` check, `?block_hash=` param |
| Validators | 2 | Bonded (real pubkey), unknown (fake pubkey) |
| Bond status | 2 | Bonded on validator node, unknown returns false |
| Epoch rewards | 1 | ExprMap structure |
| Estimate cost | 2 | Valid term returns cost, invalid syntax returns error |
| Removed endpoints | 2 | `/data-at-name` and `/transactions` return 404 |
| Edge cases | 1 | Unknown `?view=` falls back to full |

### What's NOT covered

- `POST /deploy` — requires deploy signing
- `GET /deploy/{id}` — requires prior deploy
- `GET /balance/{address}` — requires REV address with vault
- `GET /registry/{uri}` — requires deployed contract
- WebSocket events — requires async WS client
- gRPC endpoints — covered by system-integration integration tests

## Smoke Test Script (`scripts/smoke_test.sh`)

Bash script that tests the **CLI binary** end-to-end. Builds the binary, runs each command, and validates output against regex patterns.

```bash
# Against a shard (validator1 on 40412/40413, readonly on 40452/40453)
./scripts/smoke_test.sh localhost 40412 40413 40452

# Against standalone node
./scripts/smoke_test.sh localhost 40402 40403
```

### What it tests

The smoke test covers all CLI commands including deploy, propose, transfer, and load testing — operations that require signing and multi-step workflows that the Rust integration tests don't cover.

It also tests the new HTTP endpoints directly via `curl`:
- `/api/epoch`, `/api/validators`, `/api/bond-status`, `/api/estimate-cost`
- `/api/deploy/{id}?view=summary` (summary view)
- Removed endpoints return 404

### Limitations

- Output validation is regex-based (fragile when display format changes)
- Sequential execution (~5 min for full suite)
- Some tests depend on earlier test results (deploy ID cascading)
- Transfer test can fail under load (finalization timeout)

### Complementary testing

| Concern | `tests/smoke.rs` | `scripts/smoke_test.sh` |
|---|---|---|
| API response structure | Yes (typed JSON) | No (regex on CLI output) |
| CLI argument parsing | No | Yes |
| Deploy signing flow | No | Yes |
| Transfer end-to-end | No | Yes |
| Load testing | No | Yes |
| WebSocket display | No | Yes (10s capture) |
| Speed | 0.3s | ~5 min |
