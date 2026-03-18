#!/usr/bin/env bash

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <upstream-tag>" >&2
  exit 1
fi

upstream_tag="$1"
mode="${MODE:-official}"
fork_remote="${FORK_REMOTE:-origin}"
fork_repo="${FORK_REPO:-}"
fork_url="${FORK_URL:-}"
queue_branch="${QUEUE_BRANCH:-queue/internal}"
queue_base_branch="${QUEUE_BASE_BRANCH:-queue/base/internal}"
candidate_branch="${CANDIDATE_BRANCH:-}"
upstream_remote="${UPSTREAM_REMOTE:-upstream}"
upstream_url="${UPSTREAM_URL:-https://github.com/openai/codex}"
push_branch="${PUSH_BRANCH:-true}"
open_pr="${OPEN_PR:-auto}"
rehearsal_suffix="${REHEARSAL_SUFFIX:-${GITHUB_RUN_ID:-$(date +%Y%m%d%H%M%S)}}"
conflict_note_path="${CONFLICT_NOTE_PATH:-QUEUE_REPLAY_CONFLICTS.md}"
effective_rehearsal_suffix=""
cargo_lock_path="codex-rs/Cargo.lock"
cargo_manifest_path="codex-rs/Cargo.toml"

if [[ ! "${upstream_tag}" =~ ^rust-v[0-9]+\.[0-9]+\.[0-9]+(-(alpha|beta)(\.[0-9]+)?)?$ ]]; then
  echo "unexpected upstream tag format: ${upstream_tag}" >&2
  exit 1
fi

case "${mode}" in
  official|rehearsal)
    ;;
  *)
    echo "unexpected MODE: ${mode}" >&2
    exit 1
    ;;
esac

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

  if git cherry-pick -x "${commit}"; then
    strip_cargo_lock_from_head_commit
    return 0
  fi

  if git diff --name-only --diff-filter=U | grep -Fxq "${cargo_lock_path}"; then
    git restore --source=HEAD --staged --worktree -- "${cargo_lock_path}"
    git add "${cargo_lock_path}"
  fi

  mapfile -t conflict_files < <(git diff --name-only --diff-filter=U | sort -u)
  if [[ ${#conflict_files[@]} -eq 0 ]]; then
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

if git remote get-url "${upstream_remote}" >/dev/null 2>&1; then
  git remote set-url "${upstream_remote}" "${upstream_url}"
else
  git remote add "${upstream_remote}" "${upstream_url}"
fi

git fetch --quiet "${fork_remote}" "refs/heads/${queue_branch}:refs/remotes/${fork_remote}/${queue_branch}" || {
  echo "missing ${fork_remote}/${queue_branch}; bootstrap queue refs before replaying releases" >&2
  exit 1
}
git fetch --quiet "${fork_remote}" "refs/heads/${queue_base_branch}:refs/remotes/${fork_remote}/${queue_base_branch}" || {
  echo "missing ${fork_remote}/${queue_base_branch}; bootstrap queue refs before replaying releases" >&2
  exit 1
}
git fetch --quiet "${upstream_remote}" "refs/heads/main:refs/remotes/${upstream_remote}/main"
git fetch --quiet "${upstream_remote}" "refs/tags/${upstream_tag}:refs/tags/${upstream_tag}"

mapfile -t patch_commits < <(
  "${repo_root}/scripts/internal/patch_stack_commits.sh" \
    "${fork_remote}/${queue_branch}" \
    "${fork_remote}/${queue_base_branch}"
)

render_commit_list() {
  if [[ $# -eq 0 ]]; then
    echo "- No fork-only patch commits are currently queued."
    return
  fi

  local commit
  for commit in "$@"; do
    git log -1 --format='- `%h` %s' "${commit}"
  done
}

if [[ -z "${candidate_branch}" ]]; then
  if [[ "${mode}" == "official" ]]; then
    candidate_branch="candidate/queue/${upstream_tag}"
  else
    effective_rehearsal_suffix="$(
      printf '%s' "${rehearsal_suffix}" | tr -cs 'A-Za-z0-9._-' '-'
    )"
    candidate_branch="rehearsal/queue/${upstream_tag}-${effective_rehearsal_suffix}"
  fi
fi

if [[ "${open_pr}" == "auto" ]]; then
  if [[ "${mode}" == "official" ]]; then
    open_pr="true"
  else
    open_pr="false"
  fi
fi

conflict_detected=false
conflict_commit=""
conflict_files=()
existing_pr=""
rehearsal_tag=""

git checkout -B "${candidate_branch}" "${upstream_tag}"

for commit in "${patch_commits[@]}"; do
  if replay_queue_commit "${commit}"; then
    continue
  fi

  conflict_detected=true
  conflict_commit="${commit}"
  git cherry-pick --abort
  break
done

if [[ "${conflict_detected}" != "true" ]]; then
  refresh_cargo_lock
  if ! git diff --quiet -- "${cargo_lock_path}"; then
    git add "${cargo_lock_path}"
    git commit -m "chore: refresh Cargo.lock for ${upstream_tag}"
  fi
fi

remaining_commits=()
if [[ "${conflict_detected}" == "true" ]]; then
  include_remaining=false
  for commit in "${patch_commits[@]}"; do
    if [[ "${commit}" == "${conflict_commit}" ]]; then
      include_remaining=true
    fi
    if [[ "${include_remaining}" == "true" ]]; then
      remaining_commits+=("${commit}")
    fi
  done

  {
    echo "# Manual Queue Replay Conflict Resolution"
    echo
    echo "Automatic replay of the queue patch stack onto \`${upstream_tag}\` stopped at:"
    git log -1 --format='- `%h` %s' "${conflict_commit}"
    echo
    echo "This branch already contains every replayed patch before the failure point."
    echo "Continue locally by replaying the remaining commits in order."
    echo
    echo "## Conflicting Files"
    if [[ ${#conflict_files[@]} -eq 0 ]]; then
      echo "- Git reported a conflict, but no unmerged file paths were captured."
    else
      local_path=""
      for local_path in "${conflict_files[@]}"; do
        echo "- \`${local_path}\`"
      done
    fi
    echo
    echo "## Remaining Patch Queue"
    render_commit_list "${remaining_commits[@]}"
    echo
    echo "## Continue Locally"
    echo "1. Pull this candidate branch:"
    echo
    echo '   ```bash'
    echo "   gh pr checkout <pr-number> -R ${fork_repo}"
    echo "   # or"
    echo "   git fetch ${fork_remote} ${candidate_branch}"
    echo "   git switch -C ${candidate_branch} --track ${fork_remote}/${candidate_branch}"
    echo '   ```'
    echo
    echo "2. Fetch the upstream release tag:"
    echo
    echo '   ```bash'
    echo "   git fetch ${upstream_url} refs/tags/${upstream_tag}:refs/tags/${upstream_tag}"
    echo '   ```'
    echo
    echo "3. Resume the replay at the failing patch:"
    echo
    echo '   ```bash'
    echo "   git cherry-pick -x ${conflict_commit}"
    echo '   ```'
    echo
    echo "4. Resolve conflicts, then continue the cherry-pick:"
    echo
    echo '   ```bash'
    echo "   git status"
    echo "   git add <resolved-files>"
    echo "   git cherry-pick --continue"
    echo '   ```'
    echo
    if [[ ${#remaining_commits[@]} -gt 1 ]]; then
      echo "5. Replay the rest of the queue:"
      echo
      echo '   ```bash'
      for commit in "${remaining_commits[@]:1}"; do
        echo "   git cherry-pick -x ${commit}"
      done
      echo '   ```'
      echo
      echo "6. Remove this helper note and push the updated branch:"
    else
      echo "5. Remove this helper note and push the updated branch:"
    fi
    echo
    echo '   ```bash'
    echo "   git rm ${conflict_note_path}"
    echo "   git commit --amend --no-edit"
    echo "   git push ${fork_remote} HEAD:${candidate_branch}"
    echo '   ```'
  } > "${conflict_note_path}"

  git add "${conflict_note_path}"
  git commit -m "chore: record queue replay conflicts for ${upstream_tag}"
fi

body_file="$(mktemp)"
{
  echo "## Summary"
  echo "- replay queue patches from \`${fork_remote}/${queue_branch}\`"
  echo "- patch base ref: \`${fork_remote}/${queue_base_branch}\`"
  echo "- replay target upstream tag: \`${upstream_tag}\`"
  echo "- candidate branch: \`${candidate_branch}\`"
  echo
  echo "## Patch Queue"
  render_commit_list "${patch_commits[@]}"
  echo
  if [[ "${conflict_detected}" == "true" ]]; then
    echo "## Manual Conflict Resolution Required"
    echo "- failing patch:"
    git log -1 --format='  - `%h` %s' "${conflict_commit}"
    if [[ ${#conflict_files[@]} -eq 0 ]]; then
      echo "- conflicting files: Git did not report unmerged file paths."
    else
      echo "- conflicting files:"
      local_path=""
      for local_path in "${conflict_files[@]}"; do
        echo "  - \`${local_path}\`"
      done
    fi
    echo "- helper note committed at \`${conflict_note_path}\`"
  else
    echo "## Notes"
    echo "- queue replay strips stale \`${cargo_lock_path}\` hunks and refreshes the lockfile after replay"
    if [[ "${mode}" == "official" ]]; then
      echo "- ready for review against \`${queue_branch}\`"
      echo "- promotion should fast-forward both \`${queue_branch}\` and \`main\`"
    else
      echo "- rehearsal branch is ready for dry-run release validation"
      echo "- promotion is intentionally disabled in rehearsal mode"
    fi
  fi
} > "${body_file}"

if [[ "${push_branch}" == "true" ]]; then
  git push --force-with-lease "${fork_remote}" "${candidate_branch}"
fi

if [[ "${open_pr}" == "true" ]]; then
  existing_pr="$(
    gh pr list \
      --repo "${fork_repo}" \
      --base "${queue_branch}" \
      --head "${candidate_branch}" \
      --state open \
      --json number \
      --jq '.[0].number // ""'
  )"

  title="Replay ${upstream_tag} patch queue"
  create_args=()
  if [[ "${conflict_detected}" == "true" ]]; then
    title="${title} (manual conflict resolution required)"
    create_args+=(--draft)
  fi

  if [[ -n "${existing_pr}" ]]; then
    gh pr edit "${existing_pr}" --repo "${fork_repo}" --title "${title}" --body-file "${body_file}"
    if [[ "${conflict_detected}" == "true" ]]; then
      gh pr ready "${existing_pr}" --repo "${fork_repo}" --undo
    fi
  else
    gh pr create "${create_args[@]}" --repo "${fork_repo}" --base "${queue_branch}" --head "${candidate_branch}" --title "${title}" --body-file "${body_file}"
    existing_pr="$(
      gh pr list \
        --repo "${fork_repo}" \
        --base "${queue_branch}" \
        --head "${candidate_branch}" \
        --state open \
        --json number \
        --jq '.[0].number // ""'
    )"
  fi
fi

if [[ "${mode}" == "rehearsal" && "${conflict_detected}" != "true" && "${push_branch}" == "true" ]]; then
  if [[ -z "${effective_rehearsal_suffix}" ]]; then
    effective_rehearsal_suffix="${candidate_branch##*-}"
  fi
  rehearsal_tag="rehearsal-internal-${upstream_tag}-${effective_rehearsal_suffix}"
  tag_message_file="$(mktemp)"
  {
    echo "Rehearsal internal release for ${upstream_tag}"
    echo
    echo "mode=rehearsal"
    echo "candidate_branch=${candidate_branch}"
    echo "upstream_tag=${upstream_tag}"
    echo "source_head=$(git rev-parse HEAD)"
  } > "${tag_message_file}"
  git tag -a "${rehearsal_tag}" HEAD -F "${tag_message_file}"
  git push "${fork_remote}" "refs/tags/${rehearsal_tag}"
fi

if [[ "${conflict_detected}" == "true" ]]; then
  if [[ "${open_pr}" == "true" && -n "${existing_pr}" ]]; then
    comment_file="$(mktemp)"
    {
      echo "Automatic queue replay stopped while applying:"
      git log -1 --format='- `%h` %s' "${conflict_commit}"
      echo
      echo "Pull this PR locally with:"
      echo
      echo '```bash'
      echo "gh pr checkout ${existing_pr} -R ${fork_repo}"
      echo "# or"
      echo "git fetch ${fork_remote} ${candidate_branch}"
      echo "git switch -C ${candidate_branch} --track ${fork_remote}/${candidate_branch}"
      echo "git fetch ${upstream_url} refs/tags/${upstream_tag}:refs/tags/${upstream_tag}"
      echo "git cherry-pick -x ${conflict_commit}"
      echo '```'
      echo
      echo "Conflict details are committed in \`${conflict_note_path}\`."
    } > "${comment_file}"
    gh pr comment "${existing_pr}" --repo "${fork_repo}" --body-file "${comment_file}"
  fi

  echo "manual conflict resolution required on ${candidate_branch}" >&2
  exit 1
fi

if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  {
    echo "## Queue replay prepared"
    echo
    echo "- mode: \`${mode}\`"
    echo "- upstream tag: \`${upstream_tag}\`"
    echo "- candidate branch: \`${candidate_branch}\`"
    if [[ -n "${existing_pr}" ]]; then
      echo "- PR: #${existing_pr}"
    fi
    if [[ -n "${rehearsal_tag}" ]]; then
      echo "- rehearsal tag: \`${rehearsal_tag}\`"
    fi
  } >> "${GITHUB_STEP_SUMMARY}"
fi

echo "Prepared ${candidate_branch} from ${upstream_tag}"
