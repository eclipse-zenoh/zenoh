#!/usr/bin/env bash
# Bring up a virtual CAN interface for the zenoh CAN link tests, so the whole
# transport can be exercised with no CAN hardware.
#
#   ci/vcan-setup.sh              # create and bring up vcan0
#   ci/vcan-setup.sh vcan1        # a different interface name
#   ci/vcan-setup.sh --status     # report, changing nothing
#   ci/vcan-setup.sh --down       # tear down vcan0
#
# Creating the interface needs root, so the script re-executes itself under
# sudo -- but only when something actually has to change, so --status and a
# re-run against an interface that is already up never prompt.
#
# Once it is up:
#
#   cargo test -p zenoh-transport --features transport_can \
#       --test multicast_can -- --ignored --nocapture
#   candump -td vcan0        # watch every frame, in another terminal
#
# Real hardware is configured differently: the bit rates are set on the
# interface rather than by the endpoint, e.g.
#
#   sudo ip link set can0 type can bitrate 500000 dbitrate 2000000 fd on
#   sudo ip link set up can0
#
set -euo pipefail

DEV="vcan0"
ACTION="up"

for arg in "$@"; do
    case "$arg" in
        --down) ACTION="down" ;;
        --status) ACTION="status" ;;
        -h | --help)
            # Everything from the shebang to the first line of code, so the
            # help text cannot drift out of step with the comment block.
            awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"
            exit 0
            ;;
        -*)
            echo "unknown option: $arg" >&2
            exit 2
            ;;
        *) DEV="$arg" ;;
    esac
done

if [ "$(uname -s)" != "Linux" ]; then
    echo "[vcan-setup] SocketCAN is a Linux kernel interface; nothing to do on $(uname -s)" >&2
    exit 1
fi

have_dev() { ip link show "$DEV" >/dev/null 2>&1; }

# Read the operational state directly rather than grepping the flags, so a
# grep that fails to run cannot be mistaken for an interface that is down.
# A vcan interface reports UNKNOWN rather than UP once it is administratively up.
is_up() {
    local state
    state=$(ip -br link show "$DEV" 2>/dev/null | awk '{print $2}')
    [ "$state" = "UP" ] || [ "$state" = "UNKNOWN" ]
}

status() {
    if ! have_dev; then
        echo "  $DEV: absent"
        return 1
    fi
    ip -br link show "$DEV" | sed 's/^/  /'
    if ! is_up; then
        echo "  ($DEV exists but is not up)"
        return 1
    fi
    return 0
}

need_root() {
    if [ "$(id -u)" -ne 0 ]; then
        echo "[vcan-setup] needs root; re-running under sudo"
        exec sudo -- "$0" "$@"
    fi
}

case "$ACTION" in
    status)
        echo "[vcan-setup] status"
        status || exit 1
        ;;

    down)
        if ! have_dev; then
            echo "[vcan-setup] $DEV already absent"
            exit 0
        fi
        need_root "$@"
        ip link set down "$DEV" 2>/dev/null || true
        ip link delete "$DEV" type vcan
        echo "[vcan-setup] removed $DEV"
        ;;

    up)
        # Re-runs are harmless, and this is the path that avoids a pointless
        # sudo prompt when the interface is already there.
        if have_dev && is_up; then
            echo "[vcan-setup] $DEV already up"
            status
            exit 0
        fi

        need_root "$@"

        # Not fatal if vcan is built into the kernel rather than a module.
        modprobe vcan 2>/dev/null || true
        if ! have_dev && ! modinfo vcan >/dev/null 2>&1; then
            echo "[vcan-setup] the vcan module is not available on this kernel" >&2
            echo "             (Debian/Ubuntu: apt install linux-modules-extra-\$(uname -r))" >&2
            exit 1
        fi

        have_dev || ip link add dev "$DEV" type vcan
        ip link set up "$DEV"

        echo "[vcan-setup] $DEV ready"
        status

        # candump is how you watch the wire. Worth saying once, rather than
        # having someone conclude the link is dead when it is just unobserved.
        if ! command -v candump >/dev/null 2>&1; then
            echo "[vcan-setup] note: can-utils is not installed, so 'candump $DEV' is"
            echo "             unavailable. It is the fastest way to see whether frames"
            echo "             are moving. (Debian/Ubuntu: apt install can-utils)"
        fi
        ;;
esac
