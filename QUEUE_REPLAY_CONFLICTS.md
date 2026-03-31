# Manual Queue Replay Conflict Resolution

Automatic replay of the queue patch stack onto `rust-v0.118.0` stopped at:
- `57b6a9757` feat: preserve wire session ids across resumed forks

This branch already contains every replayed patch before the failure point.
Continue locally by replaying the remaining commits in order.

## Conflicting Files
- `codex-rs/core/src/client_tests.rs`
- `codex-rs/core/src/codex_tests.rs`
- `codex-rs/core/tests/suite/rollout_list_find.rs`
- `codex-rs/rollout/src/recorder_tests.rs`

## Remaining Patch Queue
- `57b6a9757` feat: preserve wire session ids across resumed forks
- `c037686ef` feat: allow model max output token overrides
- `c21b25038` chore: maintain fork sync and internal release automation
- `1e8456bf6` feat: add release installer and bootstrap internal profile
- `4c83e1038` chore: fold queue sync and internal follow-ups
- `daa0e735e` fix: adapt bootstrap internal profile replay to rust-v0.117.0
- `2a0113899` chore: refresh Cargo.lock for rust-v0.117.0
- `4dfb45f51` test: refresh rust-v0.117.0 ui snapshots and seatbelt assertions
- `ec58b08fb` fix: configure musl rusty_v8 overrides for internal release

## Continue Locally
1. Pull this candidate branch:

   ```bash
   gh pr checkout <pr-number> -R SDGLBL/codex
   # or
   git fetch origin candidate/queue/rust-v0.118.0
   git switch -C candidate/queue/rust-v0.118.0 --track origin/candidate/queue/rust-v0.118.0
   ```

2. Fetch the upstream release tag:

   ```bash
   git fetch https://github.com/openai/codex refs/tags/rust-v0.118.0:refs/tags/rust-v0.118.0
   ```

3. Resume the replay at the failing patch:

   ```bash
   git cherry-pick -x 57b6a975740e1cd5f6f0d5866cf825ddf85b7a67
   ```

4. Resolve conflicts, then continue the cherry-pick:

   ```bash
   git status
   git add <resolved-files>
   git cherry-pick --continue
   ```

5. Replay the rest of the queue:

   ```bash
   git cherry-pick -x c037686ef92b18d353c0ad7d9946bab9c68475b2
   git cherry-pick -x c21b25038d609772ee4f434250819a881337eac0
   git cherry-pick -x 1e8456bf6a2bcbba2911fd4d614fa3f3c23799c5
   git cherry-pick -x 4c83e1038b7bb0e8c266b2ab7ce093bb724a4f84
   git cherry-pick -x daa0e735e7fc776b52109244b8c28c88fbaae2f4
   git cherry-pick -x 2a0113899b3845e7bb9da25edddc08d587627882
   git cherry-pick -x 4dfb45f51df5a20f3a7be8751c3d40b624f61851
   git cherry-pick -x ec58b08fb0b130e8141e0bdbf025310fc1bf58fa
   ```

6. Remove this helper note and push the updated branch:

   ```bash
   git rm QUEUE_REPLAY_CONFLICTS.md
   git commit --amend --no-edit
   git push origin HEAD:candidate/queue/rust-v0.118.0
   ```
