#!/usr/bin/env bash

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <pr-number>" >&2
  exit 1
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "gh is required" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

pr_number="$1"
fork_remote="${FORK_REMOTE:-origin}"
fork_repo="${FORK_REPO:-}"
fork_url="${FORK_URL:-}"
queue_branch="${QUEUE_BRANCH:-queue/internal}"
queue_base_branch="${QUEUE_BASE_BRANCH:-queue/base/internal}"
main_branch="${MAIN_BRANCH:-main}"
upstream_remote="${UPSTREAM_REMOTE:-upstream}"
upstream_url="${UPSTREAM_URL:-https://github.com/openai/codex}"
push_token="${PUSH_TOKEN:-}"
require_approval="${REQUIRE_APPROVAL:-true}"
require_checks="${REQUIRE_CHECKS:-true}"
close_pr="${CLOSE_PR:-true}"

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

pr_json="$(
  gh pr view "${pr_number}" \
    --repo "${fork_repo}" \
    --json number,state,isDraft,baseRefName,headRefName,headRefOid,author,reviewDecision,title,url
)"

pr_state="$(jq -r '.state' <<<"${pr_json}")"
is_draft="$(jq -r '.isDraft' <<<"${pr_json}")"
base_ref="$(jq -r '.baseRefName' <<<"${pr_json}")"
head_ref="$(jq -r '.headRefName' <<<"${pr_json}")"
head_sha="$(jq -r '.headRefOid' <<<"${pr_json}")"
review_decision="$(jq -r '.reviewDecision // ""' <<<"${pr_json}")"

if [[ "${pr_state}" != "OPEN" ]]; then
  echo "PR #${pr_number} is not open" >&2
  exit 1
fi

if [[ "${is_draft}" == "true" ]]; then
  echo "PR #${pr_number} is still a draft" >&2
  exit 1
fi

if [[ "${base_ref}" != "${queue_branch}" ]]; then
  echo "PR #${pr_number} targets ${base_ref}; expected ${queue_branch}" >&2
  exit 1
fi

if [[ "${require_approval}" == "true" && "${review_decision}" != "APPROVED" ]]; then
  echo "PR #${pr_number} is not approved" >&2
  exit 1
fi

if [[ "${require_checks}" == "true" ]]; then
  checks_status=0
  checks_json="$(gh pr checks "${pr_number}" --repo "${fork_repo}" --json bucket,name,workflow 2>/dev/null)" || checks_status=$?
  if [[ ${checks_status} -ne 0 ]]; then
    if [[ ${checks_status} -eq 8 ]]; then
      echo "required checks are still pending for PR #${pr_number}" >&2
      exit 1
    fi
    echo "failed to inspect checks for PR #${pr_number}" >&2
    exit "${checks_status}"
  fi

  if [[ "$(jq 'length' <<<"${checks_json}")" -eq 0 ]]; then
    echo "PR #${pr_number} has no recorded checks" >&2
    exit 1
  fi

  failing_checks="$(
    jq -r '
      .[]
      | select(.bucket == "fail" or .bucket == "cancel")
      | "\(.workflow) / \(.name) [\(.bucket)]"
    ' <<<"${checks_json}"
  )"
  if [[ -n "${failing_checks}" ]]; then
    echo "failing checks prevent promotion:" >&2
    printf '%s\n' "${failing_checks}" >&2
    exit 1
  fi
fi

release_mode="patch"
upstream_tag=""
if [[ "${head_ref}" =~ ^candidate/queue/(rust-v[0-9]+\.[0-9]+\.[0-9]+(-(alpha|beta)(\.[0-9]+)?)?)$ ]]; then
  release_mode="release"
  upstream_tag="${BASH_REMATCH[1]}"
fi

if [[ "${head_ref}" =~ ^rehearsal/queue/ ]]; then
  echo "rehearsal branches are never promoted" >&2
  exit 1
fi

git fetch --quiet "${fork_remote}" "refs/heads/${queue_branch}:refs/remotes/${fork_remote}/${queue_branch}"
git fetch --quiet "${fork_remote}" "refs/heads/${main_branch}:refs/remotes/${fork_remote}/${main_branch}"
git fetch --quiet "${fork_remote}" "refs/heads/${head_ref}:refs/remotes/${fork_remote}/${head_ref}"

queue_remote_ref="refs/remotes/${fork_remote}/${queue_branch}"
main_remote_ref="refs/remotes/${fork_remote}/${main_branch}"
head_remote_ref="refs/remotes/${fork_remote}/${head_ref}"

if [[ "$(git rev-parse "${queue_remote_ref}")" != "$(git rev-parse "${main_remote_ref}")" ]]; then
  echo "${fork_remote}/${queue_branch} and ${fork_remote}/${main_branch} diverged; reconcile them before promoting more queue changes" >&2
  exit 1
fi

if [[ "$(git rev-parse "${head_remote_ref}")" != "${head_sha}" ]]; then
  echo "remote head for ${head_ref} moved while preparing promotion" >&2
  exit 1
fi

push_options=(--atomic)
push_refspecs=(
  "${head_sha}:refs/heads/${queue_branch}"
  "${head_sha}:refs/heads/${main_branch}"
)

queue_branch_sha="$(git rev-parse "${queue_remote_ref}")"
main_branch_sha="$(git rev-parse "${main_remote_ref}")"

if [[ "${release_mode}" == "release" ]]; then
  # Replay candidates are rebased onto the new upstream release tag, so promotion
  # intentionally rewrites the queue refs to the validated replay head.
  push_options+=(
    "--force-with-lease=refs/heads/${queue_branch}:${queue_branch_sha}"
    "--force-with-lease=refs/heads/${main_branch}:${main_branch_sha}"
  )
else
  if ! git merge-base --is-ancestor "${queue_remote_ref}" "${head_sha}"; then
    echo "${head_sha} does not fast-forward ${queue_branch}" >&2
    exit 1
  fi

  if ! git merge-base --is-ancestor "${main_remote_ref}" "${head_sha}"; then
    echo "${head_sha} does not fast-forward ${main_branch}" >&2
    exit 1
  fi
fi

release_summary=""
if [[ "${release_mode}" == "release" ]]; then
  git fetch --quiet "${fork_remote}" "refs/heads/${queue_base_branch}:refs/remotes/${fork_remote}/${queue_base_branch}" || {
    echo "missing ${fork_remote}/${queue_base_branch}; bootstrap queue refs before promoting release replays" >&2
    exit 1
  }
  git fetch --quiet "${upstream_remote}" "refs/tags/${upstream_tag}:refs/tags/${upstream_tag}"

  queue_base_remote_ref="refs/remotes/${fork_remote}/${queue_base_branch}"
  queue_base_sha="$(git rev-parse "${queue_base_remote_ref}")"
  push_options+=("--force-with-lease=refs/heads/${queue_base_branch}:${queue_base_sha}")
  push_refspecs+=("${upstream_tag}:refs/heads/${queue_base_branch}")

  internal_tag="internal-${upstream_tag}"
  tag_exists=false
  if git fetch --quiet "${fork_remote}" "refs/tags/${internal_tag}:refs/tags/${internal_tag}"; then
    existing_tag_sha="$(git rev-list -n1 "${internal_tag}")"
    if [[ "${existing_tag_sha}" != "${head_sha}" ]]; then
      echo "tag ${internal_tag} already exists at ${existing_tag_sha}, expected ${head_sha}" >&2
      exit 1
    fi
    tag_exists=true
  fi

  if [[ "${tag_exists}" != "true" ]]; then
    git tag -a "${internal_tag}" "${head_sha}" -m "Internal release for ${upstream_tag}"
    push_refspecs+=("refs/tags/${internal_tag}")
  fi

  release_summary=" and updated ${queue_base_branch} to ${upstream_tag}"
fi

git push "${push_options[@]}" "${fork_remote}" "${push_refspecs[@]}"

comment_file="$(mktemp)"
{
  echo "Promoted \`${head_sha}\` to \`${queue_branch}\` and \`${main_branch}\`${release_summary}."
  if [[ "${release_mode}" == "release" ]]; then
    echo
    echo "Official release tag \`internal-${upstream_tag}\` now points at the promoted queue head."
  fi
  echo
  echo "This PR was reviewed as a queue promotion artifact and then closed after promotion."
} > "${comment_file}"

gh pr comment "${pr_number}" --repo "${fork_repo}" --body-file "${comment_file}"

if [[ "${close_pr}" == "true" ]]; then
  gh pr close "${pr_number}" --repo "${fork_repo}"
fi

if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  {
    echo "## Queue promotion completed"
    echo
    echo "- PR: #${pr_number}"
    echo "- head: \`${head_sha}\`"
    echo "- promoted refs: \`${queue_branch}\`, \`${main_branch}\`"
    if [[ "${release_mode}" == "release" ]]; then
      echo "- updated base ref: \`${queue_base_branch}\` -> \`${upstream_tag}\`"
      echo "- official tag: \`internal-${upstream_tag}\`"
    fi
  } >> "${GITHUB_STEP_SUMMARY}"
fi

echo "Promoted PR #${pr_number} (${head_ref})"
