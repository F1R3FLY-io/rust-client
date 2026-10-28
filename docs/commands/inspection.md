# Node Inspection Commands

Commands for querying node state. HTTP commands take `--http <URL>` (default `http://localhost:40403`); gRPC commands take `--grpc <URL>` (default `http://localhost:40401`). Commands that need a read-only node (`bonds`, `active-validators`, `wallet-balance`, `block-transfers`) have no default and require the flag. An HTTP endpoint may carry a base path, such as `https://proxy.example/node1`, for a node served under a prefix behind a proxy; a gRPC endpoint takes no path. Every command also takes `--ca-cert <PEM file>`, the certificate authorities trusted for an `https://` endpoint in place of the bundled public roots (an error with an `http://` endpoint), `--connect-timeout <SECS>` (default 10), the seconds a connection may take to establish, and `--timeout <SECS>` (default 30), the seconds a single request may take.

On the system-integration single-host Docker shard, validator1's HTTP endpoint is `http://localhost:40413` and the read-only node's is `http://localhost:40453` (gRPC `http://localhost:40452`).

## status

```bash
node_cli status [--http URL]
```

Queries the node's `/api/status` endpoint and displays node identity, network membership, and native token metadata.

`status`, `blocks`, `metrics` and `last-finalized-block` exit non-zero when the
node answers with an HTTP error.

```
$ node_cli status --http http://localhost:40413

  Address:       rnode://24f31580...@rnode.validator1?protocol=40400&discovery=40404
  Network:       testnet
  Shard:         root
  Peers:         4
  Nodes:         4
  Min Phlo:      1
  Native Token:  F1R3CAP (F1R3, 8 decimals)
  LFB Number:    126
  Validator:     false
  Read Only:     false
  Ready:         true
  Epoch:         12 (length: 10)
  Version:       {"api":"1","node":"F1r3node Rust 0.4.10 ()"}
```

| Field | Description |
|-------|-------------|
| Native Token | Name, symbol, decimals — baked into genesis, immutable |
| LFB Number | Last finalized block number (-1 if not yet initialized) |
| Validator | Whether autopropose is enabled — **not** whether the node is a bonded validator (f1r3node-rust#429). A bonded, actively proposing validator reports `false` here. Use `validator-status` to check bonding |
| Read Only | Whether this node is in read-only mode |
| Ready | Whether the engine has entered Running state |
| Epoch | Current epoch and epoch length from genesis config |

## blocks

```bash
node_cli blocks [-n COUNT] [--block-hash HASH] [--http URL]
```

| Flag | Default | Description |
|------|---------|-------------|
| `-n` | `5` | Number of recent blocks |
| `--block-hash` | | Get specific block by hash |

```
$ node_cli blocks -n 1

[
  {
    "blockInfo": {
      "blockHash": "a47bdb405fc3ccba...",
      "blockNumber": 128,
      "isFinalized": true,
      "faultTolerance": 1.0,
      "sender": "0457febafcc25dd3...",
      "timestamp": 1776898700000,
      "deployCount": 0,
      ...
    }
  }
]
```

Block list responses use the `blockInfo` wrapper (summary view by default, deploys omitted).

## last-finalized-block

```bash
node_cli last-finalized-block [--http URL]
```

```
$ node_cli last-finalized-block

Last Finalized Block Summary:
   Block Number: 399
   Block Hash: 99f52d9b87d6da3bc8ebfdde6171d48c90da80b44aa1ec6dea7f9a1dc9d077c3
   Timestamp: 1775611773312
   Deploy Count: 0
   Shard ID: root
   Fault Tolerance: 1.000000
```

## bonds

Validators bonded in the PoS contract. A bond is listed as soon as its deploy executes. Bonds outside the active set are marked `(not active)`: the active set is recomputed at each epoch boundary, up to the shard's `number-of-active-validators`, so such a bond may never activate. Must run against a read-only node.

```bash
node_cli bonds --http URL
```

```
$ node_cli bonds --http http://localhost:40453

Bonded Validators (4 total, 4000 total stake):

   1. 0429af98...455f1727 (stake: 1000) (not active)
   2. 0457feba...b4ae661c (stake: 1000)
   3. 04837a4c...b2df065f (stake: 1000)
   4. 04fa70d7...00f60420 (stake: 1000)
```

## active-validators

Validators participating in consensus. The set is recomputed only at epoch boundaries, up to the shard's `number-of-active-validators`, so a new bond appears here no earlier than one boundary after it appears in `bonds`. Must run against a read-only node.

```bash
node_cli active-validators --http URL
```

```
$ node_cli active-validators --http http://localhost:40453

Active Validators (3 total, 3000 total stake):

   1. 0457feba...b4ae661c (stake: 1000)
   2. 04837a4c...b2df065f (stake: 1000)
   3. 04fa70d7...00f60420 (stake: 1000)
```

## wallet-balance

Must run against a read-only node.

```bash
node_cli wallet-balance --address <ADDRESS> --grpc URL
```

```
$ node_cli wallet-balance -a 1111AtahZeefej4tvVR6ti9TJtv8yxLebT31SCEVDCKMNikBk5r3g --grpc http://localhost:40452

Balance for 1111AtahZe...Bk5r3g: 49999999598463260
Block hash: 79574d57..., Block number: 400
```

## metrics

Returns Prometheus-format metrics from the node.

```bash
node_cli metrics [--http URL]
```

```
$ node_cli metrics

Key Metrics (peers, blocks, consensus):
block_requests_total{source="f1r3fly.casper.block-retriever"} 1071
casper_init_attempts{source="f1r3fly.casper"} 1
block_validation_success{source="f1r3fly.casper.block-processor"} 1071
comm_consume{source="f1r3fly.rspace"} 66608
comm_produce{source="f1r3fly.rspace.replay"} 47088
...
```

## show-main-chain

Get blocks in the main chain via gRPC.

```bash
node_cli show-main-chain [-d DEPTH] [--grpc URL]
```

```
$ node_cli show-main-chain -d 2

Found 2 blocks in main chain

Main Chain Blocks:
   Block #402:
      Hash: c6f93059d8bb3a0a...
      Sender: 0457febafcc25dd3...
      Deploy Count: 0

   Block #401:
      Hash: 207c329164cdbaaa...
      Sender: 0457febafcc25dd3...
      Deploy Count: 0
```

## get-blocks-by-height

Returns blocks in the specified height range via gRPC streaming.

```bash
node_cli get-blocks-by-height -s <START> -e <END> [--grpc URL]
```

```
$ node_cli get-blocks-by-height -s 1 -e 2

Found 5 blocks in height range

Blocks by Height:
   Block #1:
      Hash: f760d02df075...
      Sender: 04837a4cff83...
      Deploy Count: 0

   Block #1:
      Hash: 86eb29ed2612...
      Sender: 0457febafcc2...
      Deploy Count: 0

   Block #2:
      Hash: a4033c308099...
      ...
```

Multiple blocks at the same height indicate parallel proposals from different validators.

## block-transfers

Extracts native token transfers from a block's deploys.

```bash
node_cli block-transfers <BLOCK_HASH> --http URL [--all-deploys] [--max-wait SECONDS]
```

Point it at a read-only node: a validator never attaches transfers, so against one the command waits out `--max-wait` and fails. The read-only node attaches transfers to a block's deploys once it has replayed the block. Until then the command re-reads the block every 2 seconds, up to `--max-wait` (default 60), and exits with an error if the report never arrives; it never reports a pending block as having no transfers.

```
$ node_cli block-transfers 860a56e195ff08ad... --http http://localhost:40453

Block #587 (860a56e195ff08ad...)

Summary:
   Total deploys in block: 0
   Deploys with transfers: 0
   Total transfers: 0
```

When a block contains transfer deploys, this shows from/to addresses, amounts, and success status.

## bond-status

Checks whether a public key is bonded as of the last finalized block. Works against any node. A bond counts as soon as its deploy finalizes, before activation; use `validator-status` for activation and withdrawal progress.

```bash
node_cli bond-status -k <PUBLIC_KEY> [--http URL]
```

| Flag | Default | Description |
|------|---------|-------------|
| `--http` | `http://localhost:40403` | HTTP endpoint of any node, optionally with a base path |

```
$ node_cli bond-status -k 0457febafcc25dd3...b4ae661c

Validator is BONDED
```

```
$ node_cli bond-status -k 04ffc016579a6805...3ad93d

Validator is NOT BONDED
```
