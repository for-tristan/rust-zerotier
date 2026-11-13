#!/bin/bash
# Bring up the rust-zerotier TAP interface after `rzt vport` has created it.
# Port of the original build-your-own-zerotier setup.sh, generalised to take
# the interface name and address as arguments.
#
# Usage:
#   sudo ./setup.sh                         # rzt0, 10.1.1.101/24
#   sudo ./setup.sh rzt0 10.1.1.102/24      # custom name and address
set -eu

IFNAME="${1:-rzt0}"
ADDR="${2:-10.1.1.101/24}"

if [ "$(id -u)" -ne 0 ]; then
    echo "setup.sh: needs root (or CAP_NET_ADMIN) to configure $IFNAME" >&2
    exit 1
fi

ip link set "$IFNAME" up
ip addr add "$ADDR" dev "$IFNAME"

echo "[setup] $IFNAME is up at $ADDR"
