# get-deploy

Get deploy execution details by deploy ID. Returns cost, errored status, block number, and other metadata in a single call.

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

## Example: Pretty format (default)

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

## Example: JSON format

```
$ node_cli get-deploy --deploy-id 3044022075e51b8f... --format json

{
  "blockHash": "0519f656624c26e8a406ed4fd7f1fa9327f48128a0ad7758289bc09b8f646419",
  "blockNumber": 314,
  "timestamp": 1775610628069,
  "deployer": "04ffc016579a680...",
  "term": "new deployId(`rho:system:deployId`) in {\n  deployId!(42)\n}\n",
  "cost": 316,
  "errored": false,
  "systemDeployError": "",
  "phloPrice": 1,
  "phloLimit": 50000,
  "sig": "3044022075e51b8f...",
  "sigAlgorithm": "secp256k1",
  "validAfterBlockNumber": 313
}
```

## Example: Summary format

```
$ node_cli get-deploy --deploy-id 3044022075e51b8f... --format summary

Deploy 3044022075e51b8f... in block 0519f656... (#314) cost=316 errored=false
```
