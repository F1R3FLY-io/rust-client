# transfer

Transfer native tokens between vault addresses. Deploys a transfer contract, waits for finalization, and reports the result.

## Usage

```bash
node_cli transfer --to-address <ADDRESS> --amount <AMOUNT> [OPTIONS]
```

## Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--to-address` | `-t` | required | Recipient vault address (starts with `1111`) |
| `--amount` | `-a` | required | Amount in tokens (1 token = 100,000,000 dust) |
| `--private-key` | | dev key | Sender's signing key |
| `--host` | `-H` | `localhost` | Node hostname |
| `--port` | `-p` | `40412` | gRPC port |
| `--http-port` | | `40413` | HTTP port |
| `--bigger-phlo` | `-b` | true | Use high phlo limit (recommended) |
| `--propose` | | false | Also propose a block after deploy |
| `--max-wait` | | `300` | Max seconds for block inclusion |
| `--check-interval` | | `5` | Seconds between polls |
| `--observer-host` | | same as host | Observer for finalization |
| `--observer-port` | | `40452` | Observer gRPC port |
| `--expiration` | | none | Expiration timestamp (ms) |
| `--expires-in` | | none | Expiration duration (seconds) |

## Example

```
$ node_cli transfer --to-address 111127RX5ZgiAdRaQy4AWy57RdvAAckdELReEBxzvWYVvdnR32PiHA --amount 1

Transfer: 1111AtahZe...r3g -> 111127RX5Z...iHA (100000000 dust)
Deploy ID:    3045022100...
Block hash:   a1b2c3d4...
Cost:         45231
Total time:   23.70s
Transfer complete.
```

## Notes

- The sender address is derived from the private key automatically
- Amount is in whole tokens — converted to dust internally (1 token = 100,000,000 dust)
- Uses high phlo limit by default because transfer contracts are expensive
- Vault addresses must start with `1111`
