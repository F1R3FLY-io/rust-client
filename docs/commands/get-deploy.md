# get-deploy

Get deploy information by deploy ID. Returns the unified `DeployResponse` with all execution details.

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
| `--verbose` | `-v` | false | Show VABN in pretty mode |

## Example

```
$ node_cli get-deploy -d 304502210085f163...

Deploy Information
----------------------------------------
Deploy ID:    304502210085f163934f0de4c8eadb177d62b9527997100ff3b80d868dbfa702c2...
Block Hash:   79d3560b36998644d139ba0f73a3883274f28f22b6f2016e973f3606be38bb56
Block Number: 130
Finalized:    true
Deployer:     04ffc016579a68050d655d55df4e09f04605164543e257c8e6df10361e6068a533...
Cost:         317
Errored:      false
Phlo Price:   1
Phlo Limit:   50000
Timestamp:    1776898667421
Sig Algo:     secp256k1
Query time:   15.38ms
```

## Response Fields

| Field | Always present | Description |
|-------|---------------|-------------|
| `deployId` | Yes | Deploy signature ID |
| `blockHash` | Yes | Containing block hash |
| `blockNumber` | Yes | Block height |
| `timestamp` | Yes | Block timestamp |
| `cost` | Yes | Phlogiston consumed |
| `errored` | Yes | Whether execution failed |
| `isFinalized` | Yes | Whether containing block is finalized |
| `deployer` | Full view | Deployer public key |
| `term` | Full view | Rholang source |
| `systemDeployError` | Full view | System deploy error (empty if none) |
| `phloPrice` | Full view | Phlogiston price |
| `phloLimit` | Full view | Phlogiston limit |
| `sigAlgorithm` | Full view | Signature algorithm |
| `validAfterBlockNumber` | Full view | Valid-after constraint |
| `transfers` | Full view | Transfer list (null on validators, populated on readonly) |

## Summary format

```
$ node_cli get-deploy -d 304502210085f163... --format summary

Deploy 304502210085f163... in block 79d3560b... (#130) cost=317 errored=false finalized=true
```
