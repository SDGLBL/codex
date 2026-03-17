#!/usr/bin/env bash

set -euo pipefail

echo "prepare_sync_pr.sh is deprecated; preparing a candidate queue replay branch instead." >&2
MODE="${MODE:-official}" exec "$(dirname "$0")/prepare_queue_pr.sh" "$@"
