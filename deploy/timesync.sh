#!/bin/bash
# Resolve the board for a role and configure its NTP client.
#
#   timesync.sh <left|right> [user@host]
set -euo pipefail

ROLE=${1:-left}
HOST=${2:-}
LEFT_IP=${LEFT_IP:-192.168.42.92}
RIGHT_IP=${RIGHT_IP:-192.168.42.128}
NTP_SERVER=${NTP_SERVER:-192.168.42.253}

if [[ -z "$HOST" ]]; then
  case "$ROLE" in
    left)  HOST="radxa@$LEFT_IP" ;;
    right) HOST="radxa@$RIGHT_IP" ;;
    *) echo "unknown role: $ROLE" >&2; exit 1 ;;
  esac
fi

HERE=$(cd "$(dirname "$0")" && pwd)
SSH_PORT=${SSH_PORT:-2222} "$HERE/setup-timesync.sh" "$HOST" "$NTP_SERVER"
