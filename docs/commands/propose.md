# propose

Manually propose a block. Typically not needed — on shards with heartbeat enabled, blocks are auto-proposed.

## Usage

```bash
node_cli propose [OPTIONS]
```

## Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--private-key` | `-k` | dev key | Signing key |
| `--host` | `-H` | `localhost` | Node hostname |
| `--port` | `-p` | `40412` | gRPC port |

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
