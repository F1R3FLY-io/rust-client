# propose

Manually propose a block. Typically not needed — on shards with heartbeat enabled, blocks are auto-proposed.

## Usage

```bash
node_cli propose [OPTIONS]
```

## Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--grpc` | | `http://localhost:40401` | The node's **internal** gRPC endpoint, `http://` or `https://` |
| `--ca-cert` | | none | PEM file of the certificate authorities trusted for `https://` endpoints, in place of the bundled public roots |

Propose is served on a node's internal gRPC API (port 40402 on a node; 40412 for validator1 on the system-integration Docker shard), not the external API on 40401 that deploys use, so `--grpc` must name the internal port:

```bash
node_cli propose --grpc http://localhost:40402
```

## When to use

- Standalone nodes without heartbeat
- Testing scenarios where you need explicit block creation
- Use `deploy-and-wait --propose` instead if you want deploy + propose + wait

## NoNewDeploys

On shards with heartbeat, `propose` often returns:

```
Proposal failed: NoNewDeploys. No unprocessed deploys in pool.
If you just deployed, the deploy may have already been included by the auto-proposer.
```

This is expected — the heartbeat already proposed a block containing your deploy. It's not an error.
