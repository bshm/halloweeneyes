#!/bin/bash
# Resolve the board for a role and install the artefacts.
#
#   deploy.sh <left|right> [user@host]
set -euo pipefail

ROLE=${1:-left}
HOST=${2:-}
LEFT_IP=${LEFT_IP:-192.168.42.92}
RIGHT_IP=${RIGHT_IP:-192.168.42.128}

if [[ -z "$HOST" ]]; then
  case "$ROLE" in
    left)  HOST="radxa@$LEFT_IP" ;;
    right) HOST="radxa@$RIGHT_IP" ;;
    *) echo "unknown role: $ROLE" >&2; exit 1 ;;
  esac
fi

HERE=$(cd "$(dirname "$0")" && pwd)
SSH_PORT=${SSH_PORT:-2222} "$HERE/install-board.sh" "$HOST" "$ROLE"
