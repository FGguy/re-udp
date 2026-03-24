#!/bin/bash

BINARY_DIR=./target/debug
FILE=test_file.txt
SEG=256
SERVER_IP=10.0.0.1

run_scenario() {
    local label=$1

    echo ""
    echo "=== $label ==="

    ip netns exec ns_server $BINARY_DIR/server --port 9090 --file-directory ./files &
    SERVER_PID=$!
    sleep 0.5

    time ip netns exec ns_client $BINARY_DIR/client \
        --port 8080 --ip-addr $SERVER_IP --server-port 9090 \
        --file-name $FILE --segment-size $SEG

    kill $SERVER_PID 2>/dev/null
    wait $SERVER_PID 2>/dev/null

    diff $FILE ./files/$FILE && echo "PASS" || echo "FAIL"
    rm -f $FILE
}

# Scenario 1: 100ms fixed delay
ip netns exec ns_client tc qdisc replace dev veth_client root netem delay 100ms
run_scenario "Delay 100ms"

# Scenario 2: 10% packet loss
ip netns exec ns_client tc qdisc replace dev veth_client root netem loss 10%
run_scenario "Loss 10%"
