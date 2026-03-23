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
source_branch="${SOURCE_BRANCH:-main}"
patch_base_ref="${PATCH_BASE_REF:-${upstream_tag}}"
queue_branch="${QUEUE_BRANCH:-queue/internal}"
queue_base_branch="${QUEUE_BASE_BRANCH:-queue/base/internal}"
bootstrap_branch="${BOOTSTRAP_BRANCH:-bootstrap/queue/${upstream_tag}}"
push_refs="${PUSH_REFS:-false}"
promote_main="${PROMOTE_MAIN:-false}"
allow_overwrite="${ALLOW_OVERWRITE:-false}"
push_token="${PUSH_TOKEN:-}"
upstream_remote="${UPSTREAM_REMOTE:-upstream}"
upstream_url="${UPSTREAM_URL:-https://github.com/openai/codex}"
patch_commits_raw="${PATCH_COMMITS:-}"
patch_commits_file="${PATCH_COMMITS_FILE:-}"
cargo_lock_path="codex-rs/Cargo.lock"
cargo_manifest_path="codex-rs/Cargo.toml"

if [[ ! "${upstream_tag}" =~ ^rust-v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "unexpected upstream tag format: ${upstream_tag}" >&2
  exit 1
fi

repo_root="$(git rev-parse --show-toplevel)"
cd "${repo_root}"

strip_cargo_lock_from_head_commit() {
  if ! git rev-parse --verify HEAD^ >/dev/null 2>&1; then
    return
  fi

  if git diff --quiet HEAD^ HEAD -- "${cargo_lock_path}"; then
    return
  fi

  # Keep queue commits semantic and release-agnostic; refresh Cargo.lock after replay.
  git restore --source=HEAD^ --staged --worktree -- "${cargo_lock_path}"
  git commit --amend --no-edit
}

refresh_cargo_lock() {
  if [[ ! -f "${cargo_manifest_path}" || ! -f "${cargo_lock_path}" ]]; then
    return
  fi

  if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo is required to refresh ${cargo_lock_path}" >&2
    exit 1
  fi

  (
    cd codex-rs
    cargo metadata --format-version 1 >/dev/null
  )
}

replay_queue_commit() {
  local commit="$1"
  local remaining_conflicts=()

  if git cherry-pick -x "${commit}"; then
    strip_cargo_lock_from_head_commit
    return 0
  fi

  if git diff --name-only --diff-filter=U | grep -Fxq "${cargo_lock_path}"; then
    git restore --source=HEAD --staged --worktree -- "${cargo_lock_path}"
    git add "${cargo_lock_path}"
  fi

  mapfile -t remaining_conflicts < <(git diff --name-only --diff-filter=U | sort -u)
  if [[ ${#remaining_conflicts[@]} -eq 0 ]]; then
    git cherry-pick --continue
    strip_cargo_lock_from_head_commit
    return 0
  fi

  return 1
}

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

if [[ -n "${push_token}" ]]; then
  git remote set-url --push "${fork_remote}" "https://x-access-token:${push_token}@github.com/${fork_repo}.git"
else
  git remote set-url --push "${fork_remote}" "${fork_url}"
fi

if git remote get-url "${upstream_remote}" >/dev/null 2>&1; then
  git remote set-url "${upstream_remote}" "${upstream_url}"
else
  git remote add "${upstream_remote}" "${upstream_url}"
fi

git fetch --quiet "${fork_remote}" "refs/heads/${source_branch}:refs/remotes/${fork_remote}/${source_branch}"
if [[ "${promote_main}" == "true" || "${allow_overwrite}" == "true" ]]; then
  if git ls-remote --exit-code --heads "${fork_remote}" "refs/heads/main" >/dev/null 2>&1; then
    git fetch --quiet "${fork_remote}" "refs/heads/main:refs/remotes/${fork_remote}/main"
  fi
  if git ls-remote --exit-code --heads "${fork_remote}" "refs/heads/${queue_branch}" >/dev/null 2>&1; then
    git fetch --quiet "${fork_remote}" "refs/heads/${queue_branch}:refs/remotes/${fork_remote}/${queue_branch}"
  fi
  if git ls-remote --exit-code --heads "${fork_remote}" "refs/heads/${queue_base_branch}" >/dev/null 2>&1; then
    git fetch --quiet "${fork_remote}" "refs/heads/${queue_base_branch}:refs/remotes/${fork_remote}/${queue_base_branch}"
  fi
fi
git fetch --quiet "${upstream_remote}" "refs/tags/${upstream_tag}:refs/tags/${upstream_tag}"

if ! git rev-parse --verify "${patch_base_ref}^{commit}" >/dev/null 2>&1; then
  echo "patch base ref not found: ${patch_base_ref}" >&2
  exit 1
fi

mapfile -t patch_commits < <(
  if [[ -n "${patch_commits_raw}" ]]; then
    printf '%s\n' "${patch_commits_raw}" | tr ' ' '\n' | sed '/^$/d'
  elif [[ -n "${patch_commits_file}" ]]; then
    sed '/^$/d' "${patch_commits_file}"
  else
    git rev-list --reverse --no-merges "${patch_base_ref}..${fork_remote}/${source_branch}"
  fi
)

git checkout -B "${bootstrap_branch}" "${upstream_tag}"

for commit in "${patch_commits[@]}"; do
  replay_queue_commit "${commit}"
done

refresh_cargo_lock
if ! git diff --quiet -- "${cargo_lock_path}"; then
  git add "${cargo_lock_path}"
  git commit -m "chore: refresh Cargo.lock for ${upstream_tag}"
fi

git branch -f "${queue_base_branch}" "${upstream_tag}"

if [[ "${push_refs}" == "true" ]]; then
  if [[ "${allow_overwrite}" != "true" ]]; then
    if git ls-remote --exit-code --heads "${fork_remote}" "${queue_branch}" >/dev/null 2>&1; then
      echo "${fork_remote}/${queue_branch} already exists; rerun with ALLOW_OVERWRITE=true to replace it" >&2
      exit 1
    fi
    if git ls-remote --exit-code --heads "${fork_remote}" "${queue_base_branch}" >/dev/null 2>&1; then
      echo "${fork_remote}/${queue_base_branch} already exists; rerun with ALLOW_OVERWRITE=true to replace it" >&2
      exit 1
    fi
  fi

  push_options=(--force-with-lease)
  push_refspecs=(
    "${bootstrap_branch}:refs/heads/${queue_branch}"
    "${queue_base_branch}:refs/heads/${queue_base_branch}"
  )

  if [[ "${promote_main}" == "true" ]]; then
    push_refspecs+=("${bootstrap_branch}:refs/heads/main")
  fi

  git push "${push_options[@]}" "${fork_remote}" "${push_refspecs[@]}"
fi

if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  {
    echo "## Queue bootstrap prepared"
    echo
    echo "- upstream tag: \`${upstream_tag}\`"
    echo "- source branch: \`${fork_remote}/${source_branch}\`"
    echo "- bootstrap branch: \`${bootstrap_branch}\`"
    echo "- queue branch target: \`${queue_branch}\`"
    echo "- queue base target: \`${queue_base_branch}\`"
    if [[ "${promote_main}" == "true" ]]; then
      echo "- main promotion: enabled"
    fi
  } >> "${GITHUB_STEP_SUMMARY}"
fi

echo "Prepared bootstrap branch ${bootstrap_branch}"
