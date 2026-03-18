# Internal Fork Maintenance

This document describes the patch-queue-first internal release flow for `SDGLBL/codex`.

## Branches

- `queue/internal`: the only authoritative fork patch queue. It must stay linear and contain only fork-only patch commits on top of the current upstream stable base.
- `queue/base/internal`: movable base ref for the current queue. It should point at the upstream stable `rust-v*` tag that `queue/internal` currently replays onto.
- `main`: publish mirror for `queue/internal`. Promotion fast-forwards or force-updates `main` to the validated queue head; direct development on `main` is no longer part of the flow.
- `candidate/queue/rust-vX.Y.Z`: official replay candidate branch created from the upstream stable tag and populated by cherry-picking the current queue patch stack.
- `rehearsal/queue/rust-vX.Y.Z-<suffix>`: isolated replay branch used to validate the workflow against an existing upstream release without updating official refs.

Historical refs such as `patches/internal` and `sync/rust-*` are not part of this flow. If they still exist on the remote, archive their current heads first and then delete them; no queue-first automation reads them.

## Tags And Releases

- Upstream source of truth: `openai/codex` stable `rust-v*` releases.
- Official internal release tags: `internal-rust-vX.Y.Z`.
- Rehearsal validation tags: `rehearsal-internal-rust-vX.Y.Z-<suffix>`.
- GitHub Release name: `X.Y.Z-internal`.
- Release notes are generated from the replayed patch queue relative to the upstream tag.

## Required Repository Setup

- Store a bot or PAT token with branch bypass rights in `INTERNAL_QUEUE_PAT`.
- Protect `main`, `queue/internal`, and `queue/base/internal` outside the repo settings so only the bot can update them directly.
- Keep `merge`, `rebase`, and `squash` UI merge actions disabled by policy for queue PRs; promotion must happen through the queue promotion workflow.

## Automation

- `.github/workflows/bootstrap-queue-refs.yml`
  - One-time cutover workflow that reconstructs `queue/internal` from an upstream tag and a chosen patch list, then creates `queue/base/internal`.
  - Replayed commits drop stale `codex-rs/Cargo.lock` hunks and refresh the lockfile against the new base before publishing refs.
  - Can optionally rewrite `main` so it matches the bootstrapped queue head.
- `.github/workflows/track-upstream-stable.yml`
  - Runs every 4 hours or on manual dispatch.
  - Checks the latest stable upstream `rust-v*` release.
  - Dispatches `prepare-queue-pr.yml` in `official` mode when there is no matching `internal-rust-v*` tag and no open candidate PR.
- `.github/workflows/prepare-queue-pr.yml`
  - `official` mode creates or refreshes `candidate/queue/rust-vX.Y.Z`.
  - Replays `queue/base/internal..queue/internal` onto the upstream tag with `cherry-pick -x`.
  - Drops replayed `codex-rs/Cargo.lock` hunks and regenerates the lockfile after replay so queue commits stay base-agnostic across releases.
  - Uses `INTERNAL_QUEUE_PAT` for pushes so rehearsal and official tags can trigger downstream release workflows.
  - Opens or updates a PR to `queue/internal` for review.
  - `rehearsal` mode creates `rehearsal/queue/...` plus a `rehearsal-internal-rust-v...` tag for dry-run validation.
- `.github/workflows/promote-queue-pr.yml`
  - Validates PR approval and checks.
  - Fast-forwards `queue/internal` and `main` to the PR head.
  - For replay candidates, also updates `queue/base/internal` to the new upstream tag and creates `internal-rust-vX.Y.Z`.
  - Closes the PR after promotion instead of creating a GitHub merge commit.
- `.github/workflows/internal-rust-release.yml`
  - Builds official releases from `internal-rust-v*` tag pushes.
  - Builds rehearsal bundles from `rehearsal-internal-rust-v*` tag pushes without publishing a GitHub Release.
  - Still supports manual `workflow_dispatch` dry-runs against any branch or tag ref.

## Bootstrap

Bootstrap is the only time `main` may need to be rewritten to adopt the queue history.

```bash
gh workflow run bootstrap-queue-refs.yml -R SDGLBL/codex \
  -f upstream_tag=rust-v0.115.0 \
  -f source_branch=main \
  -f promote_main=true
```

If the current `main` contains merge artifacts you do not want in the queue, pass an explicit patch list:

```bash
gh workflow run bootstrap-queue-refs.yml -R SDGLBL/codex \
  -f upstream_tag=rust-v0.115.0 \
  -f patch_commits="commit1 commit2 commit3" \
  -f promote_main=true
```

## Day-To-Day Fork Patch Flow

- Start new fork-only work from `queue/internal`, not from `main`.
- Open PRs back to `queue/internal`.
- After review and CI succeed, promote them with:

```bash
gh workflow run promote-queue-pr.yml -R SDGLBL/codex -f pr_number=<pr-number>
```

- The promotion workflow updates both `queue/internal` and `main` to the PR head, then closes the PR.

## Upstream Replay Flow

- Check the latest stable upstream rust release:

```bash
scripts/internal/latest_upstream_stable.sh
```

- Prepare the official replay candidate locally without pushing:

```bash
FORK_REPO=SDGLBL/codex \
FORK_REMOTE=fork \
PUSH_BRANCH=false \
OPEN_PR=false \
scripts/internal/prepare_queue_pr.sh rust-v0.116.0
```

- Dispatch the official replay remotely:

```bash
gh workflow run prepare-queue-pr.yml -R SDGLBL/codex \
  -f upstream_tag=rust-v0.116.0 \
  -f mode=official
```

- Inspect the open replay PR:

```bash
gh pr list -R SDGLBL/codex --base queue/internal --head candidate/queue/rust-v0.116.0
```

- After review and checks succeed, promote it:

```bash
gh workflow run promote-queue-pr.yml -R SDGLBL/codex -f pr_number=<pr-number>
```

## Rehearsal Validation

Rehearsal lets you validate the replay and build pipeline even when upstream has not shipped a new stable release yet.

```bash
gh workflow run prepare-queue-pr.yml -R SDGLBL/codex \
  -f upstream_tag=rust-v0.115.0 \
  -f mode=rehearsal
```

That creates:

- `rehearsal/queue/rust-v0.115.0-<suffix>`
- `rehearsal-internal-rust-v0.115.0-<suffix>`

The rehearsal tag triggers `internal-rust-release.yml`, which uploads a dry-run artifact bundle instead of publishing a GitHub Release.

## Conflict Resolution

- If replay hits conflicts, the candidate branch is still pushed and includes `QUEUE_REPLAY_CONFLICTS.md`.
- Pull the branch locally with:

  ```bash
  gh pr checkout <pr-number> -R SDGLBL/codex
  git fetch https://github.com/openai/codex refs/tags/rust-vX.Y.Z:refs/tags/rust-vX.Y.Z
  git cherry-pick -x <failing-commit>
  ```

- Resolve conflicts, continue the cherry-pick, remove `QUEUE_REPLAY_CONFLICTS.md`, then push the branch back to the PR.
- If replay requires extra fork-side fixes, commit them on the candidate branch before promotion so they become part of the canonical queue history.

## Rollback

- If a queue PR is bad before promotion, close it and delete the candidate or rehearsal branch.
- If a promoted release is bad, fix the queue with a new queue PR and cut a new official internal tag.
- If you must revert the patch-queue cutover itself, restore `main`, `queue/internal`, and `queue/base/internal` from known-good refs together; do not update only one of them.
