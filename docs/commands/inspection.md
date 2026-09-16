# Node Inspection Commands

HTTP-based commands for querying node state.

## status

```bash
node_cli status [-H HOST] [-p HTTP_PORT]
```

Queries the node's `/api/status` endpoint and displays node identity, network membership, and native token metadata.

```
$ node_cli status -H localhost -p 40413

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
| Validator | Whether this node can propose blocks |
| Read Only | Whether this node is in read-only mode |
| Ready | Whether the engine has entered Running state |
| Epoch | Current epoch and epoch length from genesis config |

## blocks

```bash
node_cli blocks [-n COUNT] [--block-hash HASH] [-H HOST] [-p HTTP_PORT]
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
node_cli last-finalized-block [-H HOST] [-p HTTP_PORT]
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

Validators bonded in the PoS contract. A bond is listed as soon as its deploy executes. Bonds outside the active set are marked `(not active)`: the active set is recomputed at each epoch boundary, up to the shard's `number-of-active-validators`, so such a bond may never activate. Must run against observer/read-only node.

```bash
node_cli bonds [-H HOST] [-p HTTP_PORT]
```

```
$ node_cli bonds -H localhost -p 40453

Bonded Validators (4 total, 4000 total stake):

   1. 0429af98...455f1727 (stake: 1000) (not active)
   2. 0457feba...b4ae661c (stake: 1000)
   3. 04837a4c...b2df065f (stake: 1000)
   4. 04fa70d7...00f60420 (stake: 1000)
```

## active-validators

Validators participating in consensus. The set is recomputed only at epoch boundaries, up to the shard's `number-of-active-validators`, so a new bond appears here no earlier than one boundary after it appears in `bonds`. Must run against observer/read-only node.

```bash
node_cli active-validators [-H HOST] [-p HTTP_PORT]
```

```
$ node_cli active-validators -H localhost -p 40453

Active Validators (3 total, 3000 total stake):

   1. 0457feba...b4ae661c (stake: 1000)
   2. 04837a4c...b2df065f (stake: 1000)
   3. 04fa70d7...00f60420 (stake: 1000)
```

## wallet-balance

Must run against observer/read-only node.

```bash
node_cli wallet-balance --address <ADDRESS> [-H HOST] [-p GRPC_PORT]
```

```
$ node_cli wallet-balance -a 1111AtahZeefej4tvVR6ti9TJtv8yxLebT31SCEVDCKMNikBk5r3g -p 40452

Balance for 1111AtahZe...Bk5r3g: 49999999598463260
Block hash: 79574d57..., Block number: 400
```

## metrics

Returns Prometheus-format metrics from the node.

```bash
node_cli metrics [-H HOST] [-p HTTP_PORT]
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
node_cli show-main-chain [-d DEPTH] [-H HOST] [-p GRPC_PORT]
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
node_cli get-blocks-by-height -s <START> -e <END> [-H HOST] [-p GRPC_PORT]
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
node_cli block-transfers <BLOCK_HASH> [-H HOST]
```

```
$ node_cli block-transfers 860a56e195ff08ad...

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
node_cli bond-status -k <PUBLIC_KEY> [-H HOST] [-p HTTP_PORT]
```

| Flag | Default | Description |
|------|---------|-------------|
| `-p, --port` | `40413` | HTTP port of any node |

```
$ node_cli bond-status -k 0457febafcc25dd3...b4ae661c

Validator is BONDED
```

```
$ node_cli bond-status -k 04ffc016579a6805...3ad93d

Validator is NOT BONDED
```
