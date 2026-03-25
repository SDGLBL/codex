# Internal Conflict Runbook

This runbook is the concrete "start from zero and recover the replay" guide for the queue-first internal fork flow.

It complements [docs/internal-fork-maintenance.md](./internal-fork-maintenance.md), which defines the steady-state workflow. This file focuses on the preserved `rust-v0.115.0` conflict case and on the exact recovery procedure to use when a future replay stops on conflicts. The same recovery pattern applies to later stable tags; substitute the target upstream tag and candidate branch names as needed.

## Preserved Refs

The repository intentionally keeps a few refs around so the next person can restart the investigation without reconstructing history by hand.

- Failed replay branch:
  `rehearsal/queue/rust-v0.115.0-main-20260318`
  Contains `QUEUE_REPLAY_CONFLICTS.md`, which records the failing commit, the conflict file list, and the remaining queue.
- First manually resolved replay:
  `rehearsal/queue/rust-v0.115.0-gh-20260317192432`
  This is the smallest successful replay of the original three-patch queue onto `rust-v0.115.0`.
- Final cutover rehearsal:
  `rehearsal/queue/rust-v0.115.0-cutover-20260318`
  This is the replay branch that matched the then-current `main` tree during queue-first cutover validation.
- Cutover bootstrap branch:
  `bootstrap/queue-cutover-rust-v0.115.0-2026-03-18`
  This is the linearized queue history that was used to align `main` and `queue/internal`.
- Pre-cutover archives:
  `archive/main-pre-queue-cutover-2026-03-18`
  `archive/queue-internal-pre-cutover-2026-03-18`
  `archive/queue-base-internal-pre-cutover-2026-03-18`
  These are rollback anchors for the old remote state.

## What Failed In The Preserved `v0.115.0` Case

The failed replay stopped at:

- `cdd5197e3` `feat: preserve wire session ids across resumed forks`

The preserved conflict note on `rehearsal/queue/rust-v0.115.0-main-20260318` lists these files:

- `codex-rs/core/src/client.rs`
- `codex-rs/core/src/personality_migration.rs`
- `codex-rs/core/src/realtime_conversation.rs`
- `codex-rs/core/src/rollout/metadata.rs`
- `codex-rs/core/src/rollout/recorder.rs`
- `codex-rs/core/tests/suite/subagent_notifications.rs`

The remaining queue at that point was:

- `cdd5197e3` `feat: preserve wire session ids across resumed forks`
- `b2d3d872a` `feat: allow model max output token overrides`
- `a643f7800` `chore: maintain fork sync and internal release automation`

## Quick Restart From Zero

When a replay fails again in the future, use this exact sequence. For later tags, replace `rust-v0.115.0` and the rehearsal branch name with the target `rust-vX.Y.Z` you are recovering:

```bash
git fetch origin
git fetch https://github.com/openai/codex refs/tags/rust-v0.115.0:refs/tags/rust-v0.115.0

git switch -C replay-debug --detach rust-v0.115.0
git fetch origin rehearsal/queue/rust-v0.115.0-main-20260318
git switch -C rehearsal/queue/rust-v0.115.0-main-20260318 \
  --track origin/rehearsal/queue/rust-v0.115.0-main-20260318

git show HEAD:QUEUE_REPLAY_CONFLICTS.md
git cherry-pick -x cdd5197e30820cb82c8228928480f8611b5b78b7
```

Then:

```bash
git status
git add <resolved-files>
git cherry-pick --continue
git cherry-pick -x b2d3d872ade33f280f16d66686806c0ab263f06d
git cherry-pick -x a643f7800720587dcfd11fbfb4782ab67c492adf
git rm QUEUE_REPLAY_CONFLICTS.md
git commit --amend --no-edit
```

If you want to compare against a known-good replay while resolving:

```bash
git diff rehearsal/queue/rust-v0.115.0-gh-20260317192432 -- <path>
git show rehearsal/queue/rust-v0.115.0-gh-20260317192432:<path>
```

## Resolution Principles For The Preserved `v0.115.0` Case

Do not mechanically choose "ours" or "theirs". The important part is to preserve the patch semantics while keeping the `rust-v0.115.0` code structure.

### `codex-rs/core/src/client.rs`

- Keep upstream `rust-v0.115.0` behavior that writes `x-client-request-id`.
- Preserve the fork patch intent that session-routing headers come from `wire_session_id`, not from the display conversation id.
- If both headers appear in a conflict, keep both semantics:
  upstream request id behavior
  fork wire-session header behavior

### `codex-rs/core/src/personality_migration.rs`

- The conflict here was structural, not conceptual.
- Preserve the fork intent that persisted session metadata should carry a real `wire_session_id`.
- On `rust-v0.115.0`, the compatible shape is to keep the newer test/file layout and set `wire_session_id` explicitly in test data where needed.

### `codex-rs/core/src/realtime_conversation.rs`

- Preserve the fork session identity semantics.
- Avoid reverting unrelated upstream `rust-v0.115.0` changes around conversation handling.

### `codex-rs/core/src/rollout/metadata.rs`

- Keep the upstream file layout.
- Preserve the fork behavior that rollout/session metadata extraction sees a populated `wire_session_id`.
- In practice this meant adjusting the test/session-meta fixtures instead of trying to reintroduce an older code shape.

### `codex-rs/core/src/rollout/recorder.rs`

- Preserve the recorder changes that propagate `wire_session_id` through persisted rollout metadata.
- Prefer small semantic edits over structural rollback.

### `codex-rs/core/tests/suite/subagent_notifications.rs`

- This conflict was mostly caused by upstream test evolution.
- Keep the `rust-v0.115.0` test structure.
- Port the fork assertions and fixture updates into that structure instead of trying to paste back an older whole-file version.

## Second Patch Principles

For `b2d3d872a` `feat: allow model max output token overrides`:

- Preserve the new request/config field `model_max_output_tokens`.
- Keep the `rust-v0.115.0` request struct layout.
- Update tests and defaults in the files that now own those fixtures; do not try to revive removed inline test blocks.

## Lockfile Rule

Queue commits are meant to stay release-agnostic.

- During replay, strip `codex-rs/Cargo.lock` hunks from semantic queue commits.
- After all semantic patches apply, refresh the lockfile against the target upstream base.
- Expect a trailing commit like:
  `chore: refresh Cargo.lock for rust-v0.115.0`

The scripts already automate this, but if you finish a replay by hand, keep the same rule.

## Validation After Manual Resolution

For a manual recovery branch:

```bash
bash -n scripts/internal/prepare_queue_pr.sh
git diff --check
```

For a real replay candidate or rehearsal branch, also run the dry-run release workflow:

```bash
gh workflow run internal-rust-release.yml -R SDGLBL/codex --ref main \
  -f upstream_tag=rust-v0.115.0 \
  -f release_ref=rehearsal/queue/rust-v0.115.0-main-20260318 \
  -f internal_tag=rehearsal-internal-rust-v0.115.0-main-20260318 \
  -f publish=false
```

Swap `release_ref` and `internal_tag` to the branch/tag you actually resolved.

## Operational Notes

- `rehearsal` mode does not create a PR. That is expected.
- `official` mode creates `candidate/queue/rust-vX.Y.Z` and opens a PR to `queue/internal`.
- `INTERNAL_QUEUE_PAT` must exist if you want pushed rehearsal or official tags to trigger downstream `internal-rust-release` automatically.
