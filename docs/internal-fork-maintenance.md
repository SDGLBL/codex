# Internal Fork Maintenance

This document describes the stable-release sync flow for `SDGLBL/codex`.

## Branches
- `main`: current internal stable line. Every merge should correspond to one validated upstream stable release plus the internal patch stack.
- `patches/internal`: long-lived linear patch stack. Keep it branch-only and rebase it when the internal feature set changes.
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
  - Supports manual dry-runs with `workflow_dispatch`.
  - Publishes GitHub Releases only for `internal-rust-v*` tag pushes.
  - Stages CLI binaries, proxy binaries, installer scripts, and `config.schema.json`.

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
gh workflow run internal-rust-release.yml -R SDGLBL/codex -f upstream_tag=rust-v0.114.0
```

## Patch Stack Rules
- Keep the patch stack linear and cherry-pick friendly.
- Split product behavior changes from CI/docs changes.
- Avoid mixing upstream version bumps from `rust-vX.Y.Z` alignment into the long-lived patch stack. Those belong to the release-sync branch, not to `patches/internal`.
- When you add new fork-only behavior directly on `main`, immediately mirror it into `patches/internal`. The sync workflow now merges from `main`, but `patches/internal` remains the long-lived audit trail for internal deltas.

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
- For sync PRs created by the older cherry-pick-based workflow, you may need one extra `git merge fork/main` after the replay is done so GitHub sees the PR branch as mergeable.

## Rollback
- If a sync PR turns out to be bad, close the PR and delete the `sync/rust-vX.Y.Z` branch.
- If a release tag is bad, delete the GitHub Release and the `internal-rust-vX.Y.Z` tag, fix `main`, and re-tag.
- Do not rewrite `archive/main-pre-sync-2026-03-13`; keep it as a fixed recovery point.
