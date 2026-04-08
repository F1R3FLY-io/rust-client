# Advanced Commands

## load-test

Run a load test by sending multiple transfers and tracking finalization and orphan rates.

```bash
node_cli load-test --to-address <ADDR> --num-tests <N> --amount <AMT> [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--to-address` | required | Recipient address |
| `--num-tests` | required | Number of transfers to send |
| `--amount` | required | Amount per transfer (tokens) |
| `--interval` | `5` | Seconds between deploys |
| `--inclusion-timeout` | `120` | Max seconds for block inclusion |
| `--finalization-timeout` | `120` | Max seconds for finalization |
| `--check-interval` | `3` | Seconds between polls |
| `--chain-depth` | `10` | Depth to check for orphaned blocks |
| `--readonly-port` | same as port | Read-only gRPC port for balance check |

```
$ node_cli load-test --to-address 11112oRq...r2L --num-tests 3 --amount 1

F1R3FLY Load Test
Tests: 3
Amount: 1
Interval: 5s

Test 1/3
[17:24:25] Deploying transfer...
[17:24:25] Deploy submitted (87ms)
[17:24:25] Waiting for block inclusion...
[17:24:31] Included in block (6.1s)
[17:24:31] Waiting for block finalization...
[17:24:41] Block finalized (10.2s)
   SUCCESS - Block finalized and on main chain

...

FINAL RESULTS
Total tests: 3
Finalized:   3
Orphaned:    0
Timeout:     0
```

## watch-blocks

Monitor real-time block events via WebSocket.

```bash
node_cli watch-blocks [-H HOST] [--http-port PORT] [--filter TYPE] [--retry-forever]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--filter` | all | `created`, `added`, or `finalized` |
| `--retry-forever` | false | Reconnect indefinitely |

```
$ node_cli watch-blocks --filter finalized

Connected to node WebSocket at ws://localhost:40403/ws/events
Block Finalized: abc123def456...
Block Finalized: 789012345678...
```

Connects to `/ws/events` and streams block creation, validation, and finalization events. Auto-reconnects on disconnect (10 retries by default).

## dag

Interactive DAG visualization using a terminal UI (ratatui). Shows real-time block graph with parent/child relationships.

```bash
node_cli dag [-H HOST] [--http-port PORT]
```

Interactive -- requires a terminal with TUI support.

## bond-validator

Bond a new validator to the network. Deploys a bonding contract via the PoS system.

```bash
node_cli bond-validator --stake <AMOUNT> --private-key <KEY> [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--stake` | required | Stake amount |
| `--private-key` | required | Validator's signing key |
| `--propose` | false | Propose block after bonding |
| `--max-wait` | `300` | Max seconds for block inclusion |
| `--observer-host` | | Observer for finalization |
| `--observer-port` | `40452` | Observer gRPC port |

```
$ node_cli bond-validator --stake 1000 --private-key <KEY>

Bonding validator with stake: 1000
Deploy ID:    3045022100...
Block hash:   a1b2c3d4...
Total time:   25.30s
Bonding complete. Verify with: node_cli bonds
```

**Warning:** Only bond validators that are actually running nodes. Bonding a non-running validator breaks consensus.

## network-health

Check network health across multiple nodes.

```bash
node_cli network-health [-H HOST] [--recursive] [--depth N] [--custom-ports PORTS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--recursive` | false | Discover and check peers recursively |
| `--depth` | `1` | Recursion depth |
| `--custom-ports` | | Specific ports to check |
| `--standard-ports` | true | Check standard F1r3fly ports |

```
$ node_cli network-health --custom-ports 40413

Custom (localhost:40413): HEALTHY (4 peers)

Network Health Summary:
   Healthy nodes: 1/1
   Total peer entries: 4
   Average peers per node: 4.0
   All queried nodes are HEALTHY!
```

## PoS Query Commands

Query Proof-of-Stake contract state. All use exploratory deploy internally and must run against an observer node.

### epoch-info

```bash
node_cli epoch-info [-H HOST] [-p GRPC_PORT]
```

```
$ node_cli epoch-info -H localhost -p 40452

Current Epoch Status:
   Current Block: 403
   Current Epoch: 40
   Epoch Length: 10 blocks
   Progress: 3/10 blocks (30.0%)
   Remaining: 7 blocks
```

### epoch-rewards

```bash
node_cli epoch-rewards [-H HOST] [-p GRPC_PORT] [--http-port PORT]
```

Returns reward distribution data for the current epoch.

### validator-status

```bash
node_cli validator-status -k <PUBLIC_KEY> [-H HOST] [-p GRPC_PORT]
```

| Flag | Default | Description |
|------|---------|-------------|
| `-k` | required | Validator public key (hex) |

Reports whether the validator is BONDED, ACTIVE, or in QUARANTINE.

### network-consensus

```bash
node_cli network-consensus [-H HOST] [-p GRPC_PORT]
```

Returns network-wide consensus health overview including validator participation and finalization metrics.
