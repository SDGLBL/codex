#!/usr/bin/env bash

set -euo pipefail

patch_branch="${1:-origin/queue/internal}"
base_ref="${2:-origin/queue/base/internal}"

if ! git rev-parse --verify "${patch_branch}^{commit}" >/dev/null 2>&1; then
  echo "patch branch not found: ${patch_branch}" >&2
  exit 1
fi

if ! git rev-parse --verify "${base_ref}^{commit}" >/dev/null 2>&1; then
  echo "base ref not found: ${base_ref}" >&2
  exit 1
fi

merge_base="$(git merge-base "${patch_branch}" "${base_ref}")"
base_sha="$(git rev-parse "${base_ref}^{commit}")"

if [[ -z "${merge_base}" ]]; then
  echo "failed to compute merge base for ${patch_branch} and ${base_ref}" >&2
  exit 1
fi

if [[ "${merge_base}" != "${base_sha}" ]]; then
  echo "base ref ${base_ref} is not an ancestor of ${patch_branch}" >&2
  exit 1
fi

git rev-list --reverse --no-merges "${base_ref}..${patch_branch}"
