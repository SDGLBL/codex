# Codex-Driven Internal Rust Release Runbook

This runbook is the source of truth for internal Rust releases in `SDGLBL/codex`.

The flow is intentionally minimal:

- keep tracking upstream with `candidate/queue/rust-vX.Y.Z` branches
- trigger `.github/workflows/internal-rust-release.yml` manually via `workflow_dispatch`
- keep custom install scripts in release assets (`scripts/install/install.sh`, `scripts/install/install.ps1`)

No automatic upstream tracking, queue promotion, or tag-trigger-only flow is required.

## 1. Preconditions

Before you start:

1. `gh auth status` is healthy for `SDGLBL/codex`.
2. Local remotes are configured:
   - `origin` -> `SDGLBL/codex`
   - `upstream` -> `openai/codex`
3. Working tree is clean.

## 2. Branch Contract

- Upstream source tag format: `rust-vX.Y.Z`.
- Candidate branch format: `candidate/queue/rust-vX.Y.Z`.
- Internal release tag format: `internal-rust-vX.Y.Z`.

`candidate/queue/rust-vX.Y.Z` is the only required tracking branch for release follow-up.

## 3. Follow Upstream And Create Candidate

Replace `rust-vX.Y.Z` below with the target release, for example `rust-v0.125.0`.

### Step A: Fetch and preflight

```bash
git fetch origin
git fetch upstream --tags
gh release view rust-vX.Y.Z --repo openai/codex
```

### Step B: Create candidate from upstream tag

```bash
git switch -C candidate/queue/rust-vX.Y.Z rust-vX.Y.Z
```

## 4. Port Fork Patches (Semantic Alignment First)

Use the previous candidate branch as the default patch source, then replay fork patches onto the new candidate.

```bash
git for-each-ref --sort=-committerdate --format='%(refname:short)' refs/remotes/origin/candidate/queue/rust-v*
```

Pick the previous released candidate (example: `origin/candidate/queue/rust-v0.124.0`):

```bash
prev_candidate="origin/candidate/queue/rust-v0.124.0"
prev_upstream_tag="${prev_candidate#origin/candidate/queue/}"

mapfile -t patch_commits < <(
  git rev-list --reverse --no-merges "${prev_upstream_tag}..${prev_candidate}"
)

for commit in "${patch_commits[@]}"; do
  git cherry-pick -x "${commit}"
done
```

Conflict policy:

- preserve fork behavior, adapt implementation to upstream changes
- fixed commit hashes are not required
- if cherry-pick cannot be cleanly continued, hand-port the change and commit it in a clear, reviewable form

## 5. Validate And Publish Candidate

At minimum:

```bash
git diff --check
```

Run relevant formatting/tests for touched crates per repository policy, then push:

```bash
git push -u origin candidate/queue/rust-vX.Y.Z
```

## 6. Trigger Internal Release Action Directly

Publish release:

```bash
gh workflow run internal-rust-release.yml -R SDGLBL/codex -f upstream_tag=rust-vX.Y.Z -f release_ref=candidate/queue/rust-vX.Y.Z -f internal_tag=internal-rust-vX.Y.Z -f publish=true
```

Dry-run bundle only:

```bash
gh workflow run internal-rust-release.yml -R SDGLBL/codex -f upstream_tag=rust-vX.Y.Z -f release_ref=candidate/queue/rust-vX.Y.Z -f internal_tag=internal-rust-vX.Y.Z-dryrun -f publish=false
```

## 7. Verify Output

For published releases:

```bash
gh release view internal-rust-vX.Y.Z --repo SDGLBL/codex
```

Confirm:

- release exists with expected tag/version
- release notes show upstream tag and patch stack
- release assets include internal binaries, `config.schema.json`, `install.sh`, `install.ps1`, and `rg` bundles

For dry-run releases, validate uploaded `internal-release-dry-run-*` artifacts in the workflow run.

## 8. Corrective Release

If the follow-up patch port is wrong, fix the candidate branch and run workflow dispatch again with a new internal tag.

No `queue/internal`, `queue/base/internal`, or `main` promotion step is required in this runbook.
