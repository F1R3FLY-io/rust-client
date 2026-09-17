# Architecture

## F1r3flyApi vs F1r3flyConnectionManager

**F1r3flyApi** is the low-level client. It holds a signing key + host + port and makes single gRPC/HTTP calls. Each method is one network round-trip. No retry logic, no polling, no observer support.

**F1r3flyConnectionManager** is the high-level orchestrator. It wraps F1r3flyApi and adds:
- Observer node configuration for finalization checks
- Multi-phase deploy workflows (deploy -> block inclusion -> finalization -> data read)
- Retry/polling with configurable timeouts
- Transfer operations with address validation

Use `F1r3flyApi` directly when you need a single operation (exploratory deploy, one-shot deploy, propose). Use `F1r3flyConnectionManager` when you need a complete workflow that waits for results.

## Deploy Flow

### deploy_and_wait (4 phases)

```
1. Deploy         F1r3flyApi::deploy_with_phlo_limit_and_expiration() -> deploy_id
                  (deploy_and_wait maps its bigger_phlo bool to 50k/5B;
                   deploy_and_wait_with_phlo_limit takes the limit directly)
2. Finalization   wait_for_deploy_finalization() polls
                  GET /api/deploy-finalization-status/{sig} on the observer,
                  then the deploy node, until the state is terminal
                  (Finalized / Failed / Expired)
3. Data read      F1r3flyApi::get_data_at_deploy_id()  -> Vec<Par> (AFTER finalization)
4. Details        F1r3flyApi::get_deploy_detail()       -> cost, errored, blockNumber
```

There is no block-inclusion phase: the sig-level finalization status is the
only wait. The block-level fallback (`find_deploy_grpc` + `is_finalized`) was
removed, along with `deploy-and-wait --finalization-timeout`,
`ConnectionConfig.deploy_timeout_secs` and `FIREFLY_DEPLOY_TIMEOUT`.
`--max-wait` is now the whole finalization budget. A node that does not serve
`/api/deploy-finalization-status` errors with a message naming f1r3node-rust
v0.4.15 as the minimum.

The status is a canonical-state verdict, not a block-level one: a block can
finalize while some of its deploys' effects are dropped during merge, so the
node withholds a terminal verdict until the finalized floor passes the deploy's
contestability bound. A deploy that has already landed and failed can therefore
still read as `Pending` here (see rust-client#37).

Data is read AFTER finalization, not before. Reading before finalization can return empty results on shards because the block may not be replayed on the validator being queried.

## Node API Endpoints Used

### gRPC (DeployService on port 40401/40411/40421)

| Method | Used by | Notes |
|--------|---------|-------|
| `doDeploy` | deploy, deploy_internal | Submits deploy |
| `propose` | propose | Creates block |
| `exploratoryDeploy` | exploratory_deploy | Read-only execution |
| `getDataAtName` | get_data_at_deploy_id | Reads deploy result data (non-deprecated) |
| `findDeploy` | find_deploy_grpc | Finds block containing deploy |
| `isFinalized` | is_finalized | Checks block finalization |
| `showMainChain` | show_main_chain, tip sampling | Block queries |
| `getBlocksByHeights` | get_blocks_by_height | Range queries |

### HTTP (port 40403/40413/40423)

| Endpoint | Used by | Notes |
|----------|---------|-------|
| `GET /api/deploy/{id}` | get_deploy_detail, get_deploy_block_hash | Deploy execution details |
| `GET /api/deploy-finalization-status/{sig}` | deploy_finalization_status, wait_for_deploy_finalization | Canonical-state verdict; polled on the observer, then the deploy node |
| `POST /api/explore-deploy` | fetch_pos_snapshot, exploratory queries | Read-only node only |
| `GET /api/bond-status/{pk}` | bond-status | Bond state for a public key |
| `GET /api/status` | status, token metadata, tip sampling | Node identity and chain position |
| `GET /metrics` | network-health | Prometheus metrics |

### WebSocket (port 40403)

| Endpoint | Used by | Notes |
|----------|---------|-------|
| `/ws/events` | NodeEvents | Real-time block finalization events |

## Tip Sampling

The deploy flow needs the current block number to set `valid_after_block_number` (VABN). Stale chain tips cause "Block 50" errors where deploys are rejected as too old.

`get_current_block_number_monotonic()` addresses this:
1. Samples `show_main_chain(depth=8)` twice with 50ms delay
2. Takes the max block number across samples
3. Caches the result in `tip_floor` (AtomicI64) to prevent regression
4. If a new sample is lower than the cached floor, uses the floor

This prevents a scenario where a node temporarily returns a stale tip.

## Event Types (from f1r3fly-shared)

The `events` module uses `F1r3flyEvent` from the `f1r3fly-shared` crate, which is the same type the node uses to serialize WebSocket events. This ensures type-safe deserialization that stays in sync with the node.

The node sends events wrapped in an envelope:
```json
{"event": "block-finalised", "schema-version": 1, "payload": {"block-hash": "...", "deploys": [...]}}
```

The events module unwraps the envelope (merges payload fields into top-level) before deserializing as `F1r3flyEvent`, matching the approach used by the embers client.
