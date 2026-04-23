# Codex-Driven Internal Rust Release Playbook

This is the single source of truth for following upstream Rust releases in `SDGLBL/codex`.

The release process is intentionally manual and Codex-driven:

- no automatic upstream tracking
- no automatic replay PR creation
- no automatic queue promotion
- no replay helper scripts

## 1. Preconditions

Before you start:

1. You have push rights to `queue/internal`, `queue/base/internal`, and `main`.
2. `gh auth status` is healthy for the `SDGLBL/codex` repo.
3. Local remotes are configured:
   - `origin` -> `SDGLBL/codex`
   - `upstream` -> `openai/codex`
4. The working tree is clean.
5. You understand current canonical patch stack policy:
   - preserve commit `9f3577ad0b` (`wire session id`)
   - preserve commit `071236452b` (`model max output tokens`)
   - preserve commit `706e10f586` (`fork sync + internal release automation baseline`)
   - keep all later fork follow-up changes squashed into a single commit

## 2. Branch And Tag Contract

- `queue/base/internal` points to the upstream stable base tag currently tracked.
- `queue/internal` is the fork patch queue head.
- `main` mirrors `queue/internal`.
- Candidate branch format: `candidate/queue/rust-vX.Y.Z`.
- Internal release tag format: `internal-rust-vX.Y.Z`.

`queue/internal` must stay linear and replayable by cherry-picking onto a new upstream stable tag.

## 3. Manual Follow Procedure (Upstream `rust-vX.Y.Z`)

Replace `rust-vX.Y.Z` below with the target tag, for example `rust-v0.122.0`.

### Step A: Discover and preflight

```bash
git fetch origin
git fetch upstream --tags
gh release view rust-vX.Y.Z --repo openai/codex
```

Confirm:

- target upstream tag exists and is stable
- no unrelated local edits

### Step B: Build a fresh candidate branch from upstream tag

```bash
git switch -C candidate/queue/rust-vX.Y.Z rust-vX.Y.Z
```

### Step C: Replay canonical patch stack

Get current queue patch commits:

```bash
git rev-list --reverse --no-merges origin/queue/base/internal..origin/queue/internal
```

Replay in order:

```bash
git cherry-pick -x <commit-1>
git cherry-pick -x <commit-2>
# ...
```

Conflict policy:

- preserve fork behavior, adapt to upstream structure
- do not keep conflict-note helper commits (for example commits that only touch `QUEUE_REPLAY_CONFLICTS.md`)
- keep the patch queue semantically clean and replayable

### Step D: Validate locally

At minimum:

```bash
git diff --check
```

Then run relevant checks/tests for changed Rust crates. If Rust source changed, run formatting and scoped tests per repository policy.

### Step E: Publish candidate branch

```bash
git push -u origin candidate/queue/rust-vX.Y.Z
```

### Step F: Promote refs manually

After candidate push:

```bash
candidate_sha="$(git rev-parse origin/candidate/queue/rust-vX.Y.Z)"
upstream_sha="$(git rev-parse rust-vX.Y.Z^{})"

queue_sha="$(git rev-parse origin/queue/internal)"
main_sha="$(git rev-parse origin/main)"
base_sha="$(git rev-parse origin/queue/base/internal)"

git push --atomic origin \
  --force-with-lease=refs/heads/queue/internal:${queue_sha} \
  --force-with-lease=refs/heads/main:${main_sha} \
  --force-with-lease=refs/heads/queue/base/internal:${base_sha} \
  "${candidate_sha}:refs/heads/queue/internal" \
  "${candidate_sha}:refs/heads/main" \
  "${upstream_sha}:refs/heads/queue/base/internal"
```

### Step G: Tag and trigger internal release

```bash
git tag -a internal-rust-vX.Y.Z "${candidate_sha}" -m "Internal release for rust-vX.Y.Z"
git push origin "refs/tags/internal-rust-vX.Y.Z"
```

The tag push triggers `.github/workflows/internal-rust-release.yml`.

You can also dispatch manually:

```bash
gh workflow run internal-rust-release.yml -R SDGLBL/codex \
  -f upstream_tag=rust-vX.Y.Z \
  -f release_ref=candidate/queue/rust-vX.Y.Z \
  -f internal_tag=internal-rust-vX.Y.Z \
  -f publish=true
```

### Step H: Verify release output

```bash
gh release view internal-rust-vX.Y.Z --repo SDGLBL/codex
```

Confirm:

- release exists
- notes include patch stack from `rust-vX.Y.Z..candidate`
- expected assets uploaded

## 4. Updating The Canonical Patch Stack

When fork follow-up changes are needed:

1. Keep the first three preserved commits unchanged.
2. Add or refresh one squashed follow-up commit for all later fork-specific maintenance.
3. Ensure `queue/internal` remains linear.
4. Verify replay onto the latest upstream stable tag before tagging internal release.

## 5. Rollback

If a follow fails after promotion:

1. Create a corrective candidate from the same upstream tag.
2. Replay corrected patch stack.
3. Re-promote `queue/internal`/`main` and keep `queue/base/internal` on the same upstream tag.
4. Cut a new internal tag with the corrected head.

If promotion itself was wrong, restore all three refs together (`queue/internal`, `main`, `queue/base/internal`) from known-good SHAs; never restore only one ref.

## 6. Removed Automation (Intentional)

The following were removed and must not be reintroduced as default flow:

- workflows: `track-upstream-releases`, `prepare-queue-pr`, `promote-queue-pr`, `bootstrap-queue-refs`
- scripts: `prepare_queue_pr`, `promote_queue_pr`, `bootstrap_queue_refs`, `latest_upstream_release`, `patch_stack_commits`

Codex + this playbook is the required operational path.
