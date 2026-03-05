# F1R3FLY Testing Tools

## Smoke Test

Comprehensive test suite that validates all rust-client commands against a running F1r3fly node.

### Usage

```bash
# Basic usage (localhost with default ports)
./scripts/smoke_test.sh

# Custom host/ports
./scripts/smoke_test.sh localhost 40402 40403

# Run against a shard with separate observer node
./scripts/smoke_test.sh localhost 40412 40413 40452

# Run with custom private key
./scripts/smoke_test.sh localhost 40402 40403 40402 YOUR_PRIVATE_KEY
```

### Arguments

```bash
./scripts/smoke_test.sh [host] [grpc_port] [http_port] [observer_grpc_port] [private_key]
```

| Argument | Description | Default |
|----------|-------------|---------|
| `host` | Node hostname | localhost |
| `grpc_port` | gRPC port for deploy/propose operations | 40402 |
| `http_port` | HTTP port for status/query operations | 40403 |
| `observer_grpc_port` | Observer gRPC port for finalization checks | same as grpc_port |
| `private_key` | Private key for signing transactions | genesis key |

### What's Tested

The smoke test validates 30+ commands across these categories:

| Category | Commands |
|----------|----------|
| **Deploy** | deploy, deploy-and-wait, is-finalized, exploratory-deploy |
| **Crypto** | generate-public-key, generate-key-pair, generate-rev-address, get-node-id |
| **Node Inspection** | status, blocks, bonds, active-validators, metrics, last-finalized-block |
| **gRPC Queries** | show-main-chain, get-blocks-by-height, wallet-balance |
| **Network** | network-health, bond-status |
| **Transfer** | transfer, get-deploy |
| **PoS Queries** | epoch-info, epoch-rewards, validator-status, network-consensus |
| **Streaming** | watch-blocks (verifies Block Created, Added, and Finalized events) |
| **Load Testing** | load-test (runs 3 transfers, requires 100% finalization) |

---

## load-test Command

Native Rust command for testing orphan rates by sending multiple transfers with detailed logging.

### Usage

```bash
# Basic usage (20 tests, 1 REV, 10s interval)
cargo run -- load-test --to-address "111129p33f7vaRrpLqK8Nr35Y2aacAjrR5pd6PCzqcdrMuPHzymczH"

# Custom configuration
cargo run -- load-test \
  --to-address "111129p33f7vaRrpLqK8Nr35Y2aacAjrR5pd6PCzqcdrMuPHzymczH" \
  --num-tests 50 \
  --amount 5 \
  --interval 15 \
  --check-interval 1
```

### Options

- `--to-address` - Recipient address (required)
- `--num-tests` - Number of transfers (default: 20)
- `--amount` - REV per transfer (default: 1)
- `--interval` - Seconds between tests (default: 10)
- `--check-interval` - Fast polling interval (default: 1s)
- `--chain-depth` - Main chain depth for orphan check (default: 200)
- `--private-key` - Signing key (default: test key)
- `-H, --host` - Node host (default: localhost)
- `-p, --port` - gRPC port (default: 40412)
- `--http-port` - HTTP port (default: 40413)

### Features

- **Fast polling**: 1-second check intervals for finalization (configurable)
- **Accurate detection**: Waits for finalization before determining orphan status
- **Detailed logging**: Timestamps, deploy IDs, block hashes, finalization times
- **Real-time stats**: Running finalized/failed percentages
- **Visual summary**: Bar charts and timing statistics
- **Single process**: No subprocess overhead, reused connections

### Example Output

```
╔═══════════════════════════════════════════╗
║  F1R3FLY Load Test                        ║
╚═══════════════════════════════════════════╝
Tests: 10
Amount: 1 REV
Interval: 10s
Check interval: 1s (fast mode)

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
🧪 Test 1/10
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
📤 [14:32:01] Deploying transfer...
✅ [14:32:02] Deploy submitted (850ms)
   Deploy ID: 3045022100abc...
⏳ [14:32:02] Waiting for block inclusion...
✅ [14:32:14] Included in block (12.0s)
   Block hash: def456...
🔍 [14:32:14] Waiting for block finalization...
✅ [14:32:32] Block finalized (18.0s)
✅ SUCCESS - Block finalized and on main chain

📊 Current Stats:
   ✅ Finalized: 1 (100%)
   ❌ Orphaned/Timeout: 0 (0%)
```

### Recommended Settings

**Light testing (devnet):**
```bash
cargo run -- load-test --to-address "111..." --num-tests 10 --amount 1 --interval 15
```

**Performance testing:**
```bash
cargo run -- load-test --to-address "111..." --num-tests 50 --amount 1 --interval 5
```
