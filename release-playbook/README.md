# Codex驱动的内部Rust发布操作手册

本操作手册是`SDGLBL/codex`中内部Rust发布的唯一权威来源。

流程设计尽量精简：

- 通过`candidate/queue/rust-vX.Y.Z`分支跟踪上游
- 通过`workflow_dispatch`手动触发`.github/workflows/internal-rust-release.yml`
- 将自定义安装脚本保留在发布资产中（`scripts/install/install.sh`、`scripts/install/install.ps1`）

无需自动上游跟踪、队列提升或仅标签触发的流程。

## 1. 前置条件

开始之前：

1. `gh auth status`对`SDGLBL/codex`处于健康状态。
2. 本地远程仓库已配置：
   - `origin` -> `SDGLBL/codex`
   - `upstream` -> `openai/codex`
3. 工作区干净。

## 2. 分支约定

- 上游源标签格式：`rust-vX.Y.Z`。
- 候选分支格式：`candidate/queue/rust-vX.Y.Z`。
- 内部发布标签格式：`internal-rust-vX.Y.Z`。

`candidate/queue/rust-vX.Y.Z`是发布跟进所需的唯一跟踪分支。

## 3. 跟进上游并创建候选分支

将下面的`rust-vX.Y.Z`替换为目标发布版本，例如`rust-v0.125.0`。

### 步骤A：拉取与预检查

```bash
git fetch origin
git fetch upstream --tags
gh release view rust-vX.Y.Z --repo openai/codex
```

### 步骤B：从上游标签创建候选分支

```bash
git switch -C candidate/queue/rust-vX.Y.Z rust-vX.Y.Z
```

## 4. 移植Fork补丁（先进行语义对齐）

使用上一个候选分支作为默认补丁来源，然后将fork补丁重放到新的候选分支上。

```bash
git for-each-ref --sort=-committerdate --format='%(refname:short)' refs/remotes/origin/candidate/queue/rust-v*
```

选取上一个已发布的候选分支（示例：`origin/candidate/queue/rust-v0.124.0`）：

```bash
prev_candidate="origin/candidate/queue/rust-v0.124.0"
prev_upstream_tag="${prev_candidate#origin/candidate/queue/}"

mapfile -t patch_commits < <(
  git rev-list --reverse --no-merges "${prev_upstream_tag}..${prev_candidate}"
)

for commit in "${patch_commits[@]}"; do
  git cherry-pick -x "${commit}"
done
```

按以下顺序维护候选分支的补丁栈：

1. 稳定的内部修复提交，应持续向前携带
2. 一个针对当前分支的适配/整合提交，用于发布特定的对齐
3. 不稳定或仍在验证中的修复提交，应保持易于修改、删除或移动

当某个修复变得稳定后，在下次清理操作中将其移至适配提交之下。将发布特定的变动、冲突解决、快照刷新以及操作手册维护保留在适配提交中，以便可复用的修复栈保持清晰。

冲突处理策略：

- 保留fork行为，根据上游变化调整实现
- 不要求固定的提交哈希值
- 如果cherry-pick无法干净地继续，请手动移植变更并以清晰、可审查的形式提交

## 5. 验证并发布候选分支

至少执行：

```bash
git diff --check
```

根据仓库策略，对受影响的Crate运行相关的格式化/测试，然后推送：

```bash
git push -u origin candidate/queue/rust-vX.Y.Z
```

## 6. 直接触发内部发布Action

发布版本：

```bash
gh workflow run internal-rust-release.yml -R SDGLBL/codex -f upstream_tag=rust-vX.Y.Z -f release_ref=candidate/queue/rust-vX.Y.Z -f internal_tag=internal-rust-vX.Y.Z -f publish=true
```

仅进行空跑捆绑：

```bash
gh workflow run internal-rust-release.yml -R SDGLBL/codex -f upstream_tag=rust-vX.Y.Z -f release_ref=candidate/queue/rust-vX.Y.Z -f internal_tag=internal-rust-vX.Y.Z-dryrun -f publish=false
```

## 7. 验证输出

对于已发布的版本：

```bash
gh release view internal-rust-vX.Y.Z --repo SDGLBL/codex
```

确认：

- 发布存在，且具有预期的标签/版本
- 发布说明包含上游标签和补丁栈
- 发布资产包含内部二进制文件、`config.schema.json`、`install.sh`、`install.ps1`以及`rg`捆绑包

对于空跑发布，验证工作流运行中上传的`internal-release-dry-run-*`工件。

## 8. 纠正性发布

如果后续的补丁移植有误，请修复候选分支并使用新的内部标签再次运行工作流调度。

本操作手册中不需要`queue/internal`、`queue/base/internal`或`main`提升步骤。
