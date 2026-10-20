#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

if [[ ${ZENOH_MULTICAST_FANOUT_INNER:-0} != 1 ]]; then
  for command in cargo grep ip nsenter python3 timeout unshare; do
    command -v "$command" >/dev/null || {
      echo "missing required command: $command" >&2
      exit 1
    }
  done

  cargo build --manifest-path "$root/Cargo.toml" \
    -p zenoh-examples --example z_pub --example z_sub

  if ((EUID == 0)); then
    exec env ZENOH_MULTICAST_FANOUT_INNER=1 unshare --net "$0"
  fi
  exec unshare --user --map-root-user --net \
    env ZENOH_MULTICAST_FANOUT_INNER=1 "$0"
fi

bin="$root/target/debug/examples"
work="$root/target/multicast-fanout"
rm -rf "$work"
mkdir -p "$work"
report="$work/report.tsv"
printf 'receivers\tpublications\tdurable_copies\tmax_ingress_packets\n' >"$report"

keepers=()
workers=()
bridge=

cleanup_case() {
  local pid
  for pid in "${workers[@]}" "${keepers[@]}"; do
    kill "$pid" 2>/dev/null || true
  done
  for pid in "${workers[@]}" "${keepers[@]}"; do
    wait "$pid" 2>/dev/null || true
  done
  workers=()
  keepers=()
  if [[ -n $bridge ]]; then
    ip link del "$bridge" 2>/dev/null || true
    bridge=
  fi
}
trap cleanup_case EXIT

wait_for_timeout() {
  local pid=$1
  local status
  if wait "$pid"; then
    echo "process $pid exited before its bounded test window" >&2
    return 1
  else
    status=$?
  fi
  [[ $status == 124 ]] || {
    echo "process $pid failed with status $status" >&2
    return 1
  }
}

run_case() {
  local receiver_count=$1
  local case_dir="$work/n$receiver_count"
  local group="239.255.44.1:$((19447 + receiver_count))"
  local host peer
  local i keeper worker publisher publications count packets
  local max_packets=0
  local packet_limit=$((32 + 4 * receiver_count))

  mkdir -p "$case_dir"
  bridge="br$receiver_count"
  ip link add "$bridge" type bridge
  ip link set "$bridge" up

  for ((i = 0; i <= receiver_count; i++)); do
    host="h${receiver_count}x$i"
    peer="p${receiver_count}x$i"
    unshare --net sleep 120 &
    keeper=$!
    keepers+=("$keeper")
    ip link add "$host" type veth peer name "$peer"
    ip link set "$host" master "$bridge"
    ip link set "$host" up
    ip link set "$peer" netns "$keeper" name eth0
    nsenter -t "$keeper" -n ip link set lo up
    nsenter -t "$keeper" -n ip link set eth0 up
    nsenter -t "$keeper" -n ip addr add "10.88.$receiver_count.$((i + 1))/24" dev eth0
  done

  for ((i = 1; i <= receiver_count; i++)); do
    timeout 7s nsenter -t "${keepers[$i]}" -n "$bin/z_sub" \
      --no-multicast-scouting --listen "udp/$group#iface=eth0" \
      --key test/session/multicast/fanout >"$case_dir/sub$i.log" 2>&1 &
    workers+=("$!")
  done

  sleep 2
  timeout 4s nsenter -t "${keepers[0]}" -n "$bin/z_pub" \
    --no-multicast-scouting --listen "udp/$group#iface=eth0" \
    --key test/session/multicast/fanout --payload probe \
    >"$case_dir/publisher.log" 2>&1 &
  publisher=$!
  wait_for_timeout "$publisher"
  for worker in "${workers[@]}"; do
    wait_for_timeout "$worker"
  done

  publications=$(grep -c 'Putting Data' "$case_dir/publisher.log" || true)
  [[ $publications == 3 ]] || {
    echo "n=$receiver_count published $publications messages, expected 3" >&2
    return 1
  }

  for ((i = 1; i <= receiver_count; i++)); do
    count=$(grep -c 'Received' "$case_dir/sub$i.log" || true)
    [[ $count == "$publications" ]] || {
      echo "n=$receiver_count receiver=$i delivered=$count expected=$publications" >&2
      return 1
    }
  done

  for ((i = 0; i <= receiver_count; i++)); do
    host="h${receiver_count}x$i"
    packets=$(ip -s -j link show "$host" | python3 -c \
      'import json,sys; print(json.load(sys.stdin)[0]["stats64"]["rx"]["packets"])')
    ((packets > max_packets)) && max_packets=$packets
  done
  ((max_packets <= packet_limit)) || {
    echo "n=$receiver_count amplification: packets=$max_packets limit=$packet_limit" >&2
    return 1
  }

  printf '%s\t%s\t%s\t%s\n' \
    "$receiver_count" "$publications" "$((receiver_count * publications))" "$max_packets" \
    | tee -a "$report"
  cleanup_case
}

ip link set lo up
for receiver_count in ${ZENOH_MULTICAST_RECEIVER_COUNTS:-1 2 4 8 16}; do
  run_case "$receiver_count"
done

echo "PASS: monotonic multicast fanout without declaration amplification; report=$report"
