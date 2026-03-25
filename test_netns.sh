#!/bin/bash
# Re-UDP Part 2: Network Emulation Experiment Runner
# Runs baseline, delay, and loss scenarios using network namespaces and tc netem.
# Captures server logs, measures transfer time, counts retransmissions, computes
# throughput, and generates experiment_report.md.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY_DIR="$SCRIPT_DIR/target/debug"
FILES_DIR="$SCRIPT_DIR/files"
FILE=test_file.txt
SEG=256
SERVER_IP=10.0.0.1
SERVER_PORT=9090
CLIENT_PORT=8080
FILE_SIZE=10201  # bytes; test_file.txt is exactly 10201 bytes
REPORT="$SCRIPT_DIR/experiment_report.md"
LOG_DIR=/tmp/re-udp-logs
mkdir -p "$LOG_DIR"

# Associative arrays to hold per-scenario results
declare -A RESULT_STATUS RESULT_RETRANSMIT_TIMEOUT RESULT_RETRANSMIT_DUPAK \
           RESULT_RETRANSMIT_TOTAL RESULT_TIME_MS RESULT_THROUGHPUT RESULT_ANOMALY \
           RESULT_FINAL_AVG_RTT RESULT_FINAL_TIMEOUT

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------
echo "[setup] Building project..."
cd "$SCRIPT_DIR"
cargo build 2>&1 | tail -5
echo "[setup] Build complete."

# ---------------------------------------------------------------------------
# Namespace guard
# ---------------------------------------------------------------------------
if ! ip netns list | grep -q ns_server; then
    echo "[setup] Creating network namespaces..."
    bash "$SCRIPT_DIR/netns_setup.sh"
else
    echo "[setup] Namespaces already exist, skipping setup."
fi

# ---------------------------------------------------------------------------
# Cleanup any lingering server/client processes from previous runs
# ---------------------------------------------------------------------------
echo "[setup] Cleaning up any lingering processes..."
pkill -f "target/debug/server" 2>/dev/null || true
pkill -f "target/debug/client" 2>/dev/null || true
sleep 0.3

# ---------------------------------------------------------------------------
# netem helpers
# ---------------------------------------------------------------------------
apply_netem() {
    ip netns exec "$1" tc qdisc replace dev "$2" root netem "${@:3}"
}

teardown_netem() {
    apply_netem ns_client veth_client delay 0ms loss 0%
    apply_netem ns_server veth_server delay 0ms loss 0%
}

# ---------------------------------------------------------------------------
# run_scenario <label>
# ---------------------------------------------------------------------------
run_scenario() {
    local label=$1
    local safe_label
    safe_label="$(echo "$label" | tr ' /()+' '_____' | tr -s '_' | tr '[:upper:]' '[:lower:]')"
    local server_log="$LOG_DIR/${safe_label}_server.log"
    local client_log="$LOG_DIR/${safe_label}_client.log"

    echo ""
    echo "=== $label ==="

    rm -f "$SCRIPT_DIR/$FILE"

    # Start server in background, capture output
    ip netns exec ns_server "$BINARY_DIR/server" \
        --port "$SERVER_PORT" --file-directory "$FILES_DIR" \
        >"$server_log" 2>&1 &
    local server_pid=$!
    sleep 0.4

    # Time the client using date +%s%N (nanoseconds) — avoids TIMEFORMAT subshell issues
    local t_start t_end elapsed_ms
    t_start=$(date +%s%N)
    ip netns exec ns_client "$BINARY_DIR/client" \
        --port "$CLIENT_PORT" \
        --ip-addr "$SERVER_IP" \
        --server-port "$SERVER_PORT" \
        --file-name "$FILE" \
        --segment-size "$SEG" \
        >"$client_log" 2>&1
    local client_exit=$?
    t_end=$(date +%s%N)
    elapsed_ms=$(( (t_end - t_start) / 1000000 ))

    kill "$server_pid" 2>/dev/null
    wait "$server_pid" 2>/dev/null

    # ---- Integrity check ----
    local status
    if [ "$client_exit" -eq 0 ] && \
       diff -q "$SCRIPT_DIR/$FILE" "$FILES_DIR/$FILE" >/dev/null 2>&1; then
        status="PASS"
    else
        status="FAIL"
    fi
    rm -f "$SCRIPT_DIR/$FILE"

    # ---- Retransmission counts (-a: binary-safe) ----
    local retx_timeout retx_dupak retx_total
    retx_timeout=$(grep -ac "\[server\] Timeout -- retransmitting" "$server_log" 2>/dev/null) || retx_timeout=0
    retx_dupak=$(grep  -ac "\[server\] Duplicate ACK seq="         "$server_log" 2>/dev/null) || retx_dupak=0
    retx_total=$(( retx_timeout + retx_dupak ))

    # ---- Throughput (KB/s) ----
    local throughput
    if [ "$status" = "PASS" ] && [ "$elapsed_ms" -gt 0 ]; then
        throughput=$(echo "scale=2; $FILE_SIZE * 1000 / $elapsed_ms / 1024" | bc)
    else
        throughput="N/A"
    fi

    # ---- Anomaly detection ----
    local anomaly="none"
    if grep -q "Max retransmits" "$server_log" 2>/dev/null; then
        anomaly="server hit MAX_RETRANSMITS, dropped connection"
    fi
    if grep -q "No response from server" "$client_log" 2>/dev/null; then
        [ "$anomaly" = "none" ] \
            && anomaly="client exhausted MAX_REQUEST_RETRIES" \
            || anomaly="$anomaly; client exhausted MAX_REQUEST_RETRIES"
    fi

    # ---- Final adaptive-timeout values from server log ----
    # -a: treat as text (in case binary output from panic sneaks in)
    # "ACK seq=" lines contain rtt/avg_rtt/timeout; pick the last one
    local final_avg_rtt final_timeout
    final_avg_rtt=$(grep -a "ACK seq=.*avg_rtt=" "$server_log" 2>/dev/null | tail -1 \
        | grep -oP 'avg_rtt=\K[0-9.]+' 2>/dev/null) || true
    [ -z "$final_avg_rtt" ] && final_avg_rtt="N/A"
    final_timeout=$(grep -a "ACK seq=.*timeout=" "$server_log" 2>/dev/null | tail -1 \
        | grep -oP 'timeout=\K[0-9]+' 2>/dev/null) || true
    [ -z "$final_timeout" ] && final_timeout="N/A"

    # ---- Store results ----
    RESULT_STATUS["$label"]="$status"
    RESULT_RETRANSMIT_TIMEOUT["$label"]="$retx_timeout"
    RESULT_RETRANSMIT_DUPAK["$label"]="$retx_dupak"
    RESULT_RETRANSMIT_TOTAL["$label"]="$retx_total"
    RESULT_TIME_MS["$label"]="$elapsed_ms"
    RESULT_THROUGHPUT["$label"]="$throughput"
    RESULT_ANOMALY["$label"]="$anomaly"
    RESULT_FINAL_AVG_RTT["$label"]="$final_avg_rtt"
    RESULT_FINAL_TIMEOUT["$label"]="$final_timeout"

    # ---- Terminal summary ----
    echo "  Status:                $status"
    echo "  Transfer time:         ${elapsed_ms} ms"
    echo "  Throughput:            ${throughput} KB/s"
    echo "  Timeout retransmits:   $retx_timeout"
    echo "  DupACK retransmits:    $retx_dupak"
    echo "  Total retransmits:     $retx_total"
    echo "  Final avg RTT:         ${final_avg_rtt} ms"
    echo "  Final timeout:         ${final_timeout} ms"
    echo "  Anomalies:             $anomaly"
    echo "  Server log:            $server_log"
    echo "  Client log:            $client_log"
}

# ---------------------------------------------------------------------------
# Scenario execution
# ---------------------------------------------------------------------------

# Scenario 0: Baseline
teardown_netem
run_scenario "Baseline (no impairment)"

# Scenario 1: Symmetric 100ms delay +/-10ms jitter (normal distribution)
teardown_netem
apply_netem ns_client veth_client delay 100ms 10ms distribution normal
apply_netem ns_server veth_server delay 100ms 10ms distribution normal
run_scenario "Delay 100ms 10ms symmetric"

# Scenario 2: Symmetric 10% packet loss
teardown_netem
apply_netem ns_client veth_client loss 10%
apply_netem ns_server veth_server loss 10%
run_scenario "Loss 10pct symmetric"

# Final cleanup
teardown_netem

echo ""
echo "=== All scenarios complete ==="

# ---------------------------------------------------------------------------
# Report generation
# ---------------------------------------------------------------------------
SCENARIOS=(
    "Baseline (no impairment)"
    "Delay 100ms 10ms symmetric"
    "Loss 10pct symmetric"
)

# Pretty label for display in the report
display_label() {
    case "$1" in
        "Baseline (no impairment)")       echo "Baseline (no impairment)" ;;
        "Delay 100ms 10ms symmetric")     echo "Delay 100ms ±10ms (symmetric)" ;;
        "Loss 10pct symmetric")           echo "Loss 10% (symmetric)" ;;
        *)                                echo "$1" ;;
    esac
}

netem_rule() {
    case "$1" in
        "Baseline (no impairment)")       echo "\`delay 0ms loss 0%\` (no-op)" ;;
        "Delay 100ms 10ms symmetric")     echo "\`delay 100ms 10ms distribution normal\`" ;;
        "Loss 10pct symmetric")           echo "\`loss 10%\`" ;;
        *)                                echo "N/A" ;;
    esac
}

generate_report() {
    {
    printf '# Lab 3 Part 2 – Experiment Results\n\n'
    printf '**Date:** %s\n' "$(date '+%Y-%m-%d %H:%M:%S')"
    printf '**Platform:** Linux %s (WSL2)\n\n' "$(uname -r)"

    printf '## Setup\n\n'
    printf 'Two network namespaces (`ns_client` at 10.0.0.2, `ns_server` at 10.0.0.1) connected'
    printf ' by a veth pair. `tc netem` applied symmetrically to both interfaces so DATA and ACK'
    printf ' packets are equally affected.\n\n'
    printf '| Parameter | Value |\n'
    printf '|-----------|-------|\n'
    printf '| File | `%s` (%d bytes, %d segments) |\n' \
        "$FILE" "$FILE_SIZE" "$(( (FILE_SIZE + SEG - 1) / SEG ))"
    printf '| Segment size | %d bytes |\n' "$SEG"
    printf '| Transport | UDP stop-and-wait, alternating seq 0/1 |\n'
    printf '| Initial timeout | 500 ms |\n'
    printf '| Timeout update | `clamp(2 × EWMA_rtt, 1 ms, 10 s)`, α = 0.25 |\n'
    printf '| Retransmit backoff | exponential (doubles each retry, max 10 s) |\n'
    printf '| Max retransmits | 10 per segment |\n\n'

    printf '## Scenarios\n\n'
    printf '| # | netem rule (both interfaces) |\n'
    printf '|---|------------------------------|\n'
    local i=0
    for label in "${SCENARIOS[@]}"; do
        printf '| %d | %s |\n' "$i" "$(netem_rule "$label")"
        i=$(( i + 1 ))
    done
    printf '\n'

    printf '## Results\n\n'
    printf '| Scenario | Status | Time (ms) | Throughput (KB/s) | Timeout retx | DupACK retx | Total retx | Avg RTT (ms) | Final timeout (ms) | Anomalies |\n'
    printf '|----------|:------:|----------:|------------------:|:------------:|:-----------:|:----------:|-------------:|-------------------:|----------|\n'
    for label in "${SCENARIOS[@]}"; do
        printf '| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |\n' \
            "$(display_label "$label")" \
            "${RESULT_STATUS[$label]}" \
            "${RESULT_TIME_MS[$label]}" \
            "${RESULT_THROUGHPUT[$label]}" \
            "${RESULT_RETRANSMIT_TIMEOUT[$label]}" \
            "${RESULT_RETRANSMIT_DUPAK[$label]}" \
            "${RESULT_RETRANSMIT_TOTAL[$label]}" \
            "${RESULT_FINAL_AVG_RTT[$label]}" \
            "${RESULT_FINAL_TIMEOUT[$label]}" \
            "${RESULT_ANOMALY[$label]}"
    done
    printf '\n'
    } >"$REPORT"
    echo ""
    echo "Report written to: $REPORT"
}

generate_report
