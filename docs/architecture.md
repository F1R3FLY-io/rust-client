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

### deploy_and_wait (5 phases)

```
1. Deploy         F1r3flyApi::deploy()                -> deploy_id
2. Block wait     F1r3flyApi::find_deploy_grpc()      polls until deploy in block -> block_hash
3. Finalization   F1r3flyApi::is_finalized()           polls observer until finalized
4. Data read      F1r3flyApi::get_data_at_deploy_id()  -> Vec<Par> (AFTER finalization)
5. Details        F1r3flyApi::get_deploy_detail()       -> cost, errored, blockNumber
```

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
