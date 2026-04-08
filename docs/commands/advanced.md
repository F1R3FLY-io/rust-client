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

Test 2/3
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

Three event types are streamed:

- **Block Created** -- new block proposed by a validator. Includes block hash, creator, sequence number, parent hashes, and deploy IDs.
- **Block Added** -- block validated and added to the DAG by a receiving node. Same fields as Created.
- **Block Finalized** -- block reached finalized status (confirmed by consensus). Includes block hash, deploys with cost and errored status.

```
$ node_cli watch-blocks

Connected to node WebSocket
Watching for block events...

Block Created: a1b2c3d4... (creator: 0457feba..., seq: 293, deploys: 0, parents: 3)
Block Added: a1b2c3d4... (creator: 0457feba..., seq: 293, deploys: 0, parents: 3)
Block Added: e5f6a7b8... (creator: 04837a4c..., seq: 297, deploys: 0, parents: 3)
Block Finalized: 9c0d1e2f...
```

```
$ node_cli watch-blocks --filter finalized

Block Finalized: 9c0d1e2f...
Block Finalized: 3a4b5c6d...
```

Auto-reconnects on disconnect (10 retries by default, indefinitely with `--retry-forever`).

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
$ node_cli epoch-info -p 40452

Current Epoch Status:
   Current Block: 403
   Current Epoch: 40
   Epoch Length: 10 blocks
   Progress: 3/10 blocks (30.0%)
   Remaining: 7 blocks

   Recent Block Activity:
      Block 403: finalized
      Block 402: finalized
      Block 401: finalized
```

### epoch-rewards

```bash
node_cli epoch-rewards [-H HOST] [-p GRPC_PORT] [--http-port PORT]
```

```
$ node_cli epoch-rewards -p 40452 --http-port 40453

Current Epoch Rewards (3 validators):

   0457feba...b4ae661c : 1621929
   04837a4c...b2df065f : 1621929
   04fa70d7...00f60420 : 1621929

   Total: 4865787
```

### validator-status

```bash
node_cli validator-status -k <PUBLIC_KEY> [-H HOST] [-p GRPC_PORT] [--http-port PORT]
```

```
$ node_cli validator-status -k 0457febafcc25dd3...b4ae661c -p 40452 --http-port 40453

BONDED: Validator is bonded to the network
   Stake Amount: 1000
ACTIVE: Validator is actively participating in consensus

Summary:
   Bonded:  Yes
   Active:  Yes
   Status: Fully operational
```

### network-consensus

```bash
node_cli network-consensus [-H HOST] [-p GRPC_PORT] [--http-port PORT]
```

```
$ node_cli network-consensus -p 40452 --http-port 40453

Network Consensus Health:
   Current Block: 573
   Total Bonded Validators: 3
   Active Validators: 3
   Validators in Quarantine: 0
   Quarantine Length: 10 blocks
   Consensus Status: Healthy
   Participation Rate: 100.0%
```
