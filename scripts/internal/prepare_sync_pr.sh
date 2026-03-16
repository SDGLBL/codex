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
conflict_note_path="${CONFLICT_NOTE_PATH:-SYNC_CONFLICTS.md}"

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

render_commit_list() {
  local commit
  for commit in "$@"; do
    git log -1 --format='- `%h` %s' "${commit}"
  done
}

existing_pr=""

conflict_detected=false
conflict_commit=""
conflict_subject=""
conflict_files=()
remaining_commits=()

git checkout -B "${sync_branch}" "${upstream_tag}"

for i in "${!patch_commits[@]}"; do
  commit="${patch_commits[$i]}"
  if git cherry-pick -x "${commit}"; then
    continue
  fi

  conflict_detected=true
  conflict_commit="${commit}"
  conflict_subject="$(git log -1 --format='%s' "${commit}")"
  mapfile -t conflict_files < <(git diff --name-only --diff-filter=U | sort -u)
  remaining_commits=("${patch_commits[@]:$i}")

  git cherry-pick --abort

  {
    echo "# Manual Sync Conflict Resolution"
    echo
    echo "Automatic sync from \`${upstream_tag}\` into \`${sync_branch}\` stopped before the first conflicting patch commit."
    echo
    echo "## First Conflicting Patch Commit"
    echo "- \`$(git rev-parse --short "${conflict_commit}")\` ${conflict_subject}"
    echo
    echo "## Conflicting Files"
    if [[ ${#conflict_files[@]} -eq 0 ]]; then
      echo "- Git reported a cherry-pick conflict, but no unmerged file paths were captured."
    else
      for path in "${conflict_files[@]}"; do
        echo "- \`${path}\`"
      done
    fi
    echo
    echo "## Remaining Patch Commits"
    render_commit_list "${remaining_commits[@]}"
    echo
    echo "## Continue Locally"
    echo "1. Check out this branch locally from the PR."
    echo "2. Start by replaying the first conflicting patch commit:"
    echo "   \`git cherry-pick -x ${conflict_commit}\`"
    echo "3. Resolve the files above, then run \`git cherry-pick --continue\`."
    if [[ ${#remaining_commits[@]} -gt 1 ]]; then
      echo "4. Cherry-pick the remaining patch commits in order:"
      for commit in "${remaining_commits[@]:1}"; do
        echo "   - \`git cherry-pick -x ${commit}\`"
      done
    fi
    echo "5. Delete this file before merging the PR."
  } > "${conflict_note_path}"

  git add "${conflict_note_path}"
  git commit -m "chore: record sync conflicts for ${upstream_tag}"
  break
done

if [[ "${conflict_detected}" != "true" ]]; then
  changed_paths="$(git diff --name-only HEAD~${#patch_commits[@]}..HEAD)"
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
fi

body_file="$(mktemp)"
{
  echo "## Summary"
  echo "- sync fork against upstream \`${upstream_tag}\`"
  echo "- cherry-pick the current internal patch stack from \`${patch_branch}\`"
  echo "- keep the fork release automation and internal release tag flow"
  echo
  echo "## Patch Stack"
  render_commit_list "${patch_commits[@]}"
  echo
  if [[ "${conflict_detected}" == "true" ]]; then
    echo "## Manual Conflict Resolution Required"
    echo "- sync branch: \`${sync_branch}\`"
    echo "- first conflicting patch commit: \`$(git rev-parse --short "${conflict_commit}")\` ${conflict_subject}"
    if [[ ${#conflict_files[@]} -eq 0 ]]; then
      echo "- conflicting files: Git did not report unmerged file paths."
    else
      echo "- conflicting files:"
      for path in "${conflict_files[@]}"; do
        echo "  - \`${path}\`"
      done
    fi
    echo "- helper note committed at \`${conflict_note_path}\`"
    echo "- pull this branch locally and start with \`git cherry-pick -x ${conflict_commit}\`"
  else
    echo "## Notes"
    echo "- branch name: \`${sync_branch}\`"
    echo "- internal release tag after merge: \`internal-${upstream_tag}\`"
  fi
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
  create_args=()
  if [[ "${conflict_detected}" == "true" ]]; then
    title="${title} (manual conflict resolution required)"
    create_args+=(--draft)
  fi
  if [[ -n "${existing_pr}" ]]; then
    gh pr edit "${existing_pr}" --repo "${fork_repo}" --title "${title}" --body-file "${body_file}"
  else
    gh pr create "${create_args[@]}" --repo "${fork_repo}" --base "${base_branch}" --head "${sync_branch}" --title "${title}" --body-file "${body_file}"
    existing_pr="$(
      gh pr list \
        --repo "${fork_repo}" \
        --base "${base_branch}" \
        --head "${sync_branch}" \
        --state open \
        --json number \
        --jq '.[0].number // ""'
    )"
  fi
fi

if [[ "${conflict_detected}" == "true" ]]; then
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    {
      echo "## Manual conflict resolution required"
      echo
      echo "- sync branch: \`${sync_branch}\`"
      echo "- first conflicting patch commit: \`$(git rev-parse --short "${conflict_commit}")\` ${conflict_subject}"
      echo "- note committed at \`${conflict_note_path}\`"
      if [[ -n "${existing_pr}" ]]; then
        echo "- PR: #${existing_pr}"
      fi
    } >> "${GITHUB_STEP_SUMMARY}"
  fi

  if [[ "${open_pr}" == "true" ]] && [[ -n "${existing_pr}" ]]; then
    comment_file="$(mktemp)"
    {
      echo "Automatic sync hit a cherry-pick conflict while applying \`${conflict_commit}\` to \`${upstream_tag}\`."
      echo
      echo "Pull this PR locally with:"
      echo
      echo '```bash'
      echo "gh pr checkout ${existing_pr} -R ${fork_repo}"
      echo "# or"
      echo "git fetch ${fork_remote} ${sync_branch}"
      echo "git switch -C ${sync_branch} --track ${fork_remote}/${sync_branch}"
      echo "git cherry-pick -x ${conflict_commit}"
      echo '```'
      echo
      echo "Conflict details are committed in \`${conflict_note_path}\`."
    } > "${comment_file}"
    gh pr comment "${existing_pr}" --repo "${fork_repo}" --body-file "${comment_file}"
  fi

  echo "manual conflict resolution required on ${sync_branch}" >&2
  exit 1
fi

echo "Prepared ${sync_branch} from ${upstream_tag}"
