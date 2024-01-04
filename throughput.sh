#!/usr/bin/env bash

set -x

SUB=./target/release/examples/z_sub_thr
PUB=./target/release/examples/z_pub_thr
SUB_CPUS="0-1"
PUB_CPUS="2-3"
NUM=5000
# NUM=10
TIMEOUT=10s
LOG_DIR="_throughtput_logs"

function run() {
    ENDPOINT=$1
    LOG_FILE=$2
    PAYLOAD=$3

    pkill z_sub_thr
    pkill z_pub_thr

    rm $LOG_FILE &> /dev/null
    mkfifo /tmp/srv-input &> /dev/null
    parallel --halt now,success=1 --lb  <<EOL
    tail -f /tmp/srv-input | nice -n -20 taskset -c "$SUB_CPUS" "$SUB" --no-multicast-scouting -l "$ENDPOINT" -s $NUM > $LOG_FILE  && echo DONE
    sleep 1 && nice -n -20 taskset -c "$PUB_CPUS" $PUB $PAYLOAD -m client --no-multicast-scouting -e "$ENDPOINT"
    sleep 1 && sleep $TIMEOUT
EOL
}

function terminate() {
    pkill z_sub_thr
    pkill z_pub_thr
    exit
}

trap terminate SIGINT

rm -rf $LOG_DIR
mkdir -p $LOG_DIR

for PAYLOAD in 8 16 32 64 128 256 512 1024 2048 4096 8192 16384 32768; do
    run "tcp/127.0.0.1:17447" "$LOG_DIR/$PAYLOAD.log" $PAYLOAD
done

chown -R $(stat -c "%U:%G" Cargo.toml) $LOG_DIR
