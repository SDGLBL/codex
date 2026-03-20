# Manual Queue Replay Conflict Resolution

Automatic replay of the queue patch stack onto `rust-v0.117.0-alpha.5` stopped at:
- `807599cf2` chore: maintain fork sync and internal release automation

This branch already contains every replayed patch before the failure point.
Continue locally by replaying the remaining commits in order.

## Conflicting Files
- `.github/workflows/rust-ci.yml`

## Remaining Patch Queue
- `807599cf2` chore: maintain fork sync and internal release automation
- `7faf88912` codex: fix fork CI on PR #4
- `a76d71f6b` codex: fix PR #4 regressions and snapshots
- `5630cb41b` chore: dispatch internal release after tagging
- `bfebdb5e2` codex: switch internal sync to queue-first
- `d8198cdcf` chore: align queue bootstrap tree with current main
- `bb63f2051` codex: use queue PAT for prepare replay pushes
- `9e78d3b32` chore: track latest upstream rust releases
- `12678cb9d` fix: disable checkout credential persistence for queue pushes
- `d466c37bc` codex: allow replay promotion to update queue refs
- `3476d4bea` codex: push release promotions via refs
- `a68872095` codex: refresh tui version snapshots
- `7c26f06a7` codex: align alpha.12 bootstrap tree
- `bb126706d` chore: refresh Cargo.lock for rust-v0.116.0-alpha.12

## Continue Locally
1. Pull this candidate branch:

   ```bash
   gh pr checkout <pr-number> -R SDGLBL/codex
   # or
   git fetch origin candidate/queue/rust-v0.117.0-alpha.5
   git switch -C candidate/queue/rust-v0.117.0-alpha.5 --track origin/candidate/queue/rust-v0.117.0-alpha.5
   ```

2. Fetch the upstream release tag:

   ```bash
   git fetch https://github.com/openai/codex refs/tags/rust-v0.117.0-alpha.5:refs/tags/rust-v0.117.0-alpha.5
   ```

3. Resume the replay at the failing patch:

   ```bash
   git cherry-pick -x 807599cf28403b0c3a8d26d285fa51f2c239f428
   ```

4. Resolve conflicts, then continue the cherry-pick:

   ```bash
   git status
   git add <resolved-files>
   git cherry-pick --continue
   ```

5. Replay the rest of the queue:

   ```bash
   git cherry-pick -x 7faf88912027637c14f00be53f75fd5475d46315
   git cherry-pick -x a76d71f6b6fca348d6a4a975bc0014bccb277786
   git cherry-pick -x 5630cb41be903d473b60fada53a68a498558ba45
   git cherry-pick -x bfebdb5e2a011ce9511f00d1e764346922a75eb6
   git cherry-pick -x d8198cdcf9193405371a24114322b7ae285ee580
   git cherry-pick -x bb63f2051b91599aa42a0723b746e89a253dc52f
   git cherry-pick -x 9e78d3b329d986a2525495c00d2ee15f63ba324b
   git cherry-pick -x 12678cb9dceea8a265eeb90c51806f8526db11f2
   git cherry-pick -x d466c37bc975731e707cfaf60bc7355cc4e0f32b
   git cherry-pick -x 3476d4beadbe9d1f99838ad4bdc96a8fa7d271f3
   git cherry-pick -x a688720952081b2cea3dc5b08653f46dd0aa15cd
   git cherry-pick -x 7c26f06a76e4b33e6da4e8a460b8e8828da61577
   git cherry-pick -x bb126706d888999eba4a09c29b7a1770f399272a
   ```

6. Remove this helper note and push the updated branch:

   ```bash
   git rm QUEUE_REPLAY_CONFLICTS.md
   git commit --amend --no-edit
   git push origin HEAD:candidate/queue/rust-v0.117.0-alpha.5
   ```
