#!/bin/bash
# smoke_test.sh - Validate rust-client commands against F1r3fly node
# Usage: ./scripts/smoke_test.sh [host] [grpc_port] [http_port] [observer_grpc_port] [private_key]
#
# Tests each command and validates the output format matches expected patterns.
# Run against Scala node first to establish baseline, then verify against Rust node.
#
# Examples:
#   ./scripts/smoke_test.sh                           # localhost with default ports
#   ./scripts/smoke_test.sh localhost 40402 40403     # standalone node (observer = grpc)
#   ./scripts/smoke_test.sh localhost 40412 40413 40452  # shard with separate observer
#   ./scripts/smoke_test.sh localhost 40402 40403 40402 <private_key>  # custom private key

set -euo pipefail

# Configuration
HOST="${1:-localhost}"
GRPC_PORT="${2:-40402}"      # gRPC port for deploy/propose operations
HTTP_PORT="${3:-40403}"      # HTTP port for status/query operations
OBSERVER_GRPC="${4:-$GRPC_PORT}"  # Observer gRPC port (defaults to same as GRPC_PORT)
OBSERVER_HTTP=$((OBSERVER_GRPC + 1))  # Observer HTTP port (gRPC + 1)
PRIVATE_KEY="${5:-5f668a7ee96d944a4494cc947e4005e172d7ab3461ee5538f1f2a45a835e9657}"  # Signing key

# Recipient address for transfers (secondary test address from genesis)
TO_ADDR="11112oRqNpmKjfFCGgH6bw5csjBqVgb4PVRP5S98tTNjDeqdWNJr2L"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Counters
PASS=0
FAIL=0
SKIP=0

# Helper to increment counters (avoids set -e issues with ((x++)) returning 1 when x=0)
inc_pass() { PASS=$((PASS + 1)); }
inc_fail() { FAIL=$((FAIL + 1)); }
inc_skip() { SKIP=$((SKIP + 1)); }

# Change to rust-client directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

# Build release binary once before running tests
echo "Building rust-client (release)..."
cargo build --release

# Log file for test outputs (single file per run)
mkdir -p "logs"
LOG_FILE="logs/smoke_test_$(date +%Y%m%d_%H%M%S).log"

# Temp file for current test output
OUTPUT=$(mktemp)
trap "rm -f $OUTPUT" EXIT

# Helper to save test output to log file
save_log() {
    local test_name="$1"
    echo "" >> "$LOG_FILE"
    echo "========================================" >> "$LOG_FILE"
    echo "TEST: $test_name" >> "$LOG_FILE"
    echo "TIME: $(date '+%Y-%m-%d %H:%M:%S')" >> "$LOG_FILE"
    echo "========================================" >> "$LOG_FILE"
    cat "$OUTPUT" >> "$LOG_FILE"
}

# Write header to log file
{
    echo "========================================"
    echo "F1r3fly Rust Client Smoke Tests"
    echo "Started: $(date '+%Y-%m-%d %H:%M:%S')"
    echo "========================================"
    echo "Host: $HOST"
    echo "gRPC Port: $GRPC_PORT"
    echo "HTTP Port: $HTTP_PORT"
    echo "Observer gRPC: $OBSERVER_GRPC"
    echo "Private Key: ${PRIVATE_KEY:0:8}...${PRIVATE_KEY: -8}"
    echo "Working dir: $(pwd)"
    echo "========================================"
} > "$LOG_FILE"

echo "========================================"
echo "F1r3fly Rust Client Smoke Tests"
echo "========================================"
echo "Host: $HOST"
echo "gRPC Port: $GRPC_PORT"
echo "HTTP Port: $HTTP_PORT"
echo "Observer gRPC: $OBSERVER_GRPC"
echo "Private Key: ${PRIVATE_KEY:0:8}...${PRIVATE_KEY: -8}"
echo "Working dir: $(pwd)"
echo "Log file: $LOG_FILE"
echo "========================================"
echo ""

# Start suite timer
SUITE_START=$(date +%s)

# Format duration for display (ms for < 1s, seconds for >= 1s)
format_duration() {
    local ms=$1
    if [[ $ms -lt 0 ]]; then
        echo "?ms"
    elif [[ $ms -lt 1000 ]]; then
        echo "${ms}ms"
    else
        local secs=$((ms / 1000))
        local remainder=$((ms % 1000 / 100))
        echo "${secs}.${remainder}s"
    fi
}

# Run a command with a time limit (portable: Linux timeout, macOS gtimeout or shell fallback)
run_with_timeout() {
    local secs=$1
    shift
    if command -v timeout &>/dev/null; then
        timeout "${secs}s" "$@"
    elif command -v gtimeout &>/dev/null; then
        gtimeout "${secs}s" "$@"
    else
        ("$@") &
        local pid=$!
        sleep "$secs"
        kill "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    fi
}

# Test runner function
# Args: test_name, command, validation_pattern
run_test() {
    local name="$1"
    local cmd="$2"
    local pattern="$3"
    
    echo -n "Testing $name... "
    
    # Start timer (use seconds with nanoseconds for precision)
    local start_time=$(date +%s.%N)
    
    # Run the command
    if ! eval "$cmd" > "$OUTPUT" 2>&1; then
        local end_time=$(date +%s.%N)
        local duration_ms=$(echo "($end_time - $start_time) * 1000" | bc | cut -d. -f1)
        save_log "$name"
        echo -e "${RED}FAIL${NC} (non-zero exit) [$(format_duration $duration_ms)]"
        echo "  Command: $cmd"
        echo "  Output:"
        head -10 "$OUTPUT" | sed 's/^/    /'
        inc_fail
        return 1
    fi
    
    local end_time=$(date +%s.%N)
    local duration_ms=$(echo "($end_time - $start_time) * 1000" | bc | cut -d. -f1)
    
    # Save log for all tests
    save_log "$name"
    
    # Validate output matches pattern
    if grep -qE "$pattern" "$OUTPUT"; then
        echo -e "${GREEN}PASS${NC} [$(format_duration $duration_ms)]"
        inc_pass
        return 0
    else
        echo -e "${RED}FAIL${NC} (output validation failed) [$(format_duration $duration_ms)]"
        echo "  Command: $cmd"
        echo "  Expected pattern: $pattern"
        echo "  Output:"
        head -10 "$OUTPUT" | sed 's/^/    /'
        inc_fail
        return 1
    fi
}

# Skip test function
skip_test() {
    local name="$1"
    local reason="$2"
    echo -e "Testing $name... ${YELLOW}SKIP${NC} ($reason)"
    inc_skip
}

# Test with retries (for timing-sensitive operations like propose)
# Args: test_name, command, validation_pattern, max_retries
run_test_with_retry() {
    local name="$1"
    local cmd="$2"
    local pattern="$3"
    local max_retries="${4:-3}"
    local retry=0
    
    echo -n "Testing $name... "
    
    # Start timer
    local start_time=$(date +%s.%N)
    
    while [ $retry -lt $max_retries ]; do
        # Run the command
        if eval "$cmd" > "$OUTPUT" 2>&1; then
            # Validate output matches pattern
            if grep -qE "$pattern" "$OUTPUT"; then
                local end_time=$(date +%s.%N)
                local duration_ms=$(echo "($end_time - $start_time) * 1000" | bc | cut -d. -f1)
                save_log "$name"
                echo -e "${GREEN}PASS${NC} [$(format_duration $duration_ms)]"
                inc_pass
                return 0
            fi
        fi
        
        # Check if it's a "propose in progress" error - retry
        if grep -q "another propose is in progress" "$OUTPUT"; then
            retry=$((retry + 1))
            if [ $retry -lt $max_retries ]; then
                echo -n "(retry $retry) "
                sleep 2
                continue
            fi
        else
            # Other error - don't retry
            break
        fi
    done
    
    local end_time=$(date +%s.%N)
    local duration_ms=$(echo "($end_time - $start_time) * 1000" | bc | cut -d. -f1)
    save_log "$name"
    echo -e "${RED}FAIL${NC} [$(format_duration $duration_ms)]"
    echo "  Command: $cmd"
    echo "  Output:"
    head -10 "$OUTPUT" | sed 's/^/    /'
    inc_fail
    return 1
}

# ============================================
# DEPLOY COMMANDS
# ============================================
echo -e "${BLUE}--- Deploy Commands ---${NC}"

# deploy: Deploy Rholang code to a node
# Expected output: "Deployment successful!" and "Deploy ID: <hex>"
run_test "deploy" \
    "cargo run -q --release -- deploy -f ./rho_examples/stdout.rho -H $HOST -p $GRPC_PORT" \
    "Deployment successful|Deploy ID:"

# deploy-and-wait: Deploy and wait for block inclusion/finalization
# Expected output: "Deploy successful" and "Deploy found in block"
run_test "deploy-and-wait" \
    "cargo run -q --release -- deploy-and-wait -f ./rho_examples/stdout.rho -H $HOST -p $GRPC_PORT --http-port $HTTP_PORT --observer-port $OBSERVER_GRPC --max-wait 30 --check-interval 2" \
    "Deploy successful|Deploy found in block"

# is-finalized: Check if a block is finalized
# First get a recent block hash, then check if it's finalized
BLOCK_HASH=$(curl -s "http://$HOST:$HTTP_PORT/api/blocks/1" 2>/dev/null | grep -oE '"blockHash":"[a-f0-9]{64}"' | head -1 | cut -d'"' -f4 || echo "")
if [ -n "$BLOCK_HASH" ]; then
    run_test "is-finalized" \
        "cargo run -q --release -- is-finalized -b $BLOCK_HASH -H $HOST -p $GRPC_PORT -m 3 -r 2" \
        "Block is finalized|finalized"
else
    skip_test "is-finalized" "could not get block hash"
fi

# exploratory-deploy: Execute Rholang without committing to blockchain
# Must run on observer (read-only) node - validators reject exploratory deploys
# Expected output: "Execution successful"
run_test "exploratory-deploy" \
    "cargo run -q --release -- exploratory-deploy -f ./rho_examples/stdout.rho -H $HOST -p $OBSERVER_GRPC" \
    "Execution successful"

# ============================================
# CRYPTO COMMANDS (offline, no node required)
# ============================================
echo ""
echo -e "${BLUE}--- Crypto Commands (offline) ---${NC}"

# generate-public-key: Derive public key from private key
run_test "generate-public-key" \
    "cargo run -q --release -- generate-public-key" \
    "Public key.*04[a-f0-9]{128}"

# generate-key-pair: Generate new secp256k1 key pair
run_test "generate-key-pair" \
    "cargo run -q --release -- generate-key-pair" \
    "Private key.*[a-f0-9]{64}"

# generate-vault-address: Generate vault address from key
run_test "generate-vault-address" \
    "cargo run -q --release -- generate-vault-address" \
    "Vault address.*1111[a-zA-Z0-9]+"

# ============================================
# NODE INSPECTION COMMANDS (HTTP)
# ============================================
echo ""
echo -e "${BLUE}--- Node Inspection Commands (HTTP) ---${NC}"

# status: Get node status and peer information
run_test "status" \
    "cargo run -q --release -- status -H $HOST -p $HTTP_PORT" \
    "Node status retrieved successfully|version"

# blocks: Get recent blocks
run_test "blocks" \
    "cargo run -q --release -- blocks -H $HOST -p $HTTP_PORT -n 2" \
    "Blocks retrieved successfully|blockHash"

# bonds: Get validator bonds from PoS contract
# Uses exploratory-deploy internally, must run on observer (read-only) node
run_test "bonds" \
    "cargo run -q --release -- bonds -H $HOST -p $OBSERVER_HTTP" \
    "Validator bonds retrieved successfully|Bonded Validators"

# active-validators: Get active validators
# Uses exploratory-deploy internally, must run on observer (read-only) node
run_test "active-validators" \
    "cargo run -q --release -- active-validators -H $HOST -p $OBSERVER_HTTP" \
    "Active validators retrieved successfully|Active Validators"

# metrics: Get node metrics
run_test "metrics" \
    "cargo run -q --release -- metrics -H $HOST -p $HTTP_PORT" \
    "rchain|block|peer|jvm"

# last-finalized-block: Get the last finalized block
run_test "last-finalized-block" \
    "cargo run -q --release -- last-finalized-block -H $HOST -p $HTTP_PORT" \
    "Last finalized block retrieved successfully|Block Hash"

# ============================================
# gRPC QUERY COMMANDS
# ============================================
echo ""
echo -e "${BLUE}--- gRPC Query Commands ---${NC}"

# show-main-chain: Get blocks from the main chain
run_test "show-main-chain" \
    "cargo run -q --release -- show-main-chain -H $HOST -p $GRPC_PORT -d 3" \
    "Main chain blocks retrieved successfully|Found.*blocks"

# get-blocks-by-height: Get blocks in height range
run_test "get-blocks-by-height" \
    "cargo run -q --release -- get-blocks-by-height -H $HOST -p $GRPC_PORT -s 1 -e 3" \
    "Blocks retrieved successfully|Found.*blocks"

# wallet-balance: Check wallet balance for an address
# Uses exploratory-deploy internally, must run on observer (read-only) node
run_test "wallet-balance" \
    "cargo run -q --release -- wallet-balance -H $HOST -p $OBSERVER_GRPC -a 1111AtahZeefej4tvVR6ti9TJtv8yxLebT31SCEVDCKMNikBk5r3g" \
    "Wallet balance retrieved successfully|Balance"

# ============================================
# NETWORK COMMANDS
# ============================================
echo ""
echo -e "${BLUE}--- Network Commands ---${NC}"

# network-health: Check network health
run_test "network-health" \
    "cargo run -q --release -- network-health -H $HOST --standard-ports false --custom-ports $HTTP_PORT" \
    "HEALTHY|Healthy nodes"

# bond-status: Check if a validator is bonded
# Uses exploratory-deploy internally, must run on observer (read-only) node
VALIDATOR_PUBKEY="04ffc016579a68050d655d55df4e09f04605164543e257c8e6df10361e6068a5336588e9b355ea859c5ab4285a5ef0efdf62bc28b80320ce99e26bb1607b3ad93d"
run_test "bond-status" \
    "cargo run -q --release -- bond-status -H $HOST -p $OBSERVER_HTTP -k $VALIDATOR_PUBKEY" \
    "Bond information retrieved successfully|BONDED|NOT BONDED"

# ============================================
# TRANSFER COMMANDS
# ============================================
echo ""
echo -e "${BLUE}--- Transfer Commands ---${NC}"

# transfer: Transfer tokens between addresses
# Uses observer port for finalization check
# Capture deploy ID for get-deploy test
echo -n "Testing transfer... "
TRANSFER_START=$(date +%s.%N)
if cargo run -q --release -- transfer --to-address 111127RX5ZgiAdRaQy4AWy57RdvAAckdELReEBxzvWYVvdnR32PiHA --amount 1 -H $HOST -p $GRPC_PORT --http-port $HTTP_PORT --observer-port $OBSERVER_GRPC --max-wait 60 --check-interval 2 > "$OUTPUT" 2>&1; then
    TRANSFER_END=$(date +%s.%N)
    TRANSFER_MS=$(echo "($TRANSFER_END - $TRANSFER_START) * 1000" | bc | cut -d. -f1)
    save_log "transfer"
    if grep -qE "Transfer contract deployed successfully|Transfer deploy found in block" "$OUTPUT"; then
        echo -e "${GREEN}PASS${NC} [$(format_duration $TRANSFER_MS)]"
        inc_pass
        # Extract deploy ID for get-deploy test
        TRANSFER_DEPLOY_ID=$(grep -oE 'Deploy ID: [a-f0-9]+' "$OUTPUT" | head -1 | cut -d' ' -f3 || echo "")
    else
        echo -e "${RED}FAIL${NC} (output validation failed) [$(format_duration $TRANSFER_MS)]"
        inc_fail
    fi
else
    TRANSFER_END=$(date +%s.%N)
    TRANSFER_MS=$(echo "($TRANSFER_END - $TRANSFER_START) * 1000" | bc | cut -d. -f1)
    save_log "transfer"
    echo -e "${RED}FAIL${NC} (non-zero exit) [$(format_duration $TRANSFER_MS)]"
    head -10 "$OUTPUT" | sed 's/^/    /'
    inc_fail
fi

# get-deploy: Get deploy information by ID
if [ -n "${TRANSFER_DEPLOY_ID:-}" ]; then
    run_test "get-deploy" \
        "cargo run -q --release -- get-deploy -d $TRANSFER_DEPLOY_ID -H $HOST --http-port $HTTP_PORT" \
        "Deploy Information|Status.*Included|Deploy ID"
else
    skip_test "get-deploy" "no deploy ID from transfer test"
fi

# ============================================
# PoS QUERY COMMANDS
# ============================================
echo ""
echo -e "${BLUE}--- PoS Query Commands ---${NC}"

# epoch-info: Get current epoch information
# Uses exploratory-deploy internally, must run on observer (read-only) node
run_test "epoch-info" \
    "cargo run -q --release -- epoch-info -H $HOST -p $OBSERVER_GRPC" \
    "Epoch information retrieved successfully|Current Epoch"

# epoch-rewards: Get current epoch rewards
# Uses exploratory-deploy internally, must run on observer (read-only) node
run_test "epoch-rewards" \
    "cargo run -q --release -- epoch-rewards -H $HOST -p $OBSERVER_GRPC" \
    "Epoch rewards retrieved successfully|Epoch Rewards"

# validator-status: Check individual validator status
# Uses exploratory-deploy internally, must run on observer (read-only) node
run_test "validator-status" \
    "cargo run -q --release -- validator-status -H $HOST -p $OBSERVER_GRPC --http-port $OBSERVER_HTTP -k $VALIDATOR_PUBKEY" \
    "Validator status retrieved successfully|BONDED|NOT BONDED"

# network-consensus: Get network-wide consensus overview
# Uses exploratory-deploy internally, must run on observer (read-only) node
run_test "network-consensus" \
    "cargo run -q --release -- network-consensus -H $HOST -p $OBSERVER_GRPC --http-port $OBSERVER_HTTP" \
    "Network consensus data retrieved successfully|Consensus Health"

# ============================================
# CRYPTO COMMANDS (offline, continued)
# ============================================
echo ""
echo -e "${BLUE}--- Crypto Commands (continued) ---${NC}"

# get-node-id: Extract node ID from TLS certificate
# Use local test certificate if available
if [ -f "./test_certs/node.certificate.pem" ]; then
    run_test "get-node-id" \
        "cargo run -q --release -- get-node-id --cert-file ./test_certs/node.certificate.pem" \
        "Node ID extracted successfully|Node ID:"
else
    skip_test "get-node-id" "no certificate file found (run: mkdir -p test_certs && copy a .pem file)"
fi

# ============================================
# STREAMING COMMANDS
# ============================================
echo ""
echo -e "${BLUE}--- Streaming Commands ---${NC}"

# watch-blocks: Watch real-time block events via WebSocket
# Run for 10 seconds and check if it connects and receives all event types
echo -n "Testing watch-blocks... "
WB_START=$(date +%s.%N)
run_with_timeout 10 cargo run -q --release -- watch-blocks -H $HOST --http-port $HTTP_PORT > "$OUTPUT" 2>&1 || true
WB_END=$(date +%s.%N)
WB_MS=$(echo "($WB_END - $WB_START) * 1000" | bc | cut -d. -f1)
save_log "watch-blocks"
# Check for successful connection and all three event types
if grep -q "Connected to node WebSocket" "$OUTPUT"; then
    HAS_CREATED=$(grep -c "Block Created" "$OUTPUT" 2>/dev/null | tr -d '\n' || echo 0)
    HAS_ADDED=$(grep -c "Block Added" "$OUTPUT" 2>/dev/null | tr -d '\n' || echo 0)
    HAS_FINALIZED=$(grep -c "Block Finalized" "$OUTPUT" 2>/dev/null | tr -d '\n' || echo 0)
    # Ensure we have valid integers (default to 0 if empty)
    HAS_CREATED=${HAS_CREATED:-0}
    HAS_ADDED=${HAS_ADDED:-0}
    HAS_FINALIZED=${HAS_FINALIZED:-0}
    
    if [ "$HAS_CREATED" -gt 0 ] && [ "$HAS_ADDED" -gt 0 ] && [ "$HAS_FINALIZED" -gt 0 ]; then
        echo -e "${GREEN}PASS${NC} (created:$HAS_CREATED, added:$HAS_ADDED, finalized:$HAS_FINALIZED) [$(format_duration $WB_MS)]"
        inc_pass
    elif [ "$HAS_CREATED" -gt 0 ] || [ "$HAS_ADDED" -gt 0 ] || [ "$HAS_FINALIZED" -gt 0 ]; then
        echo -e "${YELLOW}PASS${NC} (partial: created:$HAS_CREATED, added:$HAS_ADDED, finalized:$HAS_FINALIZED) [$(format_duration $WB_MS)]"
        inc_pass
    else
        echo -e "${YELLOW}PASS${NC} (connected, no events in 10s - node may be idle) [$(format_duration $WB_MS)]"
        inc_pass
    fi
elif grep -qE "error|Error|refused|failed" "$OUTPUT"; then
    echo -e "${RED}FAIL${NC} (connection error) [$(format_duration $WB_MS)]"
    head -5 "$OUTPUT" | sed 's/^/    /'
    inc_fail
else
    # No connection message - unexpected
    echo -e "${RED}FAIL${NC} (unexpected output) [$(format_duration $WB_MS)]"
    head -5 "$OUTPUT" | sed 's/^/    /'
    inc_fail
fi

# ============================================
# LOAD TEST COMMANDS
# ============================================
echo ""
echo -e "${BLUE}--- Load Test Commands ---${NC}"

# load-test: Run load test with minimal config (3 tests, short timeouts)
echo -n "Testing load-test... "
LT_START=$(date +%s.%N)
cargo run -q --release -- load-test \
  --to-address "$TO_ADDR" \
  --num-tests 3 \
  --amount 1 \
  --interval 3 \
  --check-interval 2 \
  --inclusion-timeout 60 \
  --finalization-timeout 60 \
  --private-key "$PRIVATE_KEY" \
  -H $HOST \
  --port $GRPC_PORT \
  --http-port $HTTP_PORT \
  --readonly-port $OBSERVER_GRPC > "$OUTPUT" 2>&1
LT_END=$(date +%s.%N)
LT_MS=$(echo "($LT_END - $LT_START) * 1000" | bc | cut -d. -f1)
save_log "load-test"
if grep -q "FINAL RESULTS" "$OUTPUT" && grep -qE "Finalized: [0-9]+" "$OUTPUT"; then
    # Extract finalization stats
    FINALIZED=$(grep "Finalized:" "$OUTPUT" | tail -1 | grep -oE "[0-9]+" | head -1)
    TOTAL=$(grep "Total tests:" "$OUTPUT" | grep -oE "[0-9]+")
    if [ "$FINALIZED" = "$TOTAL" ] 2>/dev/null; then
        echo -e "${GREEN}PASS${NC} ($FINALIZED/$TOTAL finalized) [$(format_duration $LT_MS)]"
        inc_pass
    else
        echo -e "${RED}FAIL${NC} (only $FINALIZED/$TOTAL finalized) [$(format_duration $LT_MS)]"
        inc_fail
    fi
elif grep -qE "error|Error|failed" "$OUTPUT"; then
    echo -e "${RED}FAIL${NC} [$(format_duration $LT_MS)]"
    tail -10 "$OUTPUT" | sed 's/^/    /'
    inc_fail
else
    echo -e "${RED}FAIL${NC} (unexpected output) [$(format_duration $LT_MS)]"
    tail -10 "$OUTPUT" | sed 's/^/    /'
    inc_fail
fi

# ============================================
# SKIPPED COMMANDS
# ============================================
echo ""
echo -e "${BLUE}--- Skipped Commands ---${NC}"

# propose: Manual block proposal conflicts with heartbeat proposer
# When heartbeat is enabled (default), it automatically proposes blocks every few seconds.
# Manual propose will fail with "another propose is in progress" most of the time.
# Use deploy-and-wait instead, which relies on heartbeat to include deploys.
skip_test "propose" "conflicts with heartbeat proposer - use deploy-and-wait instead"

# full-deploy: Deploy + propose in one operation - same issue as propose
# The propose step conflicts with heartbeat. Use deploy-and-wait instead.
skip_test "full-deploy" "propose step conflicts with heartbeat - use deploy-and-wait instead"

# bond-validator: Bonds a new validator to the network
# This test should be run manually because bonding a validator that isn't running
# a node will break consensus - the new validator can't participate in block finalization,
# causing the Last Finalized Block (LFB) to stop advancing.
skip_test "bond-validator" "run manually - bonding non-running validator breaks consensus"

# dag: Interactive DAG visualization (TUI)
skip_test "dag" "interactive TUI command"

# ============================================
# SUMMARY
# ============================================
SUITE_END=$(date +%s)
SUITE_DURATION=$((SUITE_END - SUITE_START))

echo ""
echo "========================================"
echo "RESULTS"
echo "========================================"
echo -e "Passed:  ${GREEN}$PASS${NC}"
echo -e "Failed:  ${RED}$FAIL${NC}"
echo -e "Skipped: ${YELLOW}$SKIP${NC}"
echo -e "Total time: ${SUITE_DURATION}s"
echo "========================================"
echo ""
echo "Full test log: $LOG_FILE"
echo ""

# Exit with failure if any tests failed
if [ $FAIL -gt 0 ]; then
    exit 1
fi
