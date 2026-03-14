# Node API Changelog (January–March 2026)

Covers changes on the `rust/dev` and Scala `dev` branches. Compatible with rust-client `dev` branch.

## New API Features

These are new additions to both the Scala and Rust nodes (mirrored protobuf definitions).

### TransferInfo on DeployInfo

`DeployInfo` now includes inline transfer data. The `get_block` and `last_finalized_block` endpoints return transfer details for each deploy without requiring a separate transaction lookup.

**New message:**
```
TransferInfo {
  fromAddr:   string  // Sender address
  toAddr:     string  // Recipient address
  amount:     int64   // Amount in dust (smallest unit)
  success:    bool    // Whether the transfer succeeded
  failReason: string  // Error message if success is false, empty otherwise
}
```

**New field on `DeployInfo`:**
- `transfers` — `repeated TransferInfo` (proto field 13)

Transfers are populated via background enrichment on block finalization and cached for subsequent lookups.

### PeerInfo on Status

The `/status` endpoint now returns detailed peer information.

**New message:**
```
PeerInfo {
  address:       string  // Full rnode:// URL
  nodeId:        string  // Public key hash (hex)
  host:          string  // Hostname or IP
  protocolPort:  int32   // TCP port
  discoveryPort: int32   // UDP port
  isConnected:   bool    // True if active connection
}
```

**New field on `Status`:**
- `peerList` — `repeated PeerInfo` (proto field 8)

### Deploy Expiration

Deploys can now specify an expiration timestamp. After this time, the deploy is rejected and blocks containing expired deploys are slashable.

**New field on `DeployDataProto`:**
- `expirationTimestamp` — `int64` (proto field 13), millisecond Unix timestamp. `0` means no expiration. Backward-compatible (proto3 omits zero values on the wire).

### Simplified Explore Deploy

The `/explore-deploy` endpoint now accepts a plain string body (the Rholang term) instead of requiring a full structured request object. The structured format with `block_hash` and `use_pre_state_hash` is still available at `/explore-deploy-by-block-hash`.

### Produce Event Failure Flag

**New field on `ProduceEventProto`:**
- `failed` — `bool` (proto field 7), indicates a failed non-deterministic process call (e.g., external gRPC, OpenAI).

## Behavioral Changes

### BlockAPI Depth Clamping

`showBlocks`, `showMainChain`, `getBlocksByHeights`, and related depth-parameterized endpoints now silently clamp the depth to the configured maximum instead of returning an error when the requested depth exceeds the limit.

### find_deploy Polling (Rust node only)

The Scala node does a single-shot DAG lookup for `find_deploy`. The Rust node previously had a hidden 8-second polling loop (80 attempts x 100ms). This is now configurable and defaults to a single attempt:

| Environment Variable | Default | Description |
|---|---|---|
| `F1R3_FIND_DEPLOY_RETRY_INTERVAL_MS` | `50` | Interval between retry attempts (ms) |
| `F1R3_FIND_DEPLOY_MAX_ATTEMPTS` | `1` | Maximum number of lookup attempts |

## Rust Node Alignment with Scala

These changes bring the Rust node's JSON API output in line with the existing Scala node behavior. They are **not new** for clients already using the Scala node, but are breaking changes for clients that were previously hitting the Rust node.

### JSON Field Naming: snake_case to camelCase

All REST API JSON responses now use camelCase keys, matching Scala's convention:

| Previous (Rust) | Current (both nodes) |
|---|---|
| `block_hash` | `blockHash` |
| `seq_num` | `seqNum` |
| `sig_algorithm` | `sigAlgorithm` |
| `shard_id` | `shardId` |
| `extra_bytes` | `extraBytes` |
| `header_extra_bytes` | `headerExtraBytes` |
| `body_extra_bytes` | `bodyExtraBytes` |
| `parents_hash_list` | `parentsHashList` |
| `block_number` | `blockNumber` |
| `pre_state_hash` | `preStateHash` |
| `post_state_hash` | `postStateHash` |
| `block_size` | `blockSize` |
| `deploy_count` | `deployCount` |
| `fault_tolerance` | `faultTolerance` |
| `rejected_deploys` | `rejectedDeploys` |
| `latest_block_hash` | `latestBlockHash` |
| `phlo_price` | `phloPrice` |
| `phlo_limit` | `phloLimit` |
| `valid_after_block_number` | `validAfterBlockNumber` |
| `system_deploy_error` | `systemDeployError` |

### Bytes Fields: Integer Array to Base64 String

Protobuf `bytes` fields are now serialized as base64 strings in JSON, matching Scala's output. Previously the Rust node emitted integer arrays.

| Field | Previous (Rust) | Current (both nodes) |
|---|---|---|
| `extraBytes` | `[1, 2, 3]` | `"AQID"` |
| `headerExtraBytes` | `[4, 5, 6]` | `"BAUG"` |
| `bodyExtraBytes` | `[7, 8, 9]` | `"BwgJ"` |
| `postStateHash` | `[...]` | `"..."` (base64) |
| `invalidBlockHash` | `[...]` | `"..."` (base64) |
| `issuerPublicKey` | `[...]` | `"..."` (base64) |

### RhoExpr/RhoUnforg Enum Serialization

Switched from internal tagging to external tagging (wrapper-key format), matching Scala's circe derivation:

| Previous (Rust) | Current (both nodes) |
|---|---|
| `{"type": "par", "data": [...]}` | `{"ExprPar": {"data": [...]}}` |

### PrepareRequest/PrepareResponse Types

| Field | Previous (Rust) | Current (both nodes) |
|---|---|---|
| `PrepareRequest.deployer` | byte array | hex string |
| `PrepareResponse.names` | byte array[] | hex string[] |

### WebSocket Event Field

| Previous (Rust) | Current (both nodes) |
|---|---|
| `seq_number` or `seqNum` | `seq-num` (kebab-case) |

### language Field

The `language` field (proto field 12 on `DeployDataProto`) is no longer serialized or deserialized. It remains in the protobuf schema but is effectively ignored, matching Scala behavior.

## rust-client Changes (dev branch)

### Breaking: Method Signatures

| Method | Change |
|---|---|
| `deploy()` | New parameter: `expiration_timestamp: i64` |
| `full_deploy()` | New parameter: `expiration_timestamp: i64`, return type changed `String` to `ProposeResult` |
| `propose()` | Return type changed `String` to `ProposeResult` |
| `build_deploy_msg()` | Two new parameters: `expiration_timestamp: i64`, `timestamp_override: Option<i64>` |

`ProposeResult` is a new enum: `Proposed(String)` or `Skipped(String)`.

### RevAddress to VaultAddress Rename

All references to `RevAddress` have been renamed to `VaultAddress` throughout the codebase, matching the Scala node which has no `RevAddress`.

| Previous | Current |
|---|---|
| `RevAddress` | `VaultAddress` |
| `generate_rev_address` | `generate_vault_address` |
| CLI: `generate-rev-address` | CLI: `generate-vault-address` |

### Rholang URI Renames

System contract URIs have been updated to match the Scala node:

| Previous | Current |
|---|---|
| `rho:rchain:pos` | `rho:system:pos` |
| `rho:rchain:revVault` | `rho:vault:system` |
| `rho:rchain:deployerId` | `rho:system:deployerId` |

The Rust node registers legacy `rho:rchain:*` aliases for backward compatibility, but new code should use the `rho:system:*` / `rho:vault:*` URIs.

### Deploy Expiration CLI Arguments

All deploy-related commands (`deploy`, `deploy-and-wait`, `transfer`, `bond-validator`) now support:

- `--expiration <epoch_millis>` — absolute expiration timestamp
- `--expires-in <seconds>` — relative expiration from now

These are mutually exclusive.

### Library Crate Modules

The rust-client is now usable as a library dependency with feature-gated CLI. New public modules:

| Module | Purpose |
|---|---|
| `connection_manager` | Connection pooling with env-var config |
| `http_client` | HTTP-based alternative to gRPC |
| `signing` | Shared deploy signing logic |
| `vault` | Transfer operations, balance queries, address validation |
| `registry` | Registry URI generation, `insertSigned` signing |
| `rholang_helpers` | Convert Rholang expressions to plain JSON |
