# get-data

Read data from a deploy's `deployId` channel at a specific block. Uses the `getDataAtName` gRPC endpoint.

## Usage

```bash
node_cli get-data --deploy-id <ID> --block-hash <HASH> [OPTIONS]
```

## Flags

| Flag | Short | Default | Description |
|------|-------|---------|-------------|
| `--deploy-id` | `-d` | required | Deploy ID (hex) |
| `--block-hash` | `-b` | required | Block hash containing the deploy |
| `--private-key` | `-k` | dev key | Signing key |
| `--host` | `-H` | `localhost` | Node hostname |
| `--port` | `-p` | `40412` | gRPC port |

## Example

```
$ node_cli get-data \
    -d 3044022075e51b8f4a4b873344e276336c77ce9b91672bec10c8a327b6151142fd727b85022064eb25090cfd5135fdc316b77dbe0bd717ed5402bc30e8068bc9a5a0b4055e0d \
    -b 0519f656624c26e8a406ed4fd7f1fa9327f48128a0ad7758289bc09b8f646419

42
```

## When to use

- After `deploy` (without waiting) to manually read the result later
- To re-read a deploy's result from a known block
- For debugging: verify what a contract wrote to `deployId`

## Notes

- Data is only available if the contract wrote to `deployId` (e.g., `deployId!(42)`)
- The block must be finalized for the data to be reliable
- If no data exists, prints: `No data found for deploy <ID> at block <HASH>`
- `deploy-and-wait` reads data automatically — this command is for manual/scripted use
