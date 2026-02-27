#!/bin/bash
# stress_deploy.sh - Sustained deploy load generator for memory profiling
#
# Sends fire-and-forget deploys at a configurable interval.
# Designed to run alongside Grafana/Prometheus/cAdvisor to observe
# memory, CPU, and GC behavior of a Scala light shard under load.
#
# Usage:
#   ./scripts/stress_deploy.sh [profile] [options]
#
# Profiles:
#   light    - 1 deploy every 30s (baseline)
#   medium   - 1 deploy every 20s
#   heavy    - 1 deploy every 10s
#   burst    - 1 deploy every 2s  (stress test)
#
# Options (override profile defaults):
#   --interval N       seconds between deploys
#   --rho FILE         path to .rho file (default: rho_examples/stdout.rho)
#   --host HOST        node host (default: localhost)
#   --port PORT        gRPC port (default: 40412)
#   --duration MINS    stop after N minutes (default: unlimited)
#   --max-deploys N    stop after N deploys (default: unlimited)
#   --bigger-phlo      use bigger phlo limit
#
# Examples:
#
#   Default profiles (light shard ports 40412):
#     ./scripts/stress_deploy.sh light                  # 1 deploy every 30s, runs until Ctrl+C
#     ./scripts/stress_deploy.sh medium                 # 1 deploy every 20s
#     ./scripts/stress_deploy.sh heavy                  # 1 deploy every 10s
#     ./scripts/stress_deploy.sh burst                  # 1 deploy every 2s (stress)
#
#   With time limit:
#     ./scripts/stress_deploy.sh light --duration 60    # light load for 1 hour
#     ./scripts/stress_deploy.sh heavy --duration 30    # heavy load for 30 minutes
#     ./scripts/stress_deploy.sh burst --duration 5     # burst for 5 minutes
#
#   With deploy count limit:
#     ./scripts/stress_deploy.sh light --max-deploys 100   # exactly 100 deploys at 30s interval
#     ./scripts/stress_deploy.sh burst --max-deploys 50    # 50 burst deploys
#
#   Custom interval:
#     ./scripts/stress_deploy.sh --interval 45                        # every 45s, unlimited
#     ./scripts/stress_deploy.sh --interval 5 --duration 20           # every 5s for 20 minutes
#     ./scripts/stress_deploy.sh --interval 15 --max-deploys 200      # every 15s, 200 deploys
#
#   Custom rholang file:
#     ./scripts/stress_deploy.sh heavy --rho rho_examples/test_revvault.rho
#     ./scripts/stress_deploy.sh --interval 10 --rho ./my_contract.rho --duration 60
#
#   Custom host/port:
#     ./scripts/stress_deploy.sh heavy --port 40402                   # standalone bootstrap
#     ./scripts/stress_deploy.sh medium --host 192.168.1.100 --port 40412  # remote node

set -euo pipefail

# --- Defaults ---
INTERVAL=10
RHO_FILE="rho_examples/stdout.rho"
HOST="localhost"
GRPC_PORT=40412
DURATION=0        # 0 = unlimited
MAX_DEPLOYS=0     # 0 = unlimited
BIGGER_PHLO=""
PRIVATE_KEY="5f668a7ee96d944a4494cc947e4005e172d7ab3461ee5538f1f2a45a835e9657"

# --- Parse profile (first positional arg) ---
PROFILE=""
if [[ "${1:-}" =~ ^(light|medium|heavy|burst)$ ]]; then
    PROFILE="$1"
    shift
fi

case "${PROFILE}" in
    light)  INTERVAL=30 ;;
    medium) INTERVAL=20 ;;
    heavy)  INTERVAL=10 ;;
    burst)  INTERVAL=2  ;;
esac

# --- Parse named options ---
while [[ $# -gt 0 ]]; do
    case "$1" in
        --interval)    INTERVAL="$2";    shift 2 ;;
        --rho)         RHO_FILE="$2";    shift 2 ;;
        --host)        HOST="$2";        shift 2 ;;
        --port)        GRPC_PORT="$2";   shift 2 ;;
        --duration)    DURATION="$2";    shift 2 ;;
        --max-deploys) MAX_DEPLOYS="$2"; shift 2 ;;
        --bigger-phlo) BIGGER_PHLO="-b"; shift ;;
        --private-key) PRIVATE_KEY="$2"; shift 2 ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

# --- Change to project root ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

# --- Validate rho file ---
if [[ ! -f "$RHO_FILE" ]]; then
    echo "ERROR: Rholang file not found: $RHO_FILE" >&2
    exit 1
fi

# --- Build release binary ---
echo "Building rust-client (release)..."
cargo build --release 2>&1 | tail -1
echo ""

# --- Setup CSV log ---
mkdir -p logs
CSV_FILE="logs/stress_$(date +%Y%m%d_%H%M%S).csv"
echo "seq,timestamp,epoch_ms,interval_s,rho_file,deploy_id,duration_ms,status" > "$CSV_FILE"

# --- Colors ---
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
DIM='\033[2m'
NC='\033[0m'

# --- Print config ---
echo "========================================"
echo -e "${BLUE}Stress Deploy - Memory Profiling${NC}"
echo "========================================"
echo "Profile:     ${PROFILE:-custom}"
echo "Interval:    ${INTERVAL}s"
echo "Rho file:    $RHO_FILE"
echo "Target:      $HOST:$GRPC_PORT"
echo "Duration:    $([ "$DURATION" -gt 0 ] && echo "${DURATION} min" || echo "unlimited (Ctrl+C to stop)")"
echo "Max deploys: $([ "$MAX_DEPLOYS" -gt 0 ] && echo "$MAX_DEPLOYS" || echo "unlimited")"
echo "CSV log:     $CSV_FILE"
echo "========================================"
echo ""
echo -e "${DIM}Tip: Open Grafana and watch container_memory_usage_bytes${NC}"
echo -e "${DIM}     CSV timestamps are epoch_ms for easy Grafana annotation${NC}"
echo ""

# --- Compute end time ---
if [[ "$DURATION" -gt 0 ]]; then
    END_TIME=$(( $(date +%s) + DURATION * 60 ))
else
    END_TIME=0
fi

# --- Counters ---
SEQ=0
OK=0
FAIL=0
START_TIME=$(date +%s)

# --- Cleanup on exit ---
print_summary() {
    local elapsed=$(( $(date +%s) - START_TIME ))
    echo ""
    echo "========================================"
    echo "STRESS TEST SUMMARY"
    echo "========================================"
    echo -e "Total deploys:  $SEQ"
    echo -e "Successful:     ${GREEN}$OK${NC}"
    echo -e "Failed:         ${RED}$FAIL${NC}"
    echo -e "Duration:       ${elapsed}s"
    if [[ $SEQ -gt 0 ]]; then
        echo -e "Success rate:   $(( OK * 100 / SEQ ))%"
        echo -e "Avg interval:   $(( elapsed / SEQ ))s"
    fi
    echo "CSV log:        $CSV_FILE"
    echo "========================================"
}
trap print_summary EXIT

# --- Main loop ---
while true; do
    # Check duration limit
    if [[ "$END_TIME" -gt 0 ]] && [[ $(date +%s) -ge "$END_TIME" ]]; then
        echo ""
        echo -e "${YELLOW}Duration limit reached (${DURATION} min). Stopping.${NC}"
        break
    fi

    # Check deploy count limit
    if [[ "$MAX_DEPLOYS" -gt 0 ]] && [[ "$SEQ" -ge "$MAX_DEPLOYS" ]]; then
        echo ""
        echo -e "${YELLOW}Max deploys reached ($MAX_DEPLOYS). Stopping.${NC}"
        break
    fi

    SEQ=$((SEQ + 1))
    TIMESTAMP=$(date '+%Y-%m-%d %H:%M:%S')
    EPOCH_MS=$(date +%s%3N 2>/dev/null || echo $(( $(date +%s) * 1000 )))

    echo -n -e "${DIM}[${TIMESTAMP}]${NC} Deploy #${SEQ}... "

    # Generate unique rholang code per deploy (avoids duplicate deploy conflicts)
    UNIQUE_RHO=$(mktemp /tmp/stress_deploy_XXXXXX.rho)
    cat "$RHO_FILE" > "$UNIQUE_RHO"
    # Append unique marker so each deploy has different content/signature
    echo "" >> "$UNIQUE_RHO"
    echo "// stress_deploy seq=${SEQ} ts=${EPOCH_MS}" >> "$UNIQUE_RHO"

    # Time the deploy
    DEPLOY_START=$(date +%s.%N)
    OUTPUT=$(mktemp)

    if cargo run -q --release -- deploy \
        -f "$UNIQUE_RHO" \
        -H "$HOST" \
        -p "$GRPC_PORT" \
        --private-key "$PRIVATE_KEY" \
        $BIGGER_PHLO \
        > "$OUTPUT" 2>&1; then

        DEPLOY_END=$(date +%s.%N)
        DURATION_MS=$(echo "($DEPLOY_END - $DEPLOY_START) * 1000" | bc | cut -d. -f1)

        # Extract deploy ID
        DEPLOY_ID=$(grep -oE 'Deploy ID: [a-f0-9]+' "$OUTPUT" | head -1 | cut -d' ' -f3 || echo "unknown")

        echo -e "${GREEN}OK${NC} ${DIM}(${DURATION_MS}ms) deploy=${DEPLOY_ID:0:16}...${NC}"
        echo "${SEQ},${TIMESTAMP},${EPOCH_MS},${INTERVAL},${RHO_FILE},${DEPLOY_ID},${DURATION_MS},ok" >> "$CSV_FILE"
        OK=$((OK + 1))
    else
        DEPLOY_END=$(date +%s.%N)
        DURATION_MS=$(echo "($DEPLOY_END - $DEPLOY_START) * 1000" | bc | cut -d. -f1)

        ERROR=$(head -1 "$OUTPUT" | cut -c1-80)
        echo -e "${RED}FAIL${NC} ${DIM}(${DURATION_MS}ms) ${ERROR}${NC}"
        echo "${SEQ},${TIMESTAMP},${EPOCH_MS},${INTERVAL},${RHO_FILE},,${DURATION_MS},fail" >> "$CSV_FILE"
        FAIL=$((FAIL + 1))
    fi

    rm -f "$OUTPUT" "$UNIQUE_RHO"

    # Sleep until next deploy (unless we've hit limits)
    if [[ "$MAX_DEPLOYS" -gt 0 ]] && [[ "$SEQ" -ge "$MAX_DEPLOYS" ]]; then
        continue
    fi
    sleep "$INTERVAL"
done
