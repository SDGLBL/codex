# Manual Queue Replay Conflict Resolution

Automatic replay of the queue patch stack onto `rust-v0.116.0` stopped at:
- `f028679ab` ## New Features - Supported models can now request full-resolution image inspection through both `view_image` and `codex.emitImage(..., detail: "original")`, which helps with precision visual tasks. (#14175) - `js_repl` now exposes `codex.cwd` and `codex.homeDir`, and saved `codex.tool(...)` / `codex.emitImage(...)` references keep working across cells. (#14385, #14503) - Realtime websocket sessions gained a dedicated transcription mode, plus v2 handoff support through the `codex` tool, with a unified `[realtime]` session config. (#14554, #14556, #14606) - The v2 app-server now exposes filesystem RPCs for file reads, writes, copies, directory operations, and path watching, and there is a new Python SDK for integrating with that API. (#14245, #14435) - Smart Approvals can now route review requests through a guardian subagent in core, app-server, and TUI, reducing repeated setup work on follow-up approvals. (#13860, #14668) - App integrations now use the Responses API tool-search flow, can suggest missing tools, and fall back cleanly when the active model does not support search-based lookup. (#14274, #14287, #14732)

This branch already contains every replayed patch before the failure point.
Continue locally by replaying the remaining commits in order.

## Conflicting Files
- `codex-rs/Cargo.toml`

## Remaining Patch Queue
- `f028679ab` ## New Features - Supported models can now request full-resolution image inspection through both `view_image` and `codex.emitImage(..., detail: "original")`, which helps with precision visual tasks. (#14175) - `js_repl` now exposes `codex.cwd` and `codex.homeDir`, and saved `codex.tool(...)` / `codex.emitImage(...)` references keep working across cells. (#14385, #14503) - Realtime websocket sessions gained a dedicated transcription mode, plus v2 handoff support through the `codex` tool, with a unified `[realtime]` session config. (#14554, #14556, #14606) - The v2 app-server now exposes filesystem RPCs for file reads, writes, copies, directory operations, and path watching, and there is a new Python SDK for integrating with that API. (#14245, #14435) - Smart Approvals can now route review requests through a guardian subagent in core, app-server, and TUI, reducing repeated setup work on follow-up approvals. (#13860, #14668) - App integrations now use the Responses API tool-search flow, can suggest missing tools, and fall back cleanly when the active model does not support search-based lookup. (#14274, #14287, #14732)
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
- `4fd277461` Add Python SDK thread.run convenience methods (#15088)
- `903660edb` Remove stdio transport from exec server (#15119)
- `20f2a216d` feat(core, tracing): create turn spans over websockets (#14632)
- `b14689df3` Forward session and turn headers to MCP HTTP requests (#15011)
- `42e932d7b` [hooks] turn_id extension for Stop & UserPromptSubmit (#15118)
- `10eb3ec7f` Simple directory mentions (#14970)
- `01df50cf4` Add thread/shellCommand to app server API surface (#14988)
- `2bdab8368` chore: track latest upstream rust releases
- `db5781a08` feat: support product-scoped plugins. (#15041)
- `433c2b719` fix: disable checkout credential persistence for queue pushes
- `8fc505816` chore: record queue replay conflicts for rust-v0.116.0-alpha.11
- `a66f0a368` feat: preserve wire session ids across resumed forks
- `8f25c2c26` feat: allow model max output token overrides
- `2dd6d5607` chore: maintain fork sync and internal release automation
- `ad3ca0292` chore: refresh Cargo.lock for rust-v0.115.0 rehearsal
- `fac26c146` codex: fix fork CI on PR #4
- `47e4e377c` codex: fix PR #4 regressions and snapshots
- `e3d393059` chore: dispatch internal release after tagging
- `f80668d4c` codex: switch internal sync to queue-first
- `6305fd351` chore: align queue bootstrap tree with current main
- `74fd017cf` codex: use queue PAT for prepare replay pushes
- `fa299cccb` chore: track latest upstream rust releases
- `64b13f665` fix: disable checkout credential persistence for queue pushes
- `ac8db073e` codex: allow replay promotion to update queue refs
- `54aa6d095` codex: push release promotions via refs
- `70cdb1770` feat: add graph representation of agent network (#15056)
- `32d2df5c1` fix: case where agent is already closed (#15163)
- `dee03da50` Move environment abstraction into exec server (#15125)
- `2cf4d5ef3` chore: add metrics for profile (#15180)
- `859c58f07` chore: morpheus does not generate memories (#15175)
- `d05ff156f` Release 0.116.0-alpha.12
- `248c9da47` chore: record queue replay conflicts for rust-v0.116.0-alpha.11
- `40d213b0f` feat: preserve wire session ids across resumed forks
- `04e85149f` feat: allow model max output token overrides
- `152683280` chore: maintain fork sync and internal release automation
- `d3abe3b64` codex: fix fork CI on PR #4
- `cf070d9aa` codex: fix PR #4 regressions and snapshots
- `79e5de833` chore: dispatch internal release after tagging
- `d06d50b4a` codex: switch internal sync to queue-first
- `44fb26a2e` chore: align queue bootstrap tree with current main
- `5fdb1e3c8` codex: use queue PAT for prepare replay pushes
- `23326130c` chore: track latest upstream rust releases
- `73cdabf6c` fix: disable checkout credential persistence for queue pushes
- `8cd88951b` codex: allow replay promotion to update queue refs
- `c8405858c` codex: push release promotions via refs
- `2c2f0687f` chore: refresh Cargo.lock for rust-v0.116.0-alpha.12
- `e54b7153e` codex: refresh tui version snapshots

## Continue Locally
1. Pull this candidate branch:

   ```bash
   gh pr checkout <pr-number> -R SDGLBL/codex
   # or
   git fetch origin candidate/queue/rust-v0.116.0
   git switch -C candidate/queue/rust-v0.116.0 --track origin/candidate/queue/rust-v0.116.0
   ```

2. Fetch the upstream release tag:

   ```bash
   git fetch https://github.com/openai/codex refs/tags/rust-v0.116.0:refs/tags/rust-v0.116.0
   ```

3. Resume the replay at the failing patch:

   ```bash
   git cherry-pick -x f028679abb30051cec2434e624cd99975986b41b
   ```

4. Resolve conflicts, then continue the cherry-pick:

   ```bash
   git status
   git add <resolved-files>
   git cherry-pick --continue
   ```

5. Replay the rest of the queue:

   ```bash
   git cherry-pick -x 5977b9ba15aca93fc9808c38dd7b8d0aa683f624
   git cherry-pick -x e888617341d5219478bd3fa484572bb3b57c7330
   git cherry-pick -x 131d67367d2625ee83ae91e661824de9e70b5ed1
   git cherry-pick -x 3db639289a41eae24be97e8ca045ea0a772b7244
   git cherry-pick -x 47f41ca27a6130391670778289dcb21e677021dc
   git cherry-pick -x 1458deac4e10ac9c8dd0138c3e5eac02e3e7798b
   git cherry-pick -x 950bba4cd24e1e3d38d35fb15d7e4168442852f8
   git cherry-pick -x 218421bc775bdce88f2cf4c9d087aa443ff30d9e
   git cherry-pick -x 2c3eef49306d8a6da6a82528d579782b4f282fe5
   git cherry-pick -x fcaa0f16ecd26b34001f6aa44acbdc0dbf9e1380
   git cherry-pick -x 4fd2774614182ebaf74f2e7a8c04bbcf0b09ed96
   git cherry-pick -x 903660edba6e1ecfd7c9b1782105be4ebf0e02a7
   git cherry-pick -x 20f2a216df3e2d534069438ca7126811de9ff89a
   git cherry-pick -x b14689df3b97245faa9c29a0b8f3f6c4d09393bf
   git cherry-pick -x 42e932d7bf70cc8e7ce912b4bbd27c0266293ad5
   git cherry-pick -x 10eb3ec7fccaf805c7162d8370b5b99bf57ddc48
   git cherry-pick -x 01df50cf422b2eb89cb6ad8f845548e8c0d3c60c
   git cherry-pick -x 2bdab8368fd8010f7770228425f5941530433718
   git cherry-pick -x db5781a08872873a4df82fbb4b3dc6ffd98b5d15
   git cherry-pick -x 433c2b7191c9145932459b68b0e8ac10bfad4d2f
   git cherry-pick -x 8fc50581636ee8ac09037e8161724beb1c0e88e7
   git cherry-pick -x a66f0a36801b6e299555ae8169bad04f0dedb0eb
   git cherry-pick -x 8f25c2c26fbdd0642024abee6a0b59f3fee56c88
   git cherry-pick -x 2dd6d5607bb2139f5c1d62eabec5b013f5448e59
   git cherry-pick -x ad3ca029280bb49d6340ae885d2e0ccaa94c762f
   git cherry-pick -x fac26c146777d9003eb131b56acdaa6b9e119388
   git cherry-pick -x 47e4e377c084e974575699f951f575fe07476e57
   git cherry-pick -x e3d393059bc2c081ed83b948770c5b99a3a504b8
   git cherry-pick -x f80668d4c958ad5edaf3cfe1a66353a49db0fc5d
   git cherry-pick -x 6305fd3513f7d1e98248e53b91fc3b5517b13295
   git cherry-pick -x 74fd017cf619abcc669df4183e4ce8150b8e2d1b
   git cherry-pick -x fa299cccb34492bb41da9175e76be7859b42cdbd
   git cherry-pick -x 64b13f66505cf756406074ef225a5be1fa98b35b
   git cherry-pick -x ac8db073ebd34f6c6e765c7d28a50cae4005ab11
   git cherry-pick -x 54aa6d095076331e18ba0e7cef1faeefb5ada50e
   git cherry-pick -x 70cdb17703a4310b7173642e011f7534d2b2624f
   git cherry-pick -x 32d2df5c1e97948cb5c55481f0b5fd3f8dfabf43
   git cherry-pick -x dee03da508a2cdefa9cf8eadad083f6af7fe49f8
   git cherry-pick -x 2cf4d5ef353a0264df280644b26fa7d8fb42d406
   git cherry-pick -x 859c58f07dc3768b654711b7841f35e676005e6c
   git cherry-pick -x d05ff156fa4b7f745165f6f70d581b088e18412b
   git cherry-pick -x 248c9da471d36e19b794b8677625e82ed87300be
   git cherry-pick -x 40d213b0f5ac5f11214de976c895d2a7564c16dd
   git cherry-pick -x 04e85149fd6889876019cd42af76b88e29c75cd1
   git cherry-pick -x 15268328034333e6fb3e674467ea195812e553da
   git cherry-pick -x d3abe3b64832d5ce669abf68daa3334856fd298f
   git cherry-pick -x cf070d9aaf563b5a6d14413b951aed337fecca22
   git cherry-pick -x 79e5de8331492db50b25ffbd33d46511be8f318a
   git cherry-pick -x d06d50b4a1609172f46cf20480845c383a96d2b8
   git cherry-pick -x 44fb26a2ea6677700f8291be612fe223e8655198
   git cherry-pick -x 5fdb1e3c80b9a2a4ea4f1eff1c1c738d603106ca
   git cherry-pick -x 23326130cc986bcca24474683301e5f00dc3d98f
   git cherry-pick -x 73cdabf6ca192b9c6d48aa8e40d05ee8758f6471
   git cherry-pick -x 8cd88951b48f10192075f7fb3a2812483e33b167
   git cherry-pick -x c8405858ca413da90918c466c6636aa635f8b632
   git cherry-pick -x 2c2f0687fcf1461dc9c2cbcf09be122ddc62a92f
   git cherry-pick -x e54b7153e0ef0ff75520a16011573eed8a40b934
   ```

6. Remove this helper note and push the updated branch:

   ```bash
   git rm QUEUE_REPLAY_CONFLICTS.md
   git commit --amend --no-edit
   git push origin HEAD:candidate/queue/rust-v0.116.0
   ```
