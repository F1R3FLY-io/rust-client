# is-finalized

Check if a block is finalized, with automatic retries.

## Usage

```bash
node_cli is-finalized -b <BLOCK_HASH> [OPTIONS]
```

## Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--block-hash` | `-b` | required | Block hash to check |
| `--private-key` | `-k` | dev key | Signing key |
| `--host` | `-H` | `localhost` | Node hostname |
| `--port` | `-p` | `40412` | gRPC port |
| `--max-attempts` | `-m` | `12` | Max retry attempts |
| `--retry-delay` | `-r` | `5` | Seconds between retries |

## Example

```
$ node_cli is-finalized -b 0519f656624c26e8a406ed4fd7f1fa9327f48128a0ad7758289bc09b8f646419

Connecting to F1r3fly node at localhost:40412
Checking if block is finalized: 0519f656...
Will retry every 5 seconds, up to 12 times
Block is finalized!
Time taken: 8.79ms
```
