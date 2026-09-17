# Advanced Commands

## load-test

Run a load test by sending multiple transfers and tracking finalization and orphan rates.

```bash
node_cli load-test --to-address <ADDR> --num-tests <N> --amount <AMT> [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--to-address` | required | Recipient address |
| `--private-key` | required (`FIREFLY_PRIVATE_KEY`) | Key for signing |
| `--num-tests` | `20` | Number of transfers to send |
| `--amount` | `1` | Amount per transfer in base units (dust). Use `--whole-tokens`/`-d` for whole tokens. |
| `--whole-tokens`, `-d` | `false` | Treat `--amount` as whole tokens, scaled by the native token's decimals from node status |
| `--interval` | `10` | Seconds between deploys |
| `-H`, `--host` | `localhost` | Node host |
| `-p`, `--port` | `40412` | Node gRPC port |
| `--http-port` | `40413` | Node HTTP port for status queries |
| `--inclusion-timeout` | `120` | Max seconds for block inclusion |
| `--finalization-timeout` | `120` | Max seconds for finalization |
| `--check-interval` | `1` | Seconds between polls |
| `--chain-depth` | `200` | Depth to check for orphaned blocks |
| `--readonly-port` | `40452` | Read-only gRPC port for balance check |

```
$ node_cli load-test --to-address 11112oRq...r2L --num-tests 3 --amount 100000000

F1R3FLY Load Test
Tests: 3
Amount: 100000000 dust
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

Monitor real-time node events via WebSocket. Connects to `/ws/events` and streams the node's events. On connect, the node replays any startup events that occurred before the client connected.

The node defines ten event types (`F1r3flyEvent`); this client deserializes the nine below. `block-approval-received`, emitted during the genesis ceremony, is not currently surfaced.

```bash
node_cli watch-events [-H HOST] [--http-port PORT] [--filter TYPE] [--retry-forever]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--filter` | all | `created`, `added`, `finalized`, `transfers`, `genesis`, or `lifecycle` |
| `--retry-forever` | false | Reconnect indefinitely |

### Event types

| Type | Filter | Description |
|------|--------|-------------|
| Block Created | `created` | Block proposed by a validator (hash, block#, timestamp, creator, deploys) |
| Block Added | `added` | Block validated and added to the DAG |
| Block Finalized | `finalized` | Block reached finalized status |
| Transfers Available | `transfers` | Transfer extraction completed (readonly only, after block report) |
| Sent Unapproved Block | `genesis` | Boot broadcasts genesis candidate to validators |
| Sent Approved Block | `genesis` | Boot broadcasts the approved genesis block |
| Approved Block Received | `genesis` | Validator receives the approved genesis block |
| Entered Running State | `lifecycle` | Node engine transitions to Running |
| Node Started | `lifecycle` | Node HTTP server is ready |

Block events include `Block #` (block number) and `Time` (timestamp). Transfer events show per-deploy transfer details (from/to/amount/success).

### Examples

```
$ node_cli watch-events

 Node Started
 Address: rnode://24f31580...@rnode.validator1?protocol=40400&discovery=40404

 Block Created
 Hash:     25ad58ad271df3e5...
 Block #:  134
 Time:     1776898716907
 Creator:  04fa70d7be5eb750...
 Seq Num:  113
 Parents:  3
 Deploys:  0

 Block Finalized
 Hash:     6dcbb0d170f5be7b...
 Block #:  132
 Time:     1776898685324
 Creator:  0457febafcc25dd3...
 Seq Num:  118
 Parents:  3
 Deploys:  0
```

On readonly nodes, transfer events appear after block finalization:

```
$ node_cli watch-events --http-port 40453

 Transfers Available
 Block:    edc71efd1dd41be6... (#130)
 Deploys:  1
   Deploy: 3044022015f80a59...  (1 transfers)
     1111AtahZeefej4t -> 111127RX5ZgiAdRa : 100000000 (ok)
```

```
$ node_cli watch-events --filter finalized

 Block Finalized
 Hash:     6dcbb0d170f5be7b...
 Block #:  132
 ...
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
| `-H`, `--host` | `localhost` | Node to submit the deploy to — must be a **bonded validator** |
| `-p`, `--port` | `40412` | That node's gRPC port |
| `--http-port` | `40413` | That node's HTTP port |
| `--propose` | false | Propose block after bonding |
| `--max-wait` | `300` | Max seconds to wait for finalization |
| `--observer-host` | | Observer for finalization |
| `--observer-port` | `40452` | Observer gRPC port |
| `--observer-http-port` | `40453` | Observer HTTP port polled for finalization status |

**Submit to a bonded validator.** Any other node — including a bootstrap or
ceremony-master node, and the validator being bonded, which is not yet in the
bond set — accepts the deploy and strands it, because nothing can ever include
it (f1r3node-rust#427). The command then runs out `--max-wait` and reports a
timeout.

**Fund the validator's vault first.** The bond is paid for by the key being
bonded, and the vault must hold `phlo limit × phlo price` **plus** the stake at
submission. `bond-validator` uses the bigger phlo limit, 5,000,000,000, and the
default phlo price is 1. An under-funded vault does not reject the deploy: it
is included in a block and fails there with
`systemDeployError: "Deploy payment failed: Insufficient funds"`. See
[Bonding a validator](../guides/bonding-a-validator.md) for the full sequence.

```
$ node_cli bond-validator --stake 1000 --private-key <KEY>

Bonding validator with stake: 1000
Deploy ID:    3045022100...
Block hash:   a1b2c3d4...
Total time:   25.30s
Bonding complete. Verify with: node_cli bonds
```

The command exits non-zero when the PoS contract rejects the bond:

```
$ node_cli bond-validator --stake 1000 --private-key <ALREADY_BONDED_KEY>

Bonding validator with stake: 1000
Deploy ID: 3045022100e415...
Block hash: 63fb62fd...
Total time: 21.14s
 Bond rejected by PoS: Public key is already bonded.
```

It also exits non-zero, without claiming a rejection, when the deploy finalized but its verdict could not be read: `Bond deploy finalized, but its PoS verdict could not be read: <error>`. Check the outcome with `validator-status`.

A bond joins the active set at an epoch boundary only while the shard's `number-of-active-validators` has room; past that limit it stays bonded but not active.

**Warning:** Only bond validators that are actually running nodes. Bonding a non-running validator breaks consensus.

## unbond-validator

Withdraw a validator's bond. Deploys the PoS `withdraw` call signed by the validator's key; no node needs to run for that key.

```bash
node_cli unbond-validator --private-key <KEY> [OPTIONS]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--private-key` | required | Signing key of the validator to unbond |
| `-H`, `--host` | `localhost` | Node to submit the deploy to — must be a **bonded validator** |
| `-p`, `--port` | `40412` | That node's gRPC port |
| `--http-port` | `40413` | That node's HTTP port |
| `--max-wait` | `300` | Max seconds to wait for finalization |
| `--observer-host` | | Observer for finalization |
| `--observer-port` | `40452` | Observer gRPC port |
| `--observer-http-port` | `40453` | Observer HTTP port polled for finalization status |

The same routing rule as `bond-validator` applies: submit to a bonded
validator, or the deploy is accepted and never included.

The withdrawal takes effect in stages, each at an epoch boundary:

1. The deploy records the withdrawal request. The validator stays bonded and active.
2. At the next epoch boundary, the validator leaves the bond set and the active set.
3. At the first epoch boundary at or after the quarantine end, the stake and accumulated rewards are paid to the validator's vault.

The quarantine end is `quarantine-length + epoch-length * (1 + B / epoch-length)`, where `B` is the block that holds the deploy. `validator-status` shows the request with its quarantine end, and then the pending payout.

```
$ node_cli unbond-validator --private-key <KEY>

Requesting withdrawal for validator: 0429af984ed4da1a...455f1727
Deploy ID: 3044022065871c6d...
Block hash: b0737488...
Total time: 23.32s
Withdrawal requested. The validator leaves the bond set at the next epoch boundary.
Track progress with: node_cli validator-status -k 0429af984ed4da1a...455f1727
```

The command exits non-zero when the PoS contract rejects the call, for example `Withdrawal rejected by PoS: User is not bonded`, and when the verdict could not be read.

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

Query Proof-of-Stake contract state. All are built on exploratory deploys, which
only a **read-only node** serves — against any other node they fail with
`Exploratory deploy can only be executed on read-only node`.

### Port and transport

The read-only node exposes both a gRPC and an HTTP port, and these commands do
not all use the same one. Passing the gRPC port to an HTTP command fails with
`error sending request for url (…/api/explore-deploy)`, which does not name the
real problem.

| Command | Transport | Port flag | Default |
|---|---|---|---|
| `bonds` | HTTP | `-p` | `40453` |
| `active-validators` | HTTP | `-p` | `40453` |
| `validator-status` | HTTP | `--http-port` | `40453` |
| `network-consensus` | HTTP | `--http-port` | `40453` |
| `epoch-rewards` | HTTP | `--http-port` | `40453` |
| `epoch-info` | gRPC | `-p` | `40452` |
| `wallet-balance` | gRPC | `-p` | `40452` |

`epoch-info` also accepts `--http-port`, but does not read it.

Two non-PoS commands share the confusion, since they default to different
nodes: `status` defaults to `40453` (the observer), while `blocks` defaults to
`40413` (validator1).

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
node_cli validator-status -k <PUBLIC_KEY> [-H HOST] [--http-port PORT]
```

Reports the validator's state as of the last finalized block:

- `BONDED` with the stake, then `ACTIVE` or `NOT ACTIVE`. The active set is recomputed at each epoch boundary, up to the shard's `number-of-active-validators`, so a bond outside it is not guaranteed to activate.
- `WITHDRAWAL REQUESTED` after `unbond-validator`, with the block at or after which stake and rewards become payable.
- `WITHDRAWING` after the validator leaves the bond set, until the payout.

```
$ node_cli validator-status -k 0457febafcc25dd3...b4ae661c --http-port 40453

BONDED: stake 1000
ACTIVE: participating in consensus

As of block: 573 (last finalized)
```

### network-consensus

```bash
node_cli network-consensus [-H HOST] [--http-port PORT]
```

```
$ node_cli network-consensus --http-port 40453

Network Consensus Health:
   As of block: 573 (last finalized)
   Total Bonded Validators: 3
   Active Validators: 3
   Bonded, Not Active: 0
   Pending Withdrawals: 0
   Withdrawing: 0
   Withdrawal Quarantine Length: 10 blocks
   Consensus Status: Healthy
   Participation Rate: 100.0%
```
