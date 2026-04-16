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

## watch-events

Monitor real-time node events via WebSocket. Connects to `/ws/events` and streams all 9 event types defined by the node. On connect, the node replays any startup events that occurred before the client connected.

```bash
node_cli watch-events [-H HOST] [--http-port PORT] [--filter TYPE] [--retry-forever]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--filter` | all | `created`, `added`, `finalized`, `genesis`, or `lifecycle` |
| `--retry-forever` | false | Reconnect indefinitely |

### Event types

| Type | Filter | Description |
|------|--------|-------------|
| Block Created | `created` | Block proposed by a validator (hash, creator, seq-num, deploys) |
| Block Added | `added` | Block validated and added to the DAG |
| Block Finalized | `finalized` | Block reached finalized status |
| Sent Unapproved Block | `genesis` | Boot broadcasts genesis candidate to validators |
| Block Approval Received | `genesis` | Boot receives approval from a validator |
| Sent Approved Block | `genesis` | Boot broadcasts the approved genesis block |
| Approved Block Received | `genesis` | Validator receives the approved genesis block |
| Entered Running State | `lifecycle` | Node engine transitions to Running |
| Node Started | `lifecycle` | Node HTTP server is ready |

### Examples

```
$ node_cli watch-events

 Node Started
 Address: rnode://871fc407...@localhost?protocol=40400&discovery=40404

 Entered Running State
 Block: 37115b862d...

 Block Created
 Hash:     7d88a8f291...
 Creator:  04ffc01657...
 Seq Num:  1
 Parents:  1
 Deploys:  0

 Block Finalized
 Hash:     7d88a8f291...
```

```
$ node_cli watch-events --filter finalized

 Block Finalized
 Hash:     7d88a8f291...
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
