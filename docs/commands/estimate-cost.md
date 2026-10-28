# estimate-cost

Estimate the phlogiston (gas) cost of Rholang code without deploying it.

Hits the node's `POST /api/estimate-cost` HTTP endpoint. The code executes against the last finalized block's state and is rolled back — nothing is persisted.

## Usage

```bash
node_cli estimate-cost -f <FILE> [--http URL] [--deployer PUBKEY]
```

## Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--file` | `-f` | required | Rholang file to estimate |
| `--deployer` | | (none) | Hex-encoded 65-byte uncompressed secp256k1 public key (`04`-prefixed) |
| `--private-key` | | (env) | Hex-encoded private key; derives `--deployer` locally (never leaves the machine). Mutually exclusive with `--deployer`. |
| `--http` | | `http://localhost:40403` | Node HTTP endpoint, `http://` or `https://` |
| `--ca-cert` | | none | PEM file of the certificate authorities trusted for `https://` endpoints, in place of the bundled public roots |
| `--block-hash` | | latest | Estimate against a specific block's state |

Runs read-only by default. **Always pass `--deployer` (or `--private-key`) for identity-dependent contracts** such as vault transfers — without it the estimate may be significantly lower than the real deploy cost because the node executes under an empty deployer identity.

## Example

```
$ node_cli estimate-cost -f contract.rho --http http://localhost:40453 --deployer 04aabb...

317
```

Output is just the cost number — easy to parse in scripts:

```bash
COST=$(node_cli estimate-cost -f contract.rho --http http://localhost:40453 --deployer "$PUBKEY")
echo "Estimated cost: $COST phlogiston"

# Leave headroom: the estimate runs against the current state, not the state
# the deploy will execute against.
node_cli deploy-and-wait -f contract.rho --phlo-limit $((COST * 2))
```

## Library Usage

```rust
let manager = F1r3flyConnectionManager::new(config)?;
let cost = manager.estimate_cost("new x in { x!(1 + 1) }", Some(&deployer_pubkey_hex)).await?;
println!("Estimated cost: {} phlogiston", cost);
```

## Notes

- Cost is the phlogiston consumed by the interpreter during execution
- **Identity-dependent terms** (e.g. vault transfers) cost different amounts depending on the deployer's public key. Supply `--deployer` to get an accurate estimate.
- The estimate may still differ slightly from the actual deploy cost because:
  - Timestamp and valid_after_block_number differ
  - State may change between estimate and actual deploy
- **Old nodes:** nodes older than the cost-fix (PR #134) silently ignore the `deployer` field in the request body (unknown JSON fields are not rejected). Against such a node the returned value is still an underestimate for identity-dependent terms, and the client cannot detect this from the response.
- Cost of 0 means the code didn't execute (parse error or empty)
