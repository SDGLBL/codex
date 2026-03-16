#!/usr/bin/env bash

set -euo pipefail

patch_branch="${1:-origin/patches/internal}"
base_ref="${2:-main}"

if ! git rev-parse --verify "${patch_branch}" >/dev/null 2>&1; then
  echo "patch branch not found: ${patch_branch}" >&2
  exit 1
fi

if ! git rev-parse --verify "${base_ref}" >/dev/null 2>&1; then
  echo "base ref not found: ${base_ref}" >&2
  exit 1
fi

merge_base="$(git merge-base "${patch_branch}" "${base_ref}")"

if [[ -z "${merge_base}" ]]; then
  echo "failed to compute merge base for ${patch_branch} and ${base_ref}" >&2
  exit 1
fi

git rev-list --reverse --no-merges "${merge_base}..${patch_branch}"
