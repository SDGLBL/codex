# Internal Fork Maintenance

This document describes the stable-release sync flow for `SDGLBL/codex`.

## Branches
- `main`: current internal stable line. Every merge should correspond to one validated upstream stable release plus the internal patch stack.
- `patches/internal`: long-lived linear patch stack. Keep it branch-only and rebase it when the internal feature set changes.
- `sync/rust-vX.Y.Z`: per-release integration branch created from the upstream `rust-vX.Y.Z` tag and populated by cherry-picking `patches/internal`.
- `archive/main-pre-sync-2026-03-13`: backup branch that preserves the pre-sync fork state before the first large resync.
- The patch stack should stay rooted at the upstream stable release lineage, and the sync helper computes patch commits relative to the target `rust-vX.Y.Z` tag by default.

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
  - Cherry-picks the full `patches/internal` stack onto the upstream release tag.
  - Pushes the sync branch and opens or updates the corresponding pull request to `main`.
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

# Trigger a dry-run build for the internal release workflow.
gh workflow run internal-rust-release.yml -R SDGLBL/codex -f upstream_tag=rust-v0.114.0
```

## Patch Stack Rules
- Keep the patch stack linear and cherry-pick friendly.
- Split product behavior changes from CI/docs changes.
- Avoid mixing upstream version bumps from `rust-vX.Y.Z` alignment into the long-lived patch stack. Those belong to the release-sync branch, not to `patches/internal`.

## Rollback
- If a sync PR turns out to be bad, close the PR and delete the `sync/rust-vX.Y.Z` branch.
- If a release tag is bad, delete the GitHub Release and the `internal-rust-vX.Y.Z` tag, fix `main`, and re-tag.
- Do not rewrite `archive/main-pre-sync-2026-03-13`; keep it as a fixed recovery point.
