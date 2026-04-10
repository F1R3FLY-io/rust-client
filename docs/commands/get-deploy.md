# get-deploy

Get deploy information by deploy ID. Tries the detail view first (Rust node v0.4.11+), falls back to basic view on older or Scala nodes.

## Usage

```bash
node_cli get-deploy --deploy-id <ID> [OPTIONS]
```

## Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--deploy-id` | `-d` | required | Deploy ID (hex) |
| `--host` | `-H` | `localhost` | Node hostname |
| `--http-port` | | `40413` | HTTP port |
| `--format` | | `pretty` | Output format: `pretty`, `json`, `summary` |
| `--verbose` | `-v` | false | Show signature and VABN in pretty mode |

## Example: Detail view (Rust node v0.4.11+)

```
$ node_cli get-deploy --deploy-id 3044022075e51b8f...

Deploy Information
----------------------------------------
Deploy ID:    3044022075e51b8f...
Block Hash:   0519f656624c26e8a406ed4fd7f1fa9327f48128a0ad7758289bc09b8f646419
Block Number: 314
Deployer:     04ffc016579a68050d655d55df4e09f04605164543e257c8e6df10361e6068a533...
Cost:         316
Errored:      false
Phlo Price:   1
Phlo Limit:   50000
Timestamp:    1775610628069
Sig Algo:     secp256k1
Query time:   30.57ms
```

## Example: Basic view (older nodes / Scala)

When the node doesn't support the detail view, the command falls back:

```
$ node_cli get-deploy --deploy-id 3044022075e51b8f...

Deploy Information (basic view)
----------------------------------------
Deploy ID:    3044022075e51b8f...
Block Hash:   0519f656624c26e8...
Block Number: 314
Sender:       04ffc016579a6805...
Timestamp:    1775610628069
Query time:   12.31ms

Note: deploy execution details (cost, errored) require Rust node v0.4.11+
```

## Example: JSON format

On nodes with detail view, returns `DeployDetail`. On older nodes, returns raw block metadata JSON.

```
$ node_cli get-deploy --deploy-id 3044022075e51b8f... --format json

{
  "blockHash": "0519f656624c26e8...",
  "blockNumber": 314,
  "cost": 316,
  "errored": false,
  "deployer": "04ffc016579a680...",
  ...
}
```

## Example: Summary format

```
$ node_cli get-deploy --deploy-id 3044022075e51b8f... --format summary

Deploy 3044022075e51b8f... in block 0519f656... (#314) cost=316 errored=false
```

## Node Views

The command tries views in order:

| View | Endpoint | Available on | What it returns |
|------|----------|-------------|-----------------|
| detail | `?view=detail` | Rust v0.4.11+ | cost, errored, deployer, phlo, blockNumber |
| default | (no param) | All nodes | blockHash, seqNum, sender, timestamp |
