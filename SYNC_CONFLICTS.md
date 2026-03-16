# Manual Sync Conflict Resolution

Automatic sync could not complete `merge rust-v0.115.0 into origin/main`.

This branch already starts from the current internal stable line, so once you finish the same merge locally and push the result, the PR should become mergeable.

## Conflicting Files
- `codex-rs/Cargo.toml`
- `codex-rs/core/src/auth.rs`
- `codex-rs/core/src/client.rs`
- `codex-rs/core/src/client_common.rs`
- `codex-rs/core/src/codex_tests.rs`
- `codex-rs/core/src/mcp_tool_call.rs`
- `codex-rs/core/src/personality_migration.rs`
- `codex-rs/core/src/realtime_conversation.rs`
- `codex-rs/core/src/rollout/metadata.rs`
- `codex-rs/core/src/rollout/recorder.rs`
- `codex-rs/core/src/shell.rs`
- `codex-rs/core/src/tools/code_mode.rs`
- `codex-rs/core/src/tools/code_mode_bridge.js`
- `codex-rs/core/src/tools/code_mode_runner.cjs`
- `codex-rs/core/src/tools/context.rs`
- `codex-rs/core/src/tools/handlers/mcp.rs`
- `codex-rs/core/src/tools/js_repl/mod.rs`
- `codex-rs/core/src/tools/parallel.rs`
- `codex-rs/core/src/tools/router.rs`
- `codex-rs/core/src/tools/spec.rs`
- `codex-rs/core/tests/suite/code_mode.rs`
- `codex-rs/core/tests/suite/subagent_notifications.rs`
- `codex-rs/protocol/src/models.rs`
- `sdk/python/README.md`
- `sdk/python/docs/faq.md`
- `sdk/python/docs/getting-started.md`
- `sdk/python/pyproject.toml`
- `sdk/python/scripts/update_sdk_artifacts.py`
- `sdk/python/src/codex_app_server/client.py`
- `sdk/python/src/codex_app_server/generated/notification_registry.py`
- `sdk/python/src/codex_app_server/generated/v2_all.py`
- `sdk/python/src/codex_app_server/models.py`
- `sdk/python/tests/test_artifact_workflow_and_binaries.py`
- `sdk/python/tests/test_contract_generation.py`

## Current Internal Patch Stack
- `235acfedd` feat: preserve wire session ids across resumed forks
- `047115efe` feat: allow model max output token overrides
- `4b0201134` chore: automate fork sync and internal releases
- `013f5d382` chore: adapt fork CI to shared runners
- `ab01dbc9f` chore: split fork CI from upstream runner matrix
- `9b5639c1d` chore: use supported macOS Intel release runner
- `9b81c1a1f` chore: preload ubsan for musl release builds
- `0974d7f66` chore: disable aws-lc jitter entropy on musl builders
- `f36b7ba68` chore: publish config schema with upstream filename
- `d8c53cc56` chore: extend internal release timeout for mac builds
- `6dcb5e965` chore: narrow internal release to linux musl
- `74601816b` chore: restore multi-platform internal release targets
- `69d449f1e` chore: open sync prs for manual conflict resolution
- `d65219f0a` fix: pass GH token to upstream release resolver
- `a171fc96b` fix: compute patch stack from stable main
- `cc9fadfeb` fix: sync stable line by merging upstream release tags

## Continue Locally
1. Pull this sync branch locally:

   ```bash
   gh pr checkout <pr-number> -R SDGLBL/codex
   # or
   git fetch origin sync/rust-v0.115.0
   git switch -C sync/rust-v0.115.0 --track origin/sync/rust-v0.115.0
   ```

2. Fetch the upstream release tag into your local clone:

   ```bash
   git fetch https://github.com/openai/codex refs/tags/rust-v0.115.0:refs/tags/rust-v0.115.0
   ```

3. Re-run the upstream merge locally:

   ```bash
   git merge --no-ff rust-v0.115.0
   ```

4. Resolve the files above, then stage your fixes:

   ```bash
   git status
   git add <resolved-files>
   ```

5. Remove this helper note before finishing the merge:

   ```bash
   git rm SYNC_CONFLICTS.md
   ```

6. Finish the merge and push the branch back to the PR:

   ```bash
   git commit
   git push origin HEAD:sync/rust-v0.115.0
   ```

If Git says the merge produced no changes, double-check that you are on `sync/rust-v0.115.0` and that the branch tip matches the PR head before retrying.
