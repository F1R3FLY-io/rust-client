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
| `--host` | `-H` | `localhost` | Node hostname |
| `--port` | `-p` | `40412` | gRPC port |
| `--bigger-phlo` | `-b` | false | Use 5B phlo limit instead of 50K |
| `--expiration` | | none | Expiration timestamp (ms, Unix epoch) |
| `--expires-in` | | none | Expiration duration (seconds from now) |

## Example

```
$ node_cli deploy -f ./rho_examples/stdout.rho

Reading Rholang from: ./rho_examples/stdout.rho
Code size: 62 bytes
Connecting to F1r3fly node at localhost:40412
Using phlo limit: 50,000
Deploying Rholang code...
Deployment successful!
Time taken: 87.58ms
Deploy ID: 3045022100a7378028e7bdfb8ea7c908f5effc1d2018a0448090e14be5f35ba722251cf2bf02205e2146cf93018c56011f45e5ab8256dc3a767e705eaf321e23b63bb794661dc1
```

## Notes

- The deploy is submitted to the node but NOT yet in a block
- On shards with heartbeat enabled, the node auto-proposes — no manual `propose` needed
- The deploy ID is the DER-encoded secp256k1 signature of the deploy data
