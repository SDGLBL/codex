# Manual Queue Replay Conflict Resolution

Automatic replay of the queue patch stack onto `rust-v0.116.0-alpha.11` stopped at:
- `5977b9ba1` feat: preserve wire session ids across resumed forks

This branch already contains every replayed patch before the failure point.
Continue locally by replaying the remaining commits in order.

## Conflicting Files
- `codex-rs/core/src/realtime_conversation.rs`

## Remaining Patch Queue
- `5977b9ba1` feat: preserve wire session ids across resumed forks
- `e88861734` feat: allow model max output token overrides
- `131d67367` chore: maintain fork sync and internal release automation
- `3db639289` chore: refresh Cargo.lock for rust-v0.115.0 rehearsal
- `47f41ca27` codex: fix fork CI on PR #4
- `1458deac4` codex: fix PR #4 regressions and snapshots
- `950bba4cd` chore: dispatch internal release after tagging
- `218421bc7` codex: switch internal sync to queue-first
- `2c3eef493` chore: align queue bootstrap tree with current main
- `fcaa0f16e` codex: use queue PAT for prepare replay pushes
- `2bdab8368` chore: track latest upstream rust releases
- `433c2b719` fix: disable checkout credential persistence for queue pushes

## Continue Locally
1. Pull this candidate branch:

   ```bash
   gh pr checkout <pr-number> -R SDGLBL/codex
   # or
   git fetch origin candidate/queue/rust-v0.116.0-alpha.11
   git switch -C candidate/queue/rust-v0.116.0-alpha.11 --track origin/candidate/queue/rust-v0.116.0-alpha.11
   ```

2. Fetch the upstream release tag:

   ```bash
   git fetch https://github.com/openai/codex refs/tags/rust-v0.116.0-alpha.11:refs/tags/rust-v0.116.0-alpha.11
   ```

3. Resume the replay at the failing patch:

   ```bash
   git cherry-pick -x 5977b9ba15aca93fc9808c38dd7b8d0aa683f624
   ```

4. Resolve conflicts, then continue the cherry-pick:

   ```bash
   git status
   git add <resolved-files>
   git cherry-pick --continue
   ```

5. Replay the rest of the queue:

   ```bash
   git cherry-pick -x e888617341d5219478bd3fa484572bb3b57c7330
   git cherry-pick -x 131d67367d2625ee83ae91e661824de9e70b5ed1
   git cherry-pick -x 3db639289a41eae24be97e8ca045ea0a772b7244
   git cherry-pick -x 47f41ca27a6130391670778289dcb21e677021dc
   git cherry-pick -x 1458deac4e10ac9c8dd0138c3e5eac02e3e7798b
   git cherry-pick -x 950bba4cd24e1e3d38d35fb15d7e4168442852f8
   git cherry-pick -x 218421bc775bdce88f2cf4c9d087aa443ff30d9e
   git cherry-pick -x 2c3eef49306d8a6da6a82528d579782b4f282fe5
   git cherry-pick -x fcaa0f16ecd26b34001f6aa44acbdc0dbf9e1380
   git cherry-pick -x 2bdab8368fd8010f7770228425f5941530433718
   git cherry-pick -x 433c2b7191c9145932459b68b0e8ac10bfad4d2f
   ```

6. Remove this helper note and push the updated branch:

   ```bash
   git rm QUEUE_REPLAY_CONFLICTS.md
   git commit --amend --no-edit
   git push origin HEAD:candidate/queue/rust-v0.116.0-alpha.11
   ```
