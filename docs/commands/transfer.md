# transfer

Transfer native tokens between vault addresses. Deploys a transfer contract, waits for finalization, and reports the vault's verdict. The command exits non-zero when the deploy errors, when the vault rejects the transfer (for example `Transfer failed: vault rejected the transfer: Insufficient funds`), and when the verdict could not be read.

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
| `--host` | `-H` | `localhost` | Node hostname — must be a **bonded validator** |
| `--port` | `-p` | `40412` | gRPC port |
| `--http-port` | | `40413` | HTTP port |
| `--bigger-phlo` | `-b` | true | Use the 5,000,000,000 phlo limit instead of 50,000 |
| `--propose` | | false | Also propose a block after deploy |
| `--max-wait` | | `300` | Max seconds to wait for finalization |
| `--check-interval` | | `5` | Seconds between finalization status polls |
| `--observer-host` | | same as host | Observer for finalization |
| `--observer-port` | | `40452` | Observer gRPC port |
| `--observer-http-port` | | `40453` | Observer HTTP port polled for finalization status |
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

- **Submit to a bonded validator.** Any other node — a bootstrap or
  ceremony-master node, or an unbonded validator — accepts the deploy and
  strands it, because nothing can ever include it (f1r3node-rust#427). The
  command then runs out `--max-wait` and reports a timeout, and the deploy is
  never found.
- The sender address is derived from the private key automatically
- `--amount` is in base units (dust) by default; use `--whole-tokens`/`-d` to specify whole tokens (scaled by the native token's decimals fetched from node status)
- Uses the bigger phlo limit (5,000,000,000) by default because transfer
  contracts are expensive. The sender's vault must cover `phlo limit × phlo
  price` plus the amount being transferred
- `Transfer complete.` is printed only after the vault reports the transfer accepted on the deploy's `deployId` channel
- Vault addresses must start with `1111`
