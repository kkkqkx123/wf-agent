# wf-shell 独立 crate 拆分方案

> 状态：设计完成，Stage 1-4 已实施
> 范围：`crates/wf-tools/` 的 shell 引擎与工具定义拆分，新增 `crates/wf-shell/`
> 对照：TS `packages/sdk/services/terminal/`（terminal-service，行为参考，只读）
> 关联文档：`docs/plan/wf-tools-gap-supplement-plan.md`（Stage 6 终端能力，shell 探测/增量输出已完成；本文在其基础上做结构重组）

## 一、背景与目标

当前 terminal/shell 的底层实现逻辑分散在 `wf-tools` 的多处模块中，且核心引擎（约 4900 行）被物理嵌入工具定义层（`predefined/shell/`）。本文目标：

1. 将 shell/终端底层能力收敛为独立 crate `wf-shell`，与工具注册层解耦。
2. 保持现有依赖 DAG 不变（`wf-shell` 只依赖 `wf-types` / `wf-common`）。
3. 不改变任何对外工具行为与事件语义，仅做结构重组（no-backward-compatible 原则下允许接口随 crate 归属调整）。

## 二、现状分析：terminal/shell 处理分布

### 2.1 核心引擎（集中在 wf-tools，约 4900 行）

| 位置 | 规模 | 职责 |
|------|------|------|
| `crates/wf-tools/src/predefined/shell/engine.rs` | 2099 行 | PTY（`portable-pty`）/ pipe 双后端、`TerminalSession`、`ShellSession`、`PipeBackend`/`PtyBackend`、`OutputBuffer`（环形缓冲+读游标）、`BackgroundShellStore`、输出 reader、优雅 kill、超时 |
| `crates/wf-tools/src/shell.rs` | 334 行 | `ShellToolConfig` + 无状态 `execute_command` handler + `run_command`（`tokio::process`，超时/stdin/策略检查） |
| `crates/wf-tools/src/shell/shell_detector.rs` | 480 行 | `ShellType`（9 种）/ `ShellDetector`、`$SHELL`/`which` 跨平台探测与缓存 |
| `crates/wf-tools/src/predefined/shell/event_sink.rs` | 37 行 | `ShellEventSink` trait（会话生命周期/输出事件） |
| `crates/wf-tools/src/command_safety.rs` | 约 300 行 | 命令 allow/deny 策略（同时被 `approval.rs`、`engine.rs` 复用） |
| `crates/wf-tools/src/predefined/shell/*.rs`（9 个工具文件） | 约 900 行 | 工具定义薄封装（backend_shell / shell_output / shell_kill / shell_send_input / shell_resize / get_or_create_shell / execute_in_session / release_sessions_for_task / execute_command） |

### 2.2 上层接线（非 wf-tools）

| 位置 | 职责 |
|------|------|
| `crates/wf-runtime/src/shell_event_bridge.rs` | `ShellEventBusBridge`：把 shell 事件桥接到 `wf_core::EventBus`（放在 wf-runtime 是为了 wf-tools 不依赖 wf-core） |
| `crates/wf-runtime/src/bootstrap.rs` | 组装 `ShellToolConfig` + bridge；sandbox 的 shell 路由规则与集成测试 |
| `crates/wf-types/src/events/base.rs` | `EventType::Shell*` 事件类型 |
| `crates/wf-script/src/types.rs` | `ExecutorMode::Pty` 标记（仅枚举，无实现） |

### 2.3 概念独立、不合并的部分

| 位置 | 说明 |
|------|------|
| `crates/wf-sandbox/src/strategy/shell/*`、`runtime.rs` | 沙箱内 shell 执行（自有 `sh -c` 拉起、安全策略），属沙箱安全域，功能有重叠但语义不同，**保持独立** |

### 2.4 主要问题

1. **职责耦合放错层**：2099 行的 PTY/会话引擎被塞在 `predefined/shell`（工具定义层）内，工具注册（`ToolDefinition`/`ToolRegistry`/schema）只是薄壳。引擎本可被 server 终端流、VSCode/web 终端、agent 交互模式直接复用，现在却必须穿过工具注册表。
2. **依赖泄漏**：`portable-pty` + `pty` feature + `ShellEventSink` trait 都埋在 wf-tools；`event_sink.rs` 注释明确"trait 放 wf-tools 是为了不依赖 wf-core"，这是被依赖方向逼出的权宜。
3. **边界模糊**：`command_safety` 被工具层（approval）与引擎层共用，归属不清晰。

## 三、拆分方案

### 3.1 目标结构

新增 `crates/wf-shell/`（依赖 `wf-types`、`wf-common`、`tokio`、`portable-pty`（可选 feature `pty`），遵循 `wf-types → wf-shell → wf-tools → wf-runtime` 的 DAG，不引入任何新环）。

```
crates/wf-shell/src/
├── lib.rs               ← pub mod 声明 + re-export
├── config.rs            ← ShellToolConfig（自 shell.rs 迁入，去掉工具层类型依赖）
├── runner.rs            ← run_command（无状态单命令执行，自 shell.rs 迁入）
├── shell_detector.rs    ← ShellType / ShellDetector（原样迁入）
├── engine.rs            ← TerminalSession / ShellSession / PipeBackend / PtyBackend /
│                          OutputBuffer / BackgroundShellStore（自 predefined/shell/engine.rs 迁入）
├── event_sink.rs        ← ShellEventSink trait（自 predefined/shell/event_sink.rs 迁入）
└── command_safety.rs    ← get_command_decision / CommandDecision（自 command_safety.rs 迁入）
```

### 3.2 归属决策

| 模块 | 归属 | 理由 |
|------|------|------|
| 引擎 / config / detector / event_sink / command_safety | 迁入 `wf-shell` | 引擎层能力，与工具注册无关 |
| 9 个工具定义文件 | 保留在 `wf-tools/src/predefined/shell/` | 工具壳依赖 `wf_tools::predefined::schema::ToolDefinition` 与 registry，属于工具层；改为调用 `wf_shell` 引擎 |
| `approval.rs` 对 `get_command_decision` 的复用 | 经 `wf-tools` re-export 继续使用 | `wf-tools` 公开 `pub use wf_shell::command_safety::*`，避免 import 路径改动扩散 |
| `ShellEventBusBridge` | 保留在 `wf-runtime` | 桥接 `wf_core::EventBus`，依赖方向不变 |
| `EventType::Shell*` | 保留在 `wf-types` | 事件枚举归属不变 |
| sandbox shell strategy | 不动 | 语义独立，见 2.3 |

### 3.3 兼容性处理

- `wf-tools` 保持 `pub use wf_shell::{ShellToolConfig, execute_command_handler}` 等 re-export，现有外部调用（bootstrap、测试、agent 层）路径不变。
- `ShellToolConfig` 中与工具层相关的字段（若有）留在 wf-tools 侧配置或改为泛型注入，设计时确认无工具层类型依赖。
- `pty` feature 随迁至 `wf-shell`；`wf-tools` 的 `pty` feature 改为透传转发（供现有 feature 组合不变）。

## 四、分阶段实施

### Stage 1：新建 wf-shell 并迁移引擎

**做法**：

1. 新建 `crates/wf-shell`，按 3.1 模块划分迁入代码（git mv 保历史）。
2. 迁移 `pty` feature 与 `portable-pty` 可选依赖；根 `Cargo.toml` workspace 注册新 crate。
3. `wf-tools` 侧先通过 re-export 指向 `wf_shell`（引擎代码从 wf-tools 删除，符号路径经 re-export 保持不变）。
4. 编译与单测全量通过（shell 模块 18+ 个用例、event_sink、bridge 集成测试）。

**验收**：`cargo test -p wf-tools -p wf-shell` 通过；`wf-tools` 内不再存在引擎实现文件；`cargo clippy --all-targets --all-features` 通过。

### Stage 2：工具层对接新引擎

**做法**：

1. `predefined/shell/*.rs` 的工具定义改为直接引用 `wf_shell` 的 store/config 类型，删除 wf-tools 内残留的重复类型。
2. `ShellEventSink` 在 wf-tools 侧只保留 re-export；`risk.rs`、`approval.rs`、`handlers.rs` 注册入口照旧。
3. 移除 wf-tools 对 `portable-pty` 的直接依赖（经 feature 透传）。

**验收**：`cargo test -p wf-tools --all-features` 通过；`cargo tree -i portable-pty` 显示只在 `wf-shell` 与（透传的）wf-tools feature 声明中出现。

### Stage 3：接线与端到端验证

**做法**：

1. `wf-runtime` 桥接与 bootstrap 接线确认（`ShellEventBusBridge` 实现 `wf_shell::ShellEventSink`）。
2. 补充集成验证：runtime 级 `get_or_create_shell` → `execute_in_session` → 事件闭环用例（沿用现有 bootstrap 测试）。
3. 更新本文实施记录表与 `wf-tools-gap-supplement-plan.md` 的 Stage 6 说明（终端能力归属 wf-shell）。

**验收**：`cargo test --workspace` 全量通过；`docs` 记录更新完成。

## 五、实施记录

### 已完成实现摘要

**Stage 1（新建 wf-shell，迁移引擎）**
- 新增 `crates/wf-shell/`：`config.rs`（`ShellToolConfig` + `DEFAULT_ALLOWED_COMMANDS` 等常量）、`runner.rs`（`run_command` 无状态单命令执行）、`shell_detector.rs`（原样迁入）、`engine.rs`（引擎 2099 行原样迁入）、`event_sink.rs`、`command_safety.rs`、`error.rs`（新增 `ShellError`/`ShellResult`，4 个语义变体映射自原 `ToolError` 用到的分支）。
- 迁移方式为 `git mv` 保留历史；`pty` feature 与 `portable-pty` 可选依赖迁至 `wf-shell`，根 `Cargo.toml` 注册新 crate。
- `wf-tools` 侧：`src/shell.rs` 收缩为仅 `execute_command_handler`（re-export `ShellToolConfig`/`ShellEventSink`/`shell_detector`）；`lib.rs` 以 `pub use wf_shell::command_safety;` 模块别名保持 `crate::command_safety::*` 旧路径；`predefined/shell.rs` 以 `pub use wf_shell::engine;` 保持 `crate::predefined::shell::engine::*` 旧路径，工具定义文件零改动。

**Stage 2（工具层对接新引擎）**
- `ToolError` 增加 `From<ShellError>`（NotFound/ValidationFailed/Internal/ExecutionError/Io 一对一映射），工具层 `?` 无缝转换。
- `execute_in_session.rs` 尾部表达式改为 `.map_err(Into::into)`。
- `wf-tools/Cargo.toml` 移除 `portable-pty` 直接依赖，`pty = ["wf-shell/pty"]` feature 透传；移除不再使用的 `libc` 依赖。
- 验证：`cargo tree -p wf-tools --features pty -i portable-pty` 显示 portable-pty 仅经 wf-shell 引入。

**Stage 3（接线与端到端验证）**
- `wf-runtime` 的 `ShellEventBusBridge` 实现路径（`wf_tools::predefined::shell::ShellEventSink`）经 re-export 不变，零改动。
- `cargo test --workspace` 全量通过（wf-shell 38 用例、wf-tools 181/185 用例、wf-runtime shell 桥接 5 用例含 bootstrap 端到端）；`cargo clippy --workspace --all-targets --all-features` 零告警。

| 阶段 | 内容 | 状态 |
|------|------|------|
| Stage 1 | 新建 wf-shell，迁移引擎 | ✅ 已完成 |
| Stage 2 | wf-tools 工具层对接新引擎 | ✅ 已完成 |
| Stage 3 | 接线与端到端验证 | ✅ 已完成 |
| Stage 4 | 删除向后兼容 re-export stub，直接引用 wf_shell | ✅ 已完成 |

**Stage 4（删除向后兼容 stub）**
- 移除 wf-tools 侧全部兼容 re-export：`lib.rs` 的 `pub use wf_shell::command_safety;` 与 `pub use shell::shell_detector::{...}`、`src/shell.rs` 的 `pub use wf_shell::{ShellToolConfig, ShellEventSink, shell_detector}`、`predefined/shell.rs` 的 `pub use wf_shell::{engine, event_sink::ShellEventSink}`。
- 更新 wf-tools 内部引用直接使用 `wf_shell::` 路径：`approval.rs`（`wf_shell::command_safety`）、`handlers.rs`/`execute_command.rs`/`predefined/shell.rs`（`wf_shell::config::ShellToolConfig`）、8 个 shell 工具定义文件（`wf_shell::engine::...`）。
- `wf-runtime` 增加 `wf-shell` 直接依赖；`shell_event_bridge.rs` 改用 `wf_shell::event_sink::ShellEventSink`，`bootstrap.rs` 的 `RuntimeConfig.shell` 改用 `wf_shell::config::ShellToolConfig`。
- 验证：`cargo test --workspace` 全量通过；`cargo clippy --workspace --all-targets --all-features` 零告警；`cargo fmt --check` 干净。

## 六、依赖与风险

| 项 | 影响 | 缓解 |
|----|------|------|
| 拆分后 wf-tools 与 wf-shell 间 re-export 层 | 增加一层间接引用 | ✅ Stage 4 已删除全部兼容 stub，工具定义层与 wf-runtime 直接引用 `wf_shell::` 路径；re-export 层已清理 |
| `command_safety` 迁出后 `approval.rs` 引用路径变化 | 改动面扩散 | ✅ 迁移期以 `pub use wf_shell::command_safety;` 保路径；Stage 4 已改为直接 `wf_shell::command_safety::*` |
| 目前唯一消费方是 wf-tools 工具（YAGNI） | 拆分收益暂时不显 | ✅ 拆分为纯结构重组，全量测试与 clippy 通过；引擎现可在工具注册表之外被 server 终端流 / agent 交互模式直接复用 |
| sandbox 与 wf-shell 功能重叠 | 语义混淆 | 明确 sandbox 属安全域不合并，后续如需共享 `sh -c` 拉起逻辑再评估抽公共底层 |
