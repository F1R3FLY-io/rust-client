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
| `--amount` | `-a` | required | Amount in base units (dust) by default. Use `--whole-tokens`/`-d` for whole tokens. |
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
| `--whole-tokens` | `-d` | false | Treat `--amount` as whole tokens, scaled by the native token's decimals from node status (default: base units / dust) |

## Example

```
$ node_cli transfer --to-address 111127RX5ZgiAdRaQy4AWy57RdvAAckdELReEBxzvWYVvdnR32PiHA --amount 100000000

Transfer: 1111AtahZe...r3g -> 111127RX5Z...iHA (100000000 dust)
Deploy ID:    3045022100...
Block hash:   a1b2c3d4...
Cost:         45231
Total time:   23.70s
Transfer complete.
```

Or with `--whole-tokens` / `-d` to specify whole tokens:

```
$ node_cli transfer --to-address 111127RX5ZgiAdRaQy4AWy57RdvAAckdELReEBxzvWYVvdnR32PiHA --amount 1 -d

Transfer: 1111AtahZe...r3g -> 111127RX5Z...iHA (100000000 dust)
...
```

## Notes

- The sender address is derived from the private key automatically
- `--amount` is in base units (dust) by default; use `--whole-tokens`/`-d` to specify whole tokens (scaled by the native token's decimals fetched from node status)
- Uses high phlo limit by default because transfer contracts are expensive
- Vault addresses must start with `1111`
