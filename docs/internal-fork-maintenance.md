# Internal Fork Maintenance

This document describes the stable-release sync flow for `SDGLBL/codex`.

## Branches

- `main`: current internal stable line. Every merge should correspond to one validated upstream stable release plus the internal patch stack.
- `patches/internal`: non-executing cold backup and audit line for fork-only deltas. The sync automation no longer replays it directly, but we keep it as an independent record of internal patches in case `main` ever drops one accidentally.
- `sync/rust-vX.Y.Z`: per-release integration branch created from fork `main`, then updated by merging the upstream `rust-vX.Y.Z` tag into that stable line.
- `archive/main-pre-sync-2026-03-13`: backup branch that preserves the pre-sync fork state before the first large resync.
- The patch stack should stay rooted at the current internal stable line.
- The sync helper computes patch commits relative to fork `main`, not the new upstream `rust-vX.Y.Z` tag.
- This avoids accidentally replaying unrelated upstream commits when release tags do not form a simple linear ancestry chain.

## Tags And Releases

- Upstream source of truth: `openai/codex` stable `rust-v*` releases.
- Fork-only internal release tags: `internal-rust-vX.Y.Z`.
- GitHub Release name: `X.Y.Z-internal`.
- Release notes must include both the upstream tag and the current patch stack commit SHAs.
- Standard path: merge `sync/rust-vX.Y.Z` into `main`, let the tag workflow create `internal-rust-vX.Y.Z`, then let `internal-rust-release.yml` publish the release.
- Manual fallback: if the auto-tag workflow is disabled or fails after the merge, create and push `internal-rust-vX.Y.Z` yourself to trigger the same release workflow.

## Automation

- `.github/workflows/track-upstream-stable.yml`
  - Runs every 4 hours or on manual dispatch.
  - Checks the latest stable upstream `rust-v*` release.
  - Dispatches `prepare-sync-pr.yml` when the fork does not yet have a matching internal release tag or an open sync PR.
- `.github/workflows/prepare-sync-pr.yml`
  - Creates or refreshes `sync/rust-vX.Y.Z`.
  - Starts from fork `main`, then merges the upstream release tag into that branch.
  - Includes the current `patches/internal` stack in the PR body for auditability, but it does not replay those commits one by one during sync prep.
  - Pushes the sync branch and opens or updates the corresponding pull request to `main`.
  - If the merge hits conflicts, it still pushes the sync branch, commits `SYNC_CONFLICTS.md`, opens or updates the PR, comments with pull instructions, and then fails the workflow so the conflict is visible in Actions.
- `.github/workflows/internal-rust-release.yml`
  - Supports `workflow_dispatch` for both dry-runs and explicit publish runs.
  - Publishes GitHub Releases for `internal-rust-v*` tag pushes or for manual dispatches that set `publish=true`.
  - Stages CLI binaries, proxy binaries, installer scripts, and `config.schema.json`.
- `.github/workflows/tag-internal-release-on-sync-merge.yml`
  - Runs when a `sync/rust-vX.Y.Z` PR is merged into `main`.
  - Creates `internal-rust-vX.Y.Z` on the merge commit.
  - Leaves the tag untouched if it already points at that same merge commit, and fails loudly if the tag already exists on a different commit.
  - Because tags created by `GITHUB_TOKEN` do not trigger downstream push workflows, it also explicitly dispatches `internal-rust-release.yml` with `publish=true`.

## Local Setup

- The automation assumes the fork is available as a git remote, but it does not require `origin` to point at the fork.
- If your local clone still has `origin` set to `openai/codex`, pass `FORK_REPO=SDGLBL/codex` and either:
  - `FORK_REMOTE=fork` after adding `git remote add fork https://github.com/SDGLBL/codex`
  - or `FORK_URL=https://github.com/SDGLBL/codex` to let the helper script create or update the remote on demand

## Manual `gh` Fallback

```bash
# Check the latest stable upstream rust release.
scripts/internal/latest_upstream_stable.sh

# Prepare the sync branch locally without pushing.
FORK_REPO=SDGLBL/codex \
FORK_REMOTE=fork \
PUSH_BRANCH=false \
OPEN_PR=false \
scripts/internal/prepare_sync_pr.sh rust-v0.114.0

# Dispatch the sync workflow remotely.
gh workflow run prepare-sync-pr.yml -R SDGLBL/codex -f upstream_tag=rust-v0.114.0

# Inspect the current sync pull request.
gh pr list -R SDGLBL/codex --base main --head sync/rust-v0.114.0

# Pull the sync PR locally for manual conflict resolution.
gh pr checkout <pr-number> -R SDGLBL/codex

# Trigger a dry-run build for the internal release workflow.
gh workflow run internal-rust-release.yml -R SDGLBL/codex -f upstream_tag=rust-v0.114.0 -f publish=false

# Manually publish an internal release for an existing internal tag.
gh workflow run internal-rust-release.yml -R SDGLBL/codex -f upstream_tag=rust-v0.114.0 -f publish=true

# Manual fallback if the auto-tag workflow did not create the release tag.
git tag internal-rust-v0.114.0 <merge-commit-sha>
git push fork internal-rust-v0.114.0
```

## Patch Stack Rules

- Treat `patches/internal` as a backup ledger, not as the executing source for sync automation.
- Keep the recorded internal deltas linear enough to inspect and recover from if `main` loses an internal change.
- Split product behavior changes from CI/docs changes when practical so the backup line stays readable.
- Avoid mixing upstream version bumps from `rust-vX.Y.Z` alignment into that backup line. Those belong to the release-sync branch, not to `patches/internal`.
- When you add new fork-only behavior directly on `main`, mirror it into `patches/internal` soon after so the cold-backup line stays useful.

## Conflict Resolution

- When the sync workflow reports a merge conflict, look for the `sync/rust-vX.Y.Z` PR and pull it locally with `gh pr checkout <pr-number> -R SDGLBL/codex`.
- The PR branch will include `SYNC_CONFLICTS.md` with the conflicting files and an exact command sequence for reproducing the upstream merge locally.
- The first local recovery commands are:
  ```bash
  gh pr checkout <pr-number> -R SDGLBL/codex
  git fetch https://github.com/openai/codex refs/tags/rust-vX.Y.Z:refs/tags/rust-vX.Y.Z
  git merge --no-ff rust-vX.Y.Z
  ```
- Resolve the conflicts, stage your fixes, remove `SYNC_CONFLICTS.md`, then finish with `git commit` and `git push`.
- After the sync PR is reviewed and merged into `main`, the tag workflow should automatically create `internal-rust-vX.Y.Z` and explicitly dispatch the release build.
- For sync PRs created by the older cherry-pick-based workflow, you may need one extra `git merge fork/main` after the replay is done so GitHub sees the PR branch as mergeable.

## Rollback

- If a sync PR turns out to be bad, close the PR and delete the `sync/rust-vX.Y.Z` branch.
- If a release tag is bad, delete the GitHub Release and the `internal-rust-vX.Y.Z` tag, fix `main`, and re-tag.
- Do not rewrite `archive/main-pre-sync-2026-03-13`; keep it as a fixed recovery point.
