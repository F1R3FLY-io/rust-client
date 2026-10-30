# Architecture

## F1r3flyApi vs F1r3flyConnectionManager

**F1r3flyApi** is the low-level gRPC client. It holds an `Endpoint` and, optionally, a signing key, and makes single gRPC calls. Each method is one network round-trip. No retry logic, no polling, no observer support. A node's HTTP API has its own client, **NodeHttp**.

**F1r3flyConnectionManager** is the high-level orchestrator. It builds an F1r3flyApi and a NodeHttp once, reuses their connections, and adds:
- An optional observer node polled for finalization verdicts
- Multi-phase deploy workflows (deploy -> block inclusion -> finalization -> data read)
- Retry/polling with configurable timeouts
- Transfer operations with address validation

Every endpoint is a URL: `http://` is plaintext, `https://` is TLS, trusting the bundled public roots or the CA bundle set with `Endpoint::with_ca_certificates`, which refuses an `http://` endpoint. gRPC, HTTP and WebSocket all honor it, along with the endpoint's connect timeout (default 10 s) and request timeout (default 30 s); an open WebSocket stream is bounded only by the connect timeout. An HTTP endpoint may carry a base path, under which requests and `/ws/events` go; a gRPC endpoint takes none, and `F1r3flyApi::new` refuses one.

Use `F1r3flyApi` directly when you need a single operation (exploratory deploy, one-shot deploy, propose). Use `F1r3flyConnectionManager` when you need a complete workflow that waits for results.

## Deploy Flow

### deploy_and_wait (4 phases)

```
1. Deploy         F1r3flyApi::deploy(term, &DeployOptions) -> deploy_id
                  = prepare (anchors the validity window at the tip)
                  + UnsignedDeploy::sign + submit; submit_and_wait starts
                  here with a deploy signed earlier
2. Finalization   wait_for_deploy_finalization() polls
                  GET /api/deploy-finalization-status/{sig} on the deploy node
                  and, when one is configured, the observer together, until
                  either reports a terminal state (Finalized / Failed / Expired)
3. Data read      F1r3flyApi::get_data_at_deploy_id()  -> Vec<Par> (AFTER finalization)
4. Details        NodeHttp::get_deploy_detail()         -> cost, errored, blockNumber
```

There is no block-inclusion phase: the sig-level finalization status is the
only wait. The block-level fallback (`find_deploy_grpc` + `is_finalized`) was
removed, along with `deploy-and-wait --finalization-timeout`,
`ConnectionConfig.deploy_timeout_secs` and `FIREFLY_DEPLOY_TIMEOUT`.
`--max-wait` is now the whole finalization budget. A deploy node that does not
serve `/api/deploy-finalization-status`, with no answer from the observer,
errors immediately with a message naming f1r3node-rust v0.4.15 as the minimum,
provided the same endpoint answers `/api/status`; if it does not, the error is
that endpoint's HTTP error (a mistyped base path, for instance).

The status is a canonical-state verdict, not a block-level one: a block can
finalize while some of its deploys' effects are dropped during merge, so the
node withholds a terminal verdict until the finalized floor passes the deploy's
contestability bound. Each node writes its verdict once and never changes it,
so the first terminal state either node reports ends the wait; two nodes
reporting different terminal states is an error. `/api/deploy/{id}` serves
the same verdict as `finalizationState`; the wait reads it only for the
execution error it reports.

Data is read AFTER finalization, not before. Reading before finalization can return empty results on shards because the block may not be replayed on the validator being queried.

## Node API Endpoints Used

### gRPC (external API, port 40401 on a node; `propose` is on the internal API, 40402)

| Method | Used by | Notes |
|--------|---------|-------|
| `doDeploy` | deploy, deploy_internal | Submits deploy |
| `propose` | propose | Creates block |
| `exploratoryDeploy` | exploratory_deploy | Read-only execution |
| `getDataAtName` | get_data_at_deploy_id | Reads deploy result data (non-deprecated) |
| `isFinalized` | is_finalized | Checks block finalization |
| `showMainChain` | show_main_chain, tip sampling | Block queries |
| `getBlocksByHeights` | get_blocks_by_height | Range queries |

### HTTP (port 40403 on a node)

| Endpoint | Used by | Notes |
|----------|---------|-------|
| `GET /api/deploy/{id}` | get_deploy_detail, get_deploy_block_hash | Deploy execution details |
| `GET /api/deploy-finalization-status/{sig}` | deploy_finalization_status, wait_for_deploy_finalization | Canonical-state verdict; polled on the deploy node and the observer, if any, together |
| `POST /api/explore-deploy` | NodeHttp::explore_deploy (fetch_pos_snapshot, epoch-rewards) | Read-only node only |
| `GET /api/balance/{address}` | NodeHttp::balance (wallet-balance, load-test) | Read-only node only |
| `GET /api/bond-status/{pk}` | bond-status | Bond state for a public key |
| `GET /api/status` | status, token metadata, tip sampling | Node identity and chain position |
| `GET /metrics` | network-health | Prometheus metrics |

### WebSocket (the HTTP endpoint; `wss` for `https`)

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
