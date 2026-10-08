# docs/ref — 参考源码归档

本目录存放被 `docs/plan/*` 分析文档引述到的**上游项目源文件副本**，用于离线核对 `file:line` 引用，无需联网重新拉取。

## 本次新增

**code-server Git 功能分析**（对应 `docs/plan/web/coder/`）

- `coder-analysis/` — 四份分析文档副本（内容与 `docs/plan/web/coder/` 完全一致）
- `sources/code-server/` — code-server 服务端集成关键文件
- `sources/vscode-git-extension/` — VSCode 内置 git 扩展关键文件

**lazygit 交互模型分析**（对应 `docs/plan/frontend/`，Web 端 Git 工作台设计方案）

- `lazygit-analysis/` — 八份分析文档副本（内容与 `docs/plan/frontend/` 完全一致）
- `sources/lazygit/` — lazygit 上游关键源文件（Context 树、键位分发、侧栏分组、中文速查表）

## 来源

| 项目 | commit | 说明 |
|---|---|---|
| `coder/code-server` | `fa7caee4dd82df8ef78ba4c3b23a7f84c7c70370` | https://github.com/coder/code-server |
| `microsoft/vscode` | `04c0d99f4fb0d8afe6ce4f0c58e31e183ac3e4b1` | https://github.com/microsoft/vscode ；git 扩展位于 `extensions/git/src`（本仓以 `lib/vscode` 子模块 vendored） |
| `jesseduffield/lazygit` | `a3fae72578e472b11b9763c7d5536c6853f31da4` | https://github.com/jesseduffield/lazygit |

## `file:line` 对应关系

分析文档中的路径可在此处按相同相对路径离线核对：

- 文档 `src/node/routes/vscode.ts:240` → `sources/code-server/src/node/routes/vscode.ts:240`
- 文档 `extensions/git/src/repository.ts:1011` → `sources/vscode-git-extension/repository.ts:1011`
- 文档 `lazygit/pkg/config/user_config.go:882` → `sources/lazygit/pkg/config/user_config.go:882`（去掉 `lazygit/` 前缀）
- 其余文件同理：去掉 `extensions/git/src/` 前缀即为 `sources/vscode-git-extension/` 下文件名；去掉 `lazygit/` 前缀即为 `sources/lazygit/` 下路径；去掉 code-server 仓库根前缀即为 `sources/code-server/` 下路径。

## 注意

副本仅用于引述核对；如需更新请以各仓库上游为准。
