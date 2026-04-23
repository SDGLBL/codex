# Manual Queue Replay Conflict Resolution

Automatic replay of the queue patch stack onto `rust-v0.123.0` stopped at:
- `f93b00df1` feat: preserve wire session ids across resumed forks

This branch already contains every replayed patch before the failure point.
Continue locally by replaying the remaining commits in order.

## Conflicting Files
- `codex-rs/core/src/client.rs`
- `codex-rs/core/src/client_tests.rs`
- `codex-rs/core/src/codex.rs`
- `codex-rs/core/src/realtime_conversation.rs`
- `codex-rs/core/src/session/tests.rs`
- `codex-rs/core/tests/responses_headers.rs`
- `codex-rs/core/tests/suite/client.rs`
- `codex-rs/core/tests/suite/client_websockets.rs`
- `codex-rs/core/tests/suite/subagent_notifications.rs`
- `codex-rs/protocol/src/protocol.rs`

## Remaining Patch Queue
- `f93b00df1` feat: preserve wire session ids across resumed forks
- `b6df279bf` feat: allow model max output token overrides
- `5f777e52d` chore: maintain fork sync and internal release automation
- `5a71228ae` feat: add release installer and bootstrap internal profile
- `96bdd0df5` chore: fold queue sync and internal follow-ups
- `e35e47937` fix: adapt bootstrap internal profile replay to rust-v0.117.0
- `1f11aa7dc` test: refresh rust-v0.117.0 ui snapshots and seatbelt assertions
- `76e7a8c61` fix: configure musl rusty_v8 overrides for internal release
- `9293996ac` chore: finalize rust-v0.118.0 replay follow-ups

## Continue Locally
1. Pull this candidate branch:

   ```bash
   gh pr checkout <pr-number> -R SDGLBL/codex
   # or
   git fetch origin candidate/queue/rust-v0.123.0
   git switch -C candidate/queue/rust-v0.123.0 --track origin/candidate/queue/rust-v0.123.0
   ```

2. Fetch the upstream release tag:

   ```bash
   git fetch https://github.com/openai/codex refs/tags/rust-v0.123.0:refs/tags/rust-v0.123.0
   ```

3. Resume the replay at the failing patch:

   ```bash
   git cherry-pick -x f93b00df1bfd1b852fa6e4e03dc646420ad956da
   ```

4. Resolve conflicts, then continue the cherry-pick:

   ```bash
   git status
   git add <resolved-files>
   git cherry-pick --continue
   ```

5. Replay the rest of the queue:

   ```bash
   git cherry-pick -x b6df279bf954cac1e4aca1a3c7226c612cc5607c
   git cherry-pick -x 5f777e52d78b222d40427cca5a6a40c8fd6ef718
   git cherry-pick -x 5a71228ae489ecac2300ce675ec785956baccbe3
   git cherry-pick -x 96bdd0df5a7973631fe583c98d0eb2a22c7bbd17
   git cherry-pick -x e35e47937044eeb67d1d524d74e0041cccfe4ff8
   git cherry-pick -x 1f11aa7dcb46dcea56ad9768890fed1b1f4bb93a
   git cherry-pick -x 76e7a8c61d9ad806dbaff9046e9eac5227d55f05
   git cherry-pick -x 9293996ac72c5fa2538d026d262d70134531a3fd
   ```

6. Remove this helper note and push the updated branch:

   ```bash
   git rm QUEUE_REPLAY_CONFLICTS.md
   git commit --amend --no-edit
   git push origin HEAD:candidate/queue/rust-v0.123.0
   ```
