# Manual Queue Replay Conflict Resolution

Automatic replay of the queue patch stack onto `rust-v0.115.0` stopped at:
- `cdd5197e3` feat: preserve wire session ids across resumed forks

This branch already contains every replayed patch before the failure point.
Continue locally by replaying the remaining commits in order.

## Conflicting Files
- `codex-rs/core/src/client.rs`
- `codex-rs/core/src/personality_migration.rs`
- `codex-rs/core/src/realtime_conversation.rs`
- `codex-rs/core/src/rollout/metadata.rs`
- `codex-rs/core/src/rollout/recorder.rs`
- `codex-rs/core/tests/suite/subagent_notifications.rs`

## Remaining Patch Queue
- `cdd5197e3` feat: preserve wire session ids across resumed forks
- `b2d3d872a` feat: allow model max output token overrides
- `a643f7800` chore: maintain fork sync and internal release automation

## Continue Locally
1. Pull this candidate branch:

   ```bash
   gh pr checkout <pr-number> -R SDGLBL/codex
   # or
   git fetch origin rehearsal/queue/rust-v0.115.0-main-20260318
   git switch -C rehearsal/queue/rust-v0.115.0-main-20260318 --track origin/rehearsal/queue/rust-v0.115.0-main-20260318
   ```

2. Fetch the upstream release tag:

   ```bash
   git fetch https://github.com/openai/codex refs/tags/rust-v0.115.0:refs/tags/rust-v0.115.0
   ```

3. Resume the replay at the failing patch:

   ```bash
   git cherry-pick -x cdd5197e30820cb82c8228928480f8611b5b78b7
   ```

4. Resolve conflicts, then continue the cherry-pick:

   ```bash
   git status
   git add <resolved-files>
   git cherry-pick --continue
   ```

5. Replay the rest of the queue:

   ```bash
   git cherry-pick -x b2d3d872ade33f280f16d66686806c0ab263f06d
   git cherry-pick -x a643f7800720587dcfd11fbfb4782ab67c492adf
   ```

6. Remove this helper note and push the updated branch:

   ```bash
   git rm QUEUE_REPLAY_CONFLICTS.md
   git commit --amend --no-edit
   git push origin HEAD:rehearsal/queue/rust-v0.115.0-main-20260318
   ```
