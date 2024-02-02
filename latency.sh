#!/usr/bin/env bash

# How to build
# cargo build --release --example z_ping --example z_pong -F transport_unixpipe

set -x

USE_LT="-c _config-low-latency.json5"
NOT_USE_LT=""
PONG=./target/release/examples/z_pong
PING=./target/release/examples/z_ping
PONG_CPUS="0-1"
PING_CPUS="2-3"
NUM=500000
# NUM=500
LOG_DIR="_latency_logs"

function run() {
    ENDPOINT=$1
    USE_LT_OR_NOT=$2
    LOG_FILE=$3

    rm $LOG_FILE &> /dev/null
    mkfifo /tmp/srv-input &> /dev/null
    parallel --halt now,success=1 --lb  <<EOL
    tail -f /tmp/srv-input | nice -n -20 taskset -c "$PONG_CPUS" "$PONG" --no-multicast-scouting -l "$ENDPOINT" $USE_LT_OR_NOT
    sleep 1 && \
        nice -n -20 taskset -c "$PING_CPUS" $PING 64 -m client --no-multicast-scouting -e "$ENDPOINT" $USE_LT_OR_NOT -n $NUM > $LOG_FILE && \
        echo DONE
EOL
}

function terminate() {
    pkill z_pong
    pkill z_ping
    exit
}

trap terminate SIGINT

rm -rf $LOG_DIR
mkdir -p $LOG_DIR
run "unixpipe/example_endpoint.pipe" "$USE_LT"     $LOG_DIR/"LT-UNP.log"
run "unixpipe/example_endpoint.pipe" "$NOT_USE_LT" $LOG_DIR/"NT-UNP.log"
run "tcp/127.0.0.1:17447"            "$USE_LT"     $LOG_DIR/"LT-TCP.log"
run "tcp/127.0.0.1:17447"            "$NOT_USE_LT" $LOG_DIR/"NT-TCP.log"

chown -R $(stat -c "%U:%G" Cargo.toml) $LOG_DIR
rm -f example_endpoint.pipe_*
terminate
