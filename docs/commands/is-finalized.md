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
| `--grpc` | | `http://localhost:40401` | Node gRPC endpoint, `http://` or `https://`, with no path |
| `--ca-cert` | | none | PEM file of the certificate authorities trusted for `https://` endpoints, in place of the bundled public roots; refused with an `http://` endpoint |
| `--connect-timeout` | | `10` | Seconds a connection to a node may take to establish (at least 1) |
| `--timeout` | | `30` | Seconds a single request to a node may take (at least 1) |
| `--max-attempts` | `-m` | `12` | Max retry attempts |
| `--retry-delay` | `-r` | `5` | Seconds between retries |

## Example

```
$ node_cli is-finalized -b 0519f656624c26e8a406ed4fd7f1fa9327f48128a0ad7758289bc09b8f646419

Connecting to F1r3fly node at http://localhost:40401
Checking if block is finalized: 0519f656...
Will retry every 5 seconds, up to 12 times
Block is finalized!
Time taken: 8.79ms
```
