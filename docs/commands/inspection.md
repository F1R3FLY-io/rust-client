# Node Inspection Commands

HTTP-based commands for querying node state.

## status

```bash
node_cli status [-H HOST] [-p HTTP_PORT]
```

```
$ node_cli status

Node Status:
{
  "address": "rnode://24f31580...@rnode.validator1?protocol=40400&discovery=40404",
  "nodes": 4,
  "peers": 4,
  "version": "F1r3node Rust 0.4.10 ()",
  "peerList": [
    { "host": "rnode.validator2", "isConnected": true, ... },
    { "host": "rnode.validator3", "isConnected": true, ... },
    { "host": "rnode.bootstrap", "isConnected": true, ... },
    { "host": "rnode.readonly", "isConnected": true, ... }
  ]
}
```

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
    "blockHash": "c6f93059d8bb3a0a...",
    "blockNumber": 402,
    "deployCount": 0,
    "faultTolerance": 0.0,
    "sender": "0457febafcc25dd3...",
    "timestamp": 1775611821639,
    ...
  }
]
```

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

Get current validator bonds from PoS contract. Must run against observer/read-only node.

```bash
node_cli bonds [-H HOST] [-p HTTP_PORT]
```

```
$ node_cli bonds -H localhost -p 40453

Bonded Validators (3 total, 3000 total stake):

   1. 0457feba...b4ae661c (stake: 1000)
   2. 04837a4c...b2df065f (stake: 1000)
   3. 04fa70d7...00f60420 (stake: 1000)
```

## active-validators

```bash
node_cli active-validators [-H HOST] [-p HTTP_PORT]
```

Must run against observer/read-only node. Similar output to `bonds`.

## wallet-balance

```bash
node_cli wallet-balance --address <ADDRESS> [-H HOST] [-p GRPC_PORT]
```

Must run against observer/read-only node.

```
$ node_cli wallet-balance -a 1111AtahZeefej4tvVR6ti9TJtv8yxLebT31SCEVDCKMNikBk5r3g -H localhost -p 40452

Balance for 1111AtahZe...Bk5r3g: 49999999598463260
Block hash: 79574d57..., Block number: 400
```

## metrics

```bash
node_cli metrics [-H HOST] [-p HTTP_PORT]
```

Returns Prometheus-format metrics from the node.

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
      Timestamp: 1775611821639
      Deploy Count: 0

   Block #401:
      Hash: 207c329164cdbaaa...
      Sender: 0457febafcc25dd3...
      Timestamp: 1775611805852
      Deploy Count: 0
```

## get-blocks-by-height

```bash
node_cli get-blocks-by-height -s <START> -e <END> [-H HOST] [-p GRPC_PORT]
```

Returns blocks in the specified height range via gRPC streaming.

## block-transfers

```bash
node_cli block-transfers <BLOCK_HASH> [-H HOST] [--http-port PORT]
```

Extracts and displays native token transfers from a block's deploys.

## bond-status

```bash
node_cli bond-status -k <PUBLIC_KEY> [-H HOST] [-p HTTP_PORT]
```

Checks if a validator public key appears in the bonds list. Must run against observer.
