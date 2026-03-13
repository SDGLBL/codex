#!/usr/bin/env bash

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <upstream-tag>" >&2
  exit 1
fi

upstream_tag="$1"
fork_remote="${FORK_REMOTE:-origin}"
fork_repo="${FORK_REPO:-}"
fork_url="${FORK_URL:-}"
base_branch="${BASE_BRANCH:-main}"
patch_base_ref="${PATCH_BASE_REF:-${upstream_tag}}"
sync_branch="${SYNC_BRANCH:-sync/${upstream_tag}}"
patch_branch="${PATCH_BRANCH:-${fork_remote}/patches/internal}"
upstream_remote="${UPSTREAM_REMOTE:-upstream}"
upstream_url="${UPSTREAM_URL:-https://github.com/openai/codex}"
push_branch="${PUSH_BRANCH:-true}"
open_pr="${OPEN_PR:-true}"

if [[ ! "${upstream_tag}" =~ ^rust-v[0-9]+\.[0-9]+\.[0-9]+(-(alpha|beta)(\.[0-9]+)?)?$ ]]; then
  echo "unexpected upstream tag format: ${upstream_tag}" >&2
  exit 1
fi

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"

if [[ -z "${fork_repo}" && -n "${fork_url}" ]]; then
  fork_repo="${fork_url#https://github.com/}"
  fork_repo="${fork_repo%.git}"
fi

if [[ -z "${fork_url}" && -n "${fork_repo}" ]]; then
  fork_url="https://github.com/${fork_repo}"
fi

if [[ -z "${fork_url}" ]] && git remote get-url "${fork_remote}" >/dev/null 2>&1; then
  fork_url="$(git remote get-url "${fork_remote}")"
fi

if [[ -z "${fork_repo}" ]] && [[ -n "${fork_url}" ]]; then
  fork_repo="${fork_url#https://github.com/}"
  fork_repo="${fork_repo%.git}"
fi

if [[ -z "${fork_url}" ]]; then
  echo "failed to resolve fork url; set FORK_URL or configure ${fork_remote}" >&2
  exit 1
fi

if [[ -z "${fork_repo}" ]]; then
  echo "failed to resolve fork repo; set FORK_REPO or FORK_URL" >&2
  exit 1
fi

if git remote get-url "${fork_remote}" >/dev/null 2>&1; then
  git remote set-url "${fork_remote}" "${fork_url}"
else
  git remote add "${fork_remote}" "${fork_url}"
fi

if git remote get-url "${upstream_remote}" >/dev/null 2>&1; then
  git remote set-url "${upstream_remote}" "${upstream_url}"
else
  git remote add "${upstream_remote}" "${upstream_url}"
fi

git fetch --quiet "${fork_remote}" "refs/heads/${base_branch}:refs/remotes/${fork_remote}/${base_branch}"
if [[ "${patch_branch}" == "${fork_remote}/"* ]]; then
  patch_branch_name="${patch_branch#"${fork_remote}/"}"
  git fetch --quiet "${fork_remote}" "refs/heads/${patch_branch_name}:refs/remotes/${fork_remote}/${patch_branch_name}"
elif ! git rev-parse --verify "${patch_branch}" >/dev/null 2>&1; then
  git fetch --quiet "${fork_remote}" "${patch_branch}"
fi
git fetch --quiet "${upstream_remote}" "refs/heads/main:refs/remotes/${upstream_remote}/main"
git fetch --quiet "${upstream_remote}" "refs/tags/${upstream_tag}:refs/tags/${upstream_tag}"

mapfile -t patch_commits < <("${repo_root}/scripts/internal/patch_stack_commits.sh" "${patch_branch}" "${patch_base_ref}")

if [[ ${#patch_commits[@]} -eq 0 ]]; then
  echo "no patch commits found on ${patch_branch}" >&2
  exit 1
fi

git checkout -B "${sync_branch}" "${upstream_tag}"
git cherry-pick -x "${patch_commits[@]}"

changed_paths="$(git diff --name-only HEAD~${#patch_commits}..HEAD)"
if command -v just >/dev/null 2>&1 && grep -Eq '^codex-rs/core/src/config/(mod|profile)\.rs$' <<<"${changed_paths}"; then
  (
    cd "${repo_root}/codex-rs"
    just write-config-schema
  )
  generated_artifacts=()
  if ! git diff --quiet -- codex-rs/core/config.schema.json; then
    generated_artifacts+=(codex-rs/core/config.schema.json)
  fi
  if ! git diff --quiet -- codex-rs/Cargo.lock; then
    generated_artifacts+=(codex-rs/Cargo.lock)
  fi
  if [[ ${#generated_artifacts[@]} -gt 0 ]]; then
    git add "${generated_artifacts[@]}"
    git commit -m "chore: refresh generated artifacts for ${upstream_tag}"
  fi
fi

body_file="$(mktemp)"
{
  echo "## Summary"
  echo "- sync fork against upstream \`${upstream_tag}\`"
  echo "- cherry-pick the current internal patch stack from \`${patch_branch}\`"
  echo "- keep the fork release automation and internal release tag flow"
  echo
  echo "## Patch Stack"
  for commit in "${patch_commits[@]}"; do
    git log -1 --format='- `%h` %s' "${commit}"
  done
  echo
  echo "## Notes"
  echo "- branch name: \`${sync_branch}\`"
  echo "- internal release tag after merge: \`internal-${upstream_tag}\`"
} >"${body_file}"

if [[ "${push_branch}" == "true" ]]; then
  git push --force-with-lease "${fork_remote}" "${sync_branch}"
fi

if [[ "${open_pr}" == "true" ]]; then
    existing_pr="$(
    gh pr list \
      --repo "${fork_repo}" \
      --base "${base_branch}" \
      --head "${sync_branch}" \
      --state open \
      --json number \
      --jq '.[0].number // ""'
  )"
  title="Sync ${upstream_tag} + internal patch set"
  if [[ -n "${existing_pr}" ]]; then
    gh pr edit "${existing_pr}" --repo "${fork_repo}" --title "${title}" --body-file "${body_file}"
  else
    gh pr create --repo "${fork_repo}" --base "${base_branch}" --head "${sync_branch}" --title "${title}" --body-file "${body_file}"
  fi
fi

echo "Prepared ${sync_branch} from ${upstream_tag}"
