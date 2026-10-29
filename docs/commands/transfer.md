# transfer

Transfer native tokens between vault addresses. Deploys a transfer contract, waits for finalization, and reports the vault's verdict. The command exits non-zero when the deploy errors, when the vault rejects the transfer (for example `the vault rejected the call: Insufficient funds`), and when the verdict could not be read.

## Usage

```bash
node_cli transfer --to-address <ADDRESS> --amount <AMOUNT> [OPTIONS]
```

## Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--to-address` | `-t` | required | Recipient vault address (starts with `1111`) |
| `--amount` | `-a` | required | Amount in base units (dust) by default. Use `--whole-tokens`/`-d` for whole tokens. |
| `--private-key-file` | | none | File holding the sender's signing key's hex; must not be readable by group or others. Default: `FIREFLY_PRIVATE_KEY` |
| `--grpc` | | `http://localhost:40401` | gRPC endpoint, with no path, of the node to submit to — must be a **bonded validator** |
| `--http` | | `http://localhost:40403` | That node's HTTP endpoint, optionally with a base path |
| `--ca-cert` | | none | PEM file of the certificate authorities trusted for `https://` endpoints, in place of the bundled public roots; refused with an `http://` endpoint |
| `--connect-timeout` | | `10` | Seconds a connection to a node may take to establish (at least 1) |
| `--timeout` | | `30` | Seconds a single request to a node may take (at least 1) |
| `--phlo-limit` | | `500000` | Most phlo the transfer deploy may spend |
| `--phlo-price` | | `1` | Price per phlo, in the native token's base units |
| `--shard-id` | | `root` | Shard the deploy is signed for: the node's `casper.shard-name` (env `FIREFLY_SHARD_ID`) |
| `--propose` | | false | Also propose a block after deploy; `--grpc` must then be the node's internal API |
| `--max-wait` | | `300` | Max seconds to wait for finalization |
| `--check-interval` | | `5` | Seconds between finalization status polls |
| `--observer` | | none | HTTP endpoint, optionally with a base path, of a second node polled alongside the deploy node for the verdict; without it, only the deploy node is polled |
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
- The sender address is derived from the signing key automatically
- `--amount` is in base units (dust) by default; use `--whole-tokens`/`-d` to specify whole tokens (scaled by the native token's decimals fetched from node status)
- Deploys with a 500,000 phlo limit by default: the transfer contract costs
  about 250,000. The sender's vault must cover `phlo limit × phlo price` plus
  the amount being transferred
- `Transfer complete.` is printed only after the vault reports the transfer accepted on the deploy's `deployId` channel
- Vault addresses must start with `1111`

## Exact amounts

`--amount` accepts an unsigned decimal integer in raw base units, up to
`2^256 - 1`. With `--whole-tokens`, it accepts a decimal token amount and uses
the node's reported precision. For example, on an 18-decimal node,
`--amount 1.000000000000000001 --whole-tokens` sends
`1000000000000000001` raw units. Excess decimal places and overflow are errors;
amounts are never rounded.

The client keeps ordinary Rholang integer literals for amounts through
`2^63 - 1` and uses BigInt literals above that range. Large transfers require
a node whose vault supports BigInt amounts. Node balance and stake limits
still apply.
