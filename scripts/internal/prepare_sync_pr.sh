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
# The patch stack should be computed against the current internal stable line,
# not the target upstream tag. Upstream release tags do not necessarily form a
# linear ancestry chain, so comparing against the new tag can accidentally pull
# in unrelated upstream commits as "patches".
patch_base_ref="${PATCH_BASE_REF:-${fork_remote}/${base_branch}}"
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

render_commit_list() {
  if [[ $# -eq 0 ]]; then
    echo "- No commits are currently unique to \`${patch_branch}\`."
    return
  fi

  local commit
  for commit in "$@"; do
    git log -1 --format='- `%h` %s' "${commit}"
  done
}

existing_pr=""

conflict_detected=false
conflict_subject=""
conflict_files=()
merge_summary="merge ${upstream_tag} into ${fork_remote}/${base_branch}"

git checkout -B "${sync_branch}" "${fork_remote}/${base_branch}"

if ! git merge --no-ff --no-commit "${upstream_tag}"; then
  conflict_detected=true
  conflict_subject="${merge_summary}"
  mapfile -t conflict_files < <(git diff --name-only --diff-filter=U | sort -u)

  git merge --abort

  {
    echo "# Manual Sync Conflict Resolution"
    echo
    echo "Automatic sync could not complete \`${merge_summary}\`."
    echo
    echo "This branch already starts from the current internal stable line, so once you finish the same merge locally and push the result, the PR should become mergeable."
    echo
    echo "## Conflicting Files"
    if [[ ${#conflict_files[@]} -eq 0 ]]; then
      echo "- Git reported a merge conflict, but no unmerged file paths were captured."
    else
      for path in "${conflict_files[@]}"; do
        echo "- \`${path}\`"
      done
    fi
    echo
    echo "## Current Internal Patch Stack"
    render_commit_list "${patch_commits[@]}"
    echo
    echo "## Continue Locally"
    echo "1. Pull this sync branch locally:"
    echo
    echo '   ```bash'
    echo "   gh pr checkout <pr-number> -R ${fork_repo}"
    echo "   # or"
    echo "   git fetch ${fork_remote} ${sync_branch}"
    echo "   git switch -C ${sync_branch} --track ${fork_remote}/${sync_branch}"
    echo '   ```'
    echo
    echo "2. Fetch the upstream release tag into your local clone:"
    echo
    echo '   ```bash'
    echo "   git fetch ${upstream_url} refs/tags/${upstream_tag}:refs/tags/${upstream_tag}"
    echo '   ```'
    echo
    echo "3. Re-run the upstream merge locally:"
    echo
    echo '   ```bash'
    echo "   git merge --no-ff ${upstream_tag}"
    echo '   ```'
    echo
    echo "4. Resolve the files above, then stage your fixes:"
    echo
    echo '   ```bash'
    echo "   git status"
    echo "   git add <resolved-files>"
    echo '   ```'
    echo
    echo "5. Remove this helper note before finishing the merge:"
    echo
    echo '   ```bash'
    echo "   git rm ${conflict_note_path}"
    echo '   ```'
    echo
    echo "6. Finish the merge and push the branch back to the PR:"
    echo
    echo '   ```bash'
    echo "   git commit"
    echo "   git push ${fork_remote} HEAD:${sync_branch}"
    echo '   ```'
    echo
    echo "If Git says the merge produced no changes, double-check that you are on \`${sync_branch}\` and that the branch tip matches the PR head before retrying."
  } > "${conflict_note_path}"

  git add "${conflict_note_path}"
  git commit -m "chore: record sync conflicts for ${upstream_tag}"
fi

if [[ "${conflict_detected}" != "true" ]]; then
  changed_paths="$(git diff --name-only --cached)"
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
    fi
  fi

  git commit -m "chore: sync ${upstream_tag} into ${base_branch}"
fi

body_file="$(mktemp)"
{
  echo "## Summary"
  echo "- branch starts from the current internal stable line: \`${fork_remote}/${base_branch}\`"
  echo "- merge upstream release \`${upstream_tag}\` into that stable line"
  echo "- keep the fork release automation and internal release tag flow"
  echo
  echo "## Patch Stack"
  render_commit_list "${patch_commits[@]}"
  echo
  if [[ "${conflict_detected}" == "true" ]]; then
    echo "## Manual Conflict Resolution Required"
    echo "- sync branch: \`${sync_branch}\`"
    echo "- blocked operation: \`${conflict_subject}\`"
    if [[ ${#conflict_files[@]} -eq 0 ]]; then
      echo "- conflicting files: Git did not report unmerged file paths."
    else
      echo "- conflicting files:"
      for path in "${conflict_files[@]}"; do
        echo "  - \`${path}\`"
      done
    fi
    echo "- helper note committed at \`${conflict_note_path}\`"
    echo "- pull this branch locally, fetch \`${upstream_tag}\`, then run \`git merge --no-ff ${upstream_tag}\`"
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
    if [[ "${conflict_detected}" == "true" ]]; then
      gh pr ready "${existing_pr}" --repo "${fork_repo}" --undo
    fi
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
      echo "- blocked operation: \`${conflict_subject}\`"
      echo "- note committed at \`${conflict_note_path}\`"
      if [[ -n "${existing_pr}" ]]; then
        echo "- PR: #${existing_pr}"
      fi
    } >> "${GITHUB_STEP_SUMMARY}"
  fi

  if [[ "${open_pr}" == "true" ]] && [[ -n "${existing_pr}" ]]; then
    comment_file="$(mktemp)"
    {
      echo "Automatic sync could not complete \`${merge_summary}\`."
      echo
      echo "Pull this PR locally with:"
      echo
      echo '```bash'
      echo "gh pr checkout ${existing_pr} -R ${fork_repo}"
      echo "# or"
      echo "git fetch ${fork_remote} ${sync_branch}"
      echo "git switch -C ${sync_branch} --track ${fork_remote}/${sync_branch}"
      echo "git fetch ${upstream_url} refs/tags/${upstream_tag}:refs/tags/${upstream_tag}"
      echo "git merge --no-ff ${upstream_tag}"
      echo '```'
      echo
      echo "After resolving the merge, run \`git rm ${conflict_note_path}\`, finish with \`git commit\`, and push back to \`${sync_branch}\`."
      echo
      echo "Conflict details are committed in \`${conflict_note_path}\`."
    } > "${comment_file}"
    gh pr comment "${existing_pr}" --repo "${fork_repo}" --body-file "${comment_file}"
  fi

  echo "manual conflict resolution required on ${sync_branch}" >&2
  exit 1
fi

echo "Prepared ${sync_branch} from ${upstream_tag}"
