# deploy

Submit Rholang code to the blockchain without waiting for finalization.

Returns immediately after the node accepts the deploy. Use [deploy-and-wait](deploy-and-wait.md) if you need to wait for the result.

## Usage

```bash
node_cli deploy -f <FILE> [OPTIONS]
```

## Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--file` | `-f` | required | Rholang file to deploy |
| `--private-key` | `-k` | dev key | Signing key (64 hex chars) |
| `--grpc` | | `http://localhost:40401` | Node gRPC endpoint, `http://` or `https://`, with no path |
| `--ca-cert` | | none | PEM file of the certificate authorities trusted for `https://` endpoints, in place of the bundled public roots |
| `--connect-timeout` | | `10` | Seconds a connection to a node may take to establish (at least 1) |
| `--timeout` | | `30` | Seconds a single request to a node may take (at least 1) |
| `--phlo-limit` | | `50000` | Most phlo the deploy may spend |
| `--phlo-price` | | `1` | Price per phlo, in the native token's base units |
| `--shard-id` | | `root` | Shard the deploy is signed for: the node's `casper.shard-name` (env `FIREFLY_SHARD_ID`) |
| `--expiration` | | none | Expiration timestamp (ms, Unix epoch) |
| `--expires-in` | | none | Expiration duration (seconds from now) |

## Example

```
$ node_cli deploy -f ./rho_examples/stdout.rho

Reading Rholang from: ./rho_examples/stdout.rho
Code size: 62 bytes
Connecting to F1r3fly node at http://localhost:40401
Using phlo limit: 50000
Deploying Rholang code...
Deployment successful!
Time taken: 87.58ms
Deploy ID: 3045022100a7378028e7bdfb8ea7c908f5effc1d2018a0448090e14be5f35ba722251cf2bf02205e2146cf93018c56011f45e5ab8256dc3a767e705eaf321e23b63bb794661dc1
```

## Notes

- The deploy is submitted to the node but NOT yet in a block
- **Submit to a bonded validator.** The deploy queue is node-local and deploys
  do not gossip, so a node that cannot propose — a bootstrap or ceremony-master
  node, or an unbonded validator — accepts the deploy, returns a deploy ID, and
  never includes it (f1r3node-rust#427). Nothing reports an error; the deploy
  simply never appears in a block
- On shards with heartbeat enabled, the node auto-proposes — no manual `propose` needed
- The deploy ID is the DER-encoded secp256k1 signature of the deploy data
