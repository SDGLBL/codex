#!/usr/bin/env bash

set -euo pipefail

repo="${1:-openai/codex}"

tag="$(
  gh api "repos/${repo}/releases?per_page=20" \
    --jq 'map(select(.draft == false and .prerelease == false and (.tag_name | startswith("rust-v"))))[0].tag_name // ""'
)"

if [[ -z "${tag}" ]]; then
  echo "failed to resolve latest stable rust release for ${repo}" >&2
  exit 1
fi

echo "${tag}"
