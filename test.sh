#!/bin/bash

SERVER_PORT=9090
CLIENT_PORT=8080
SERVER_IP="127.0.0.1"
FILE_NAME="test_file.txt"
SEGMENT_SIZE=256

# start server in background, serving files from the files/ directory
./target/debug/server --port $SERVER_PORT --file-directory ./files &
SERVER_PID=$!
sleep 1

# client runs from repo root, so received file lands here as test_file.txt
./target/debug/client --port $CLIENT_PORT --ip-addr $SERVER_IP --server-port $SERVER_PORT --file-name $FILE_NAME --segment-size $SEGMENT_SIZE

# compare received file against the original
if diff -q $FILE_NAME ./files/$FILE_NAME > /dev/null 2>&1; then
    echo "PASS: files match"
else
    echo "FAIL: files differ"
fi

kill $SERVER_PID
