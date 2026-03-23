#!/usr/bin/env bash

set -euo pipefail

repo="${1:-openai/codex}"

tags="$(
  gh api "repos/${repo}/releases?per_page=100" --paginate \
    --jq '
      .[]
      | select(
          .draft == false
          and .prerelease == false
          and (.tag_name | test("^rust-v[0-9]+\\.[0-9]+\\.[0-9]+$"))
        )
      | .tag_name
    '
)"

tag="${tags%%$'\n'*}"

if [[ -z "${tag}" ]]; then
  echo "failed to resolve latest upstream rust release for ${repo}" >&2
  exit 1
fi

echo "${tag}"
