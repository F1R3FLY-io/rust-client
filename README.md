# F1r3fly Node CLI

A command-line interface for interacting with F1r3fly nodes.

### Prerequisites

- [Running Node](https://github.com/F1R3FLY-io/f1r3fly/tree/rust/dev?tab=readme-ov-file#running)

> **Note:** The commands in this CLI work out of the box with the Docker setup found in the [f1r3node Docker README](https://github.com/F1R3FLY-io/f1r3node/blob/main/docker/README.md). The default ports and configuration align with the standard F1r3fly Docker deployment.

## Building

```bash
cargo build
```

## Usage

The CLI provides the following commands for interacting with F1r3fly nodes:

### Deploy

Deploy Rholang code to a F1r3fly node.

```bash
# Using default values (localhost:40412 with default private key)
cargo run -- deploy -f ./rho_examples/stdout.rho

# With custom parameters
cargo run -- deploy -f ./rho_examples/stdout.rho --private-key YOUR_PRIVATE_KEY -H node.example.com -p 40412

# With bigger phlo limit
cargo run -- deploy -f ./rho_examples/stdout.rho -b
```

### Propose

Propose a block to the F1r3fly network.

```bash
# Using default values
cargo run -- propose

# With custom parameters
cargo run -- propose --private-key YOUR_PRIVATE_KEY -H node.example.com -p 40412
```

### Full Deploy

Deploy Rholang code and propose a block in one operation.

```bash
# Using default values
cargo run -- full-deploy -f ./rho_examples/stdout.rho

# With custom parameters
cargo run -- full-deploy -f ./rho_examples/stdout.rho --private-key YOUR_PRIVATE_KEY -H node.example.com -p 40412

# With bigger phlo limit
cargo run -- full-deploy -f ./rho_examples/stdout.rho -b
```

### Deploy and Wait

The `deploy-and-wait` command deploys Rholang code and waits for the transaction to be included in a block and finalized.

```bash
# Using default values
cargo run -- deploy-and-wait -f ./rho_examples/stdout.rho

# With custom parameters
cargo run -- deploy-and-wait -f ./rho_examples/stdout.rho --max-wait 600 --check-interval 10
```

### Get Deploy Information

The `get-deploy` command retrieves comprehensive information about a deploy by its ID.

```bash
# Basic usage
cargo run -- get-deploy -d "3045022100abc..."

# Different output formats
cargo run -- get-deploy -d "3045022100abc..." --format summary
cargo run -- get-deploy -d "3045022100abc..." --format json
cargo run -- get-deploy -d "3045022100abc..." --format pretty --verbose

# Custom node connection
cargo run -- get-deploy -d "3045022100abc..." -H validator2.local --http-port 40423
```

Available output formats:
- `pretty` (default): Human-readable formatted output with emojis
- `summary`: One-line status summary
- `json`: Raw JSON response for scripting

### Is Finalized

Check if a block is finalized, with automatic retries.

```bash
# Using default values (retry every 5 seconds, up to 12 times)
cargo run -- is-finalized -b BLOCK_HASH

# With custom parameters
cargo run -- is-finalized -b BLOCK_HASH --private-key YOUR_PRIVATE_KEY -H node.example.com -p 40412

# With custom retry settings
cargo run -- is-finalized -b BLOCK_HASH -m 20 -r 3  # Retry every 3 seconds, up to 20 times
```

### Exploratory Deploy

Execute Rholang code without committing it to the blockchain. This is useful for read-only operations or when working with nodes in read-only mode.

```bash
# Using default values (latest state)
cargo run -- exploratory-deploy -f ./rho_examples/stdout.rho

# With custom parameters
cargo run -- exploratory-deploy -f ./rho_examples/stdout.rho --private-key YOUR_PRIVATE_KEY -H node.example.com -p 40412

# Execute at a specific block
cargo run -- exploratory-deploy -f ./rho_examples/stdout.rho --block-hash BLOCK_HASH

# Using pre-state hash instead of post-state hash
cargo run -- exploratory-deploy -f ./rho_examples/stdout.rho --block-hash BLOCK_HASH --use-pre-state
```

### Generate Public Key

Generate a public key from a given private key.

```bash
# Using default private key
cargo run -- generate-public-key

# Provide your own private key
cargo run -- generate-public-key --private-key YOUR_PRIVATE_KEY

# Generate compressed format public key
cargo run -- generate-public-key --private-key YOUR_PRIVATE_KEY --compressed
```

### Generate Key Pair

Generate a new secp256k1 private/public key pair.

```bash
# Generate a new key pair and display on screen
cargo run -- generate-key-pair

# Generate a key pair with compressed public key
cargo run -- generate-key-pair --compressed

# Generate a key pair and save to files
cargo run -- generate-key-pair --save

# Save to a specific directory
cargo run -- generate-key-pair --save --output-dir /path/to/keys
```

### Generate REV Address

Generate a REV address from a public key. You can either provide a public key directly or use a private key (from which the public key will be derived).

```bash
# Using default private key
cargo run -- generate-rev-address

# Provide your own private key
cargo run -- generate-rev-address --private-key YOUR_PRIVATE_KEY

# Provide a public key directly
cargo run -- generate-rev-address --public-key YOUR_PUBLIC_KEY
```

### Get Node ID

Extract the F1R3FLY node ID from a TLS private key file. The node ID is a 40-character hexadecimal string derived from the Keccak-256 hash of the TLS public key (removing the '04' prefix).

```bash
# Extract node ID from TLS key file (hex format)
cargo run -- get-node-id --key-file /path/to/node.key.pem

# Extract node ID and generate RNode URL format
cargo run -- get-node-id --key-file /path/to/node.key.pem --format rnode-url

# Generate RNode URL with custom host and ports
cargo run -- get-node-id --key-file /path/to/node.key.pem --format rnode-url --host mynode.com --protocol-port 40400 --discovery-port 40404
```

**Output formats:**
- `hex` (default): Returns just the 40-character node ID
- `rnode-url`: Returns both the node ID and a complete RNode URL for network connections

### Watch Blocks

Monitor real-time block events from a F1r3fly node via WebSocket. This command connects to the node's `/ws/events` endpoint and streams block creation, validation, and finalization events with detailed information including deploy IDs. Automatically reconnects on disconnect (10 retries every 10 seconds by default).

```bash
# Watch all block events (created, added, and finalized)
cargo run -- watch-blocks

# Watch from remote node
cargo run -- watch-blocks -H node.example.com --http-port 40403

# Filter to show only created blocks
cargo run -- watch-blocks --filter created

# Filter to show only added blocks (validated and added to DAG)
cargo run -- watch-blocks --filter added

# Filter to show only finalized blocks
cargo run -- watch-blocks --filter finalized

# Retry reconnection forever until manually killed (Ctrl+C)
cargo run -- watch-blocks --retry-forever
```

**Features:**
- Human-readable formatted output with emojis
- Shows block hash, creator, sequence number, parents count, and deploy IDs for created/added blocks
- Shows block hash for finalized blocks
- Automatic reconnection on disconnect (10 attempts by default, or infinite with `--retry-forever`)
- 10 second intervals between reconnection attempts
- Real-time event statistics on exit (counts created, added, and finalized events)

**Event Types:**
- `created` - New block proposed by a validator (includes all block details and deploy IDs)
- `added` - Block validated and added to the DAG (includes all block details and deploy IDs)
- `finalized` - Block reached finalized status (shows block hash)

**Options:**
- `-H, --host <HOST>` - Node hostname (default: localhost)
- `--http-port <PORT>` - HTTP port for WebSocket (default: 40403)
- `-f, --filter <TYPE>` - Show only specific event type (created/added/finalized)
- `--retry-forever` - Keep trying to reconnect indefinitely until manually killed (Ctrl+C)

### Transfer REV

Transfer REV tokens between addresses. The command automatically derives the sender address from the private key and deploys a transfer contract.

```bash
# Basic transfer (requires manual block proposal)
cargo run -- transfer --to-address "111127RX5ZgiAdRaQy4AWy57RdvAAckdELReEBxzvWYVvdnR32PiHA" --amount 100

# Transfer with custom private key
cargo run -- transfer --to-address "111127RX5ZgiAdRaQy4AWy57RdvAAckdELReEBxzvWYVvdnR32PiHA" --amount 100 --private-key YOUR_PRIVATE_KEY

# Transfer and auto-propose a block
cargo run -- transfer --to-address "111127RX5ZgiAdRaQy4AWy57RdvAAckdELReEBxzvWYVvdnR32PiHA" --amount 100 --propose true

# Transfer with standard phlo limit (not recommended - may run out of gas)
cargo run -- transfer --to-address "111127RX5ZgiAdRaQy4AWy57RdvAAckdELReEBxzvWYVvdnR32PiHA" --amount 100 --bigger-phlo false

# Transfer to custom node
cargo run -- transfer --to-address "111127RX5ZgiAdRaQy4AWy57RdvAAckdELReEBxzvWYVvdnR32PiHA" --amount 100 -H node.example.com -p 40412
```

**Note**: The transfer command uses a high phlo limit by default (`--bigger-phlo true`) because transfer contracts require more computational resources than simple deployments. This helps prevent "out of phlogistons" errors.
## Node Inspection Commands

The CLI provides several commands for inspecting and monitoring F1r3fly nodes using HTTP endpoints:

### Status

Get node status and peer information.

```bash
# Get status from default node (localhost:40413)
cargo run -- status

# Get status from custom node
cargo run -- status -H node.example.com -p 40413
```

### Blocks

Get recent blocks or specific block information.

```bash
# Get 5 recent blocks (default)
cargo run -- blocks

# Get 10 recent blocks
cargo run -- blocks -n 10

# Get specific block by hash
cargo run -- blocks --block-hash BLOCK_HASH_HERE

# Get blocks from custom node
cargo run -- blocks -H node.example.com -p 40413 -n 3
```

### Bonds

Get current validator bonds from the PoS contract.

```bash
# Get validator bonds (uses HTTP port for explore-deploy endpoint)
cargo run -- bonds

# Get bonds from custom node
cargo run -- bonds -H node.example.com -p 40453
```

### Active Validators

Get active validators from the PoS contract.

```bash
# Get active validators (uses HTTP port for explore-deploy endpoint)
cargo run -- active-validators

# Get active validators from custom node
cargo run -- active-validators -H node.example.com -p 40413
```

### Wallet Balance

Check wallet balance for a specific address.

```bash
# Check wallet balance for an address (requires read-only node on port 40452)
cargo run -- wallet-balance --address "1111AtahZeefej4tvVR6ti9TJtv8yxLebT31SCEVDCKMNikBk5r3g"

# Check balance from custom node (uses gRPC, requires read-only node)
cargo run -- wallet-balance -a "1111AtahZeefej4tvVR6ti9TJtv8yxLebT31SCEVDCKMNikBk5r3g" -H node.example.com -p 40452
```

### Bond Status

Check if a validator is bonded by public key.

```bash
# Check bond status for a public key
cargo run -- bond-status --public-key "04ffc016579a68050d655d55df4e09f04605164543e257c8e6df10361e6068a5336588e9b355ea859c5ab4285a5ef0efdf62bc28b80320ce99e26bb1607b3ad93d"

# Check from custom node (uses HTTP port like other inspection commands)
cargo run -- bond-status -k "PUBLIC_KEY_HERE" -H node.example.com -p 40413
```

### Metrics

Get node metrics for monitoring.

```bash
# Get node metrics (filtered to show key metrics)
cargo run -- metrics

# Get metrics from custom node
cargo run -- metrics -H node.example.com -p 40413
```

### Last Finalized Block

Get the last finalized block from the node.

```bash
# Get last finalized block from default node (localhost:40413)
cargo run -- last-finalized-block

# Get last finalized block from custom node
cargo run -- last-finalized-block -H node.example.com -p 40413
```

### Show Main Chain

Get blocks from the main chain in sequential order.

```bash
# Get last 10 blocks from main chain (default depth)
cargo run -- show-main-chain

# Get specific number of blocks from main chain
cargo run -- show-main-chain -d 5

# Get main chain blocks from custom node
cargo run -- show-main-chain -H node.example.com -p 40412 -d 20

# Use custom private key for authentication
cargo run -- show-main-chain --private-key YOUR_PRIVATE_KEY
```

### Get Blocks by Height

Get blocks within a specific height range from the blockchain.

```bash
# Get blocks between height 100 and 105 (inclusive)
cargo run -- get-blocks-by-height -s 100 -e 105

# Get blocks from custom node
cargo run -- get-blocks-by-height -s 50 -e 75 -H node.example.com -p 40412

# Use custom private key for authentication
cargo run -- get-blocks-by-height -s 1 -e 10 --private-key YOUR_PRIVATE_KEY

# Get a single block by height (start and end the same)
cargo run -- get-blocks-by-height -s 42 -e 42
```

**Parameters:**
- `-s, --start-block-number`: Start block number (inclusive)
- `-e, --end-block-number`: End block number (inclusive)
- `-H, --host`: Node hostname (default: localhost)
- `-p, --port`: gRPC port (default: 40412)
- `--private-key`: Private key for gRPC authentication

**Note:** The command validates that start block number ≤ end block number and both are non-negative.

## Dynamic Validator Addition Commands

The CLI provides commands for dynamically adding validators to a running F1r3fly network, based on the procedures outlined in the `add-validator-dynamically.md` guide.

### Bond Validator

Deploy a bonding transaction to add a new validator to the network. The command waits for the deploy to be included in a block and finalized, similar to `deploy-and-wait`. **Requires specifying which validator to bond via private key.**

```bash
# Bond Validator_4 node as validator (1000 REV stake)
cargo run -- bond-validator --stake 1000 --private-key 5ff3514bf79a7d18e8dd974c699678ba63b7762ce8d78c532346e52f0ad219cd --port 40411

# Deploy bonding transaction and propose block immediately  
cargo run -- bond-validator --stake 1000 --private-key 5ff3514bf79a7d18e8dd974c699678ba63b7762ce8d78c532346e52f0ad219cd --propose true --port 40411

# Deploy bonding transaction and wait for finalization (with custom timeouts)
cargo run -- bond-validator --stake 1000 --private-key 5ff3514bf79a7d18e8dd974c699678ba63b7762ce8d78c532346e52f0ad219cd --max-wait 600 --check-interval 10 --port 40411

# Bond validator on custom node
cargo run -- bond-validator --stake 1000 --private-key YOUR_VALIDATOR_PRIVATE_KEY -H node.example.com -p 40411
```

### Transfer

Transfer REV tokens between addresses. The command waits for the deploy to be included in a block and finalized, providing full confirmation of the transfer.

```bash
# Transfer 1000 REV from bootstrap wallet to another address
cargo run -- transfer --to-address 1111La6tHaCtGjRiv4wkffbTAAjGyMsVhzSUNzQxH1jjZH9jtEi3M --amount 1000 --port 40411

# Transfer with custom private key (different sender)
cargo run -- transfer --to-address 1111La6tHaCtGjRiv4wkffbTAAjGyMsVhzSUNzQxH1jjZH9jtEi3M --amount 500 --private-key 5ff3514bf79a7d18e8dd974c699678ba63b7762ce8d78c532346e52f0ad219cd --port 40411

# Transfer with auto-propose enabled
cargo run -- transfer --to-address 1111La6tHaCtGjRiv4wkffbTAAjGyMsVhzSUNzQxH1jjZH9jtEi3M --amount 1000 --propose true --port 40411

# Transfer with custom wait settings
cargo run -- transfer --to-address 1111La6tHaCtGjRiv4wkffbTAAjGyMsVhzSUNzQxH1jjZH9jtEi3M --amount 1000 --max-wait 600 --check-interval 10 --port 40411

# Transfer using custom node connection
cargo run -- transfer --to-address RECIPIENT_ADDRESS --amount 1000 -H node.example.com -p 40411
```

### Network Health

Check the health and connectivity of multiple nodes in your F1r3fly shard.

**Local Development (Single Host):**
```bash
# Check standard F1r3fly shard ports (bootstrap, validator1, validator2, observer)
cargo run -- network-health

# Check localhost with explicit host flag (same as above)
cargo run -- network-health -H localhost

# Check network health with custom additional ports (e.g., after adding validator3)
cargo run -- network-health --custom-ports "60503"

# Check only custom ports (disable standard ports)
cargo run -- network-health --standard-ports false --custom-ports "60503,70503"
```

**Multi-Host / Remote Networks:**
```bash
# For remote hosts, you MUST specify --custom-ports (no standard port assumptions)
cargo run -- network-health -H testnet.example.com --custom-ports "8001,8002,9443"

# Single remote node
cargo run -- network-health -H validator.net --custom-ports "7890"

# Different hosts require separate commands
cargo run -- network-health -H host1.com --custom-ports "8001"
cargo run -- network-health -H host2.com --custom-ports "8002" 
cargo run -- network-health -H host3.com --custom-ports "9443"
```

**Note:** Remote hosts don't use standard F1r3fly ports (40403, 40413, etc.). You must explicitly specify the actual ports in use with `--custom-ports` to avoid connection failures.

### Epoch Info

Get current epoch information including epoch length, quarantine length, and transition timing.

```bash
# Get epoch information (uses default observer port 40452)
cargo run -- epoch-info

# Get epoch info from custom node
cargo run -- epoch-info -H node.example.com -p 40452
```

### Validator Status

Check the detailed status of a specific validator (bonded, active, or quarantine state).

```bash
# Check validator4 status (replace with actual public key)
cargo run -- validator-status -k 04d26c6103d7269773b943d7a9c456f9eb227e0d8b1fe30bccee4fca963f4446e3385d99f6386317f2c1ad36b9e6b0d5f97bb0a0041f05781c60a5ebca124a251d

# Check validator status on custom node
cargo run -- validator-status -k YOUR_VALIDATOR_PUBLIC_KEY -H node.example.com -p 40452
```

### Epoch Rewards

Get current epoch rewards information from the PoS contract.

```bash
# Get epoch rewards (uses default observer port 40452)
cargo run -- epoch-rewards

# Get epoch rewards from custom node
cargo run -- epoch-rewards -H node.example.com -p 40452
```

### Network Consensus

Get network-wide consensus health overview including validator participation rates.

```bash
# Get network consensus overview (uses default observer port 40452)
cargo run -- network-consensus

# Get consensus overview from custom node
cargo run -- network-consensus -H node.example.com -p 40452
```

## Command Line Options

### Deploy and Full-Deploy Commands

- `-f, --file <FILE>`: Path to the Rholang file to deploy (required)
- `--private-key <PRIVATE_KEY>`: Private key in hex format
- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: gRPC port number (default: 40412)
- `-b, --bigger-phlo`: Use bigger phlo limit

### Deploy-and-Wait Command

- `-f, --file <FILE>`: Path to the Rholang file to deploy (required)
- `--private-key <PRIVATE_KEY>`: Private key in hex format
- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: gRPC port number for deploy (default: 40412)
- `--http-port <HTTP_PORT>`: HTTP port number for deploy status checks (default: 40413)
- `-b, --bigger-phlo`: Use bigger phlo limit
- `--max-wait <MAX_WAIT>`: Maximum total wait time in seconds (default: 300)
- `--check-interval <CHECK_INTERVAL>`: Check interval in seconds (default: 5)

### Get-Deploy Command

- `-d, --deploy-id <DEPLOY_ID>`: Deploy ID to retrieve (required)
- `-H, --host <HOST>`: Host address (default: "localhost")
- `--http-port <HTTP_PORT>`: HTTP port number for API queries (default: 40413)
- `-f, --format <FORMAT>`: Output format: "pretty" (default), "summary", or "json"
- `--verbose`: Show additional deploy details (signature, etc.)

### Propose Command

- `--private-key <PRIVATE_KEY>`: Private key in hex format
- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: gRPC port number (default: 40412)

### Is-Finalized Command

- `-b, --block-hash <BLOCK_HASH>`: Block hash to check (required)
- `--private-key <PRIVATE_KEY>`: Private key in hex format
- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: gRPC port number (default: 40412)
- `-m, --max-attempts <MAX_ATTEMPTS>`: Maximum number of retry attempts (default: 12)
- `-r, --retry-delay <RETRY_DELAY>`: Delay between retries in seconds (default: 5)

### Exploratory-Deploy Command

- `-f, --file <FILE>`: Path to the Rholang file to execute (required)
- `--private-key <PRIVATE_KEY>`: Private key in hex format
- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: gRPC port number (default: 40412)
- `-b, --block-hash <BLOCK_HASH>`: Optional block hash to use as reference
- `-u, --use-pre-state`: Use pre-state hash instead of post-state hash

### Generate-Public-Key Command

- `--private-key <PRIVATE_KEY>`: Private key in hex format
- `-c, --compressed`: Output public key in compressed format (shorter)

### Generate-Key-Pair Command

- `-c, --compressed`: Output public key in compressed format (shorter)
- `-s, --save`: Save keys to files instead of displaying them
- `-o, --output-dir <DIR>`: Output directory for saved keys (default: current directory)

### Get-Node-ID Command

- `-k, --key-file <KEY_FILE>`: Path to the TLS private key file (node.key.pem) (required)
- `-f, --format <FORMAT>`: Output format: "hex" (default) or "rnode-url"
- `-H, --host <HOST>`: Node hostname for rnode-url format (default: "localhost")
- `--protocol-port <PROTOCOL_PORT>`: Protocol port for rnode-url format (default: 40400)
- `--discovery-port <DISCOVERY_PORT>`: Discovery port for rnode-url format (default: 40404)

### Status Command

- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: HTTP port number (default: 40413)

### Blocks Command

- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: HTTP port number (default: 40413)
- `-n, --number <NUMBER>`: Number of recent blocks to fetch (default: 5)
- `-b, --block-hash <BLOCK_HASH>`: Specific block hash to fetch (optional)

### Bonds Command

- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: HTTP port number (default: 40413)

### Active-Validators Command

- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: HTTP port number (default: 40413)

### Wallet-Balance Command

- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: gRPC port number (default: 40452, requires read-only node)
- `-a, --address <ADDRESS>`: Wallet address to check balance for (required)

### Bond-Status Command

- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: HTTP port number (default: 40413)
- `-k, --public-key <PUBLIC_KEY>`: Public key to check bond status for (required)

### Metrics Command

- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: HTTP port number (default: 40413)

### Last-Finalized-Block Command

- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: HTTP port number (default: 40413)

### Show-Main-Chain Command

- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: gRPC port number (default: 40412)
- `-d, --depth <DEPTH>`: Number of blocks to fetch from main chain (default: 10)
- `--private-key <PRIVATE_KEY>`: Private key in hex format (required for gRPC)

### Get-Blocks-By-Height Command

- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: gRPC port number (default: 40412)
- `-s, --start-block-number <START>`: Start block number (inclusive)
- `-e, --end-block-number <END>`: End block number (inclusive)
- `--private-key <PRIVATE_KEY>`: Private key in hex format (required for gRPC)

### Bond-Validator Command

- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: gRPC port number for deploy (default: 40412)
- `--http-port <HTTP_PORT>`: HTTP port number for deploy status checks (default: 40413)
- `-s, --stake <STAKE>`: Stake amount for the validator (required)
- `--private-key <PRIVATE_KEY>`: Private key for signing the deploy - determines which validator gets bonded (required)
- `--propose <PROPOSE>`: Also propose a block after bonding (default: false)
- `--max-wait <MAX_WAIT>`: Maximum total wait time in seconds for deploy finalization (default: 300)
- `--check-interval <CHECK_INTERVAL>`: Check interval in seconds for deploy status (default: 5)

### Network-Health Command

- `-H, --host <HOST>`: Host address (default: "localhost")
- `-s, --standard-ports <STANDARD_PORTS>`: Check standard F1r3fly shard ports (default: true)
- `-c, --custom-ports <CUSTOM_PORTS>`: Additional custom ports to check (comma-separated)

### Transfer Command

- `-t, --to-address <TO_ADDRESS>`: Recipient REV address (required)
- `-a, --amount <AMOUNT>`: Amount in REV to transfer (required)
- `--private-key <PRIVATE_KEY>`: Private key for signing the transfer (hex format)
- `-H, --host <HOST>`: Host address (default: "localhost")
- `-p, --port <PORT>`: gRPC port number for deploy (default: 40412)
- `--http-port <HTTP_PORT>`: HTTP port number for deploy status checks (default: 40413)
- `-b, --bigger-phlo`: Use bigger phlo limit (default: true, recommended for transfers)
- `--propose <PROPOSE>`: Also propose a block after transfer (default: false)
- `--max-wait <MAX_WAIT>`: Maximum total wait time in seconds for deploy finalization (default: 300)
- `--check-interval <CHECK_INTERVAL>`: Check interval in seconds for deploy status (default: 5)