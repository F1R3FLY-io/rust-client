# estimate-cost

Estimate the phlogiston (gas) cost of Rholang code without deploying it.

Runs an exploratory deploy under the hood and returns only the cost. The code executes against the last finalized block's state and is rolled back — nothing is persisted.

## Usage

```bash
node_cli estimate-cost -f <FILE> [-H HOST] [-p PORT]
```

## Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--file` | `-f` | required | Rholang file to estimate |
| `--private-key` | `-k` | dev key | Signing key |
| `--host` | `-H` | `localhost` | Node hostname |
| `--port` | `-p` | `40412` | gRPC port |
| `--block-hash` | | latest | Estimate against a specific block's state |
| `--use-pre-state` | | false | Use pre-state hash |

Must run against an observer/read-only node.

## Example

```
$ node_cli estimate-cost -f contract.rho -H localhost -p 40452

317
```

Output is just the cost number — easy to parse in scripts:

```bash
COST=$(node_cli estimate-cost -f contract.rho -p 40452)
echo "Estimated cost: $COST phlogiston"

if [ "$COST" -gt 50000 ]; then
    echo "Using bigger phlo limit"
    node_cli deploy-and-wait -f contract.rho --bigger-phlo
else
    node_cli deploy-and-wait -f contract.rho
fi
```

## Library Usage

```rust
let manager = F1r3flyConnectionManager::new(config);
let cost = manager.estimate_cost("new x in { x!(1 + 1) }").await?;
println!("Estimated cost: {} phlogiston", cost);
```

## Notes

- Cost is the phlogiston consumed by the interpreter during execution
- The estimate may differ slightly from the actual deploy cost because:
  - Exploratory deploy uses a synthetic deployer key (not yours)
  - Timestamp and valid_after_block_number differ
  - State may change between estimate and actual deploy
- For most contracts, the difference is negligible
- Cost of 0 means the code didn't execute (parse error or empty)
