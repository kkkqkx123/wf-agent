# wf-cli 剩余问题分阶段深化总览

> 状态：方案设计 / 待评审
> 日期：2026-09-02
> 范围：`crates/app/wf-cli` 对照 `crates/app/wf-api` 约 600+ 公开函数的二次 GAP 盘点后的深化整改
> 基线文档：`docs/plan/wf-cli-api-gap-analysis.md`、`docs/cli/01-功能清单.md`、`docs/plan/cli/wf-cli-分阶段实现方案.md`
> 源码锚点：`crates/app/wf-cli/src/lib.rs:62` / `args.rs:202` / `crates/app/wf-api/src/lib.rs:1` / `crates/app/wf-cli/src/screens.rs:1` / `tui.rs:16`

---

## 一、总体目标与拆分原则

### 1.1 背景

首轮 GAP 分析（`wf-cli-api-gap-analysis.md`）与首轮补齐（2026-09-02，补齐 `approval/task/metrics/analysis + execution 深度分析 + script execute + workflow rollback + Stage7 TUI 骨架`）后，CLI 已从"只读查询 12%"提升至"资产读写 + 执行生命周期 + 可观测性基础 + TUI 骨架"形态。但仍存在三类残余：

1. **浅实现**：参数透传未完成（如 `workflow import --format toml` 被忽略、`event follow` 非流式、`metrics registry` 未配置时直接报错）
2. **域不完整**：同一域内 30-50% API 仍未暴露（如 `variable history/statistics`、`message conversation_history`、`trigger register`）
3. **TUI 未联调**：`screens.rs` 8 屏仅为占位渲染，未接 `DomainAdapter` 真实数据；`modal.rs` 仅有 `Confirm/Help`，缺 `FileViewer/DiffViewer/ModelPicker` 等关键交互

### 1.2 拆分原则

按**变更耦合度**而非优先级拆分，每个子文档对应一个可独立分支、独立验收的类别，避免单文档同时改动 10+ 域导致合并冲突：

| 子文档 | 类别 | 关联域 | 可并行度 |
| :--- | :--- | :--- | :--- |
| `wf-cli-剩余问题-资产管理深化.md` | 资产与模板 | `workflow/llm_profile/template/trigger/tool/script` | 可与 B/C 并行 |
| `wf-cli-剩余问题-执行可观测性深化.md` | 执行可观测性 | `workflow_execution/checkpoint/state_tracker/event/audit/execution_graph` | 可与 A/C 并行 |
| `wf-cli-剩余问题-数据查询与分析深化.md` | 数据与分析 | `variable/message/task/query/stats/performance/error_analysis/search` | 可与 A/B 并行 |
| `wf-cli-剩余问题-交互与生态深化.md` | 交互与生态 | `skill/approval/user_interaction/llm generate/hook/plugin` | 依赖 A-C 完成后 |
| `wf-cli-剩余问题-TUI深化.md` | 全屏 TUI | `screens/modal/tui/replay/theme` | 依赖 A-C 数据面稳定后联调 |

> 约束：所有子文档共享同一输出契约（`OutputEnvelope` + `OutputFormat` + `render_envelope` `crates/app/wf-cli/src/cmd/render.rs`）与同一适配层（`DomainAdapter::bootstrap_for_cli` `domain.rs:36`），禁止各域自定义渲染分支。

### 1.3 演进依赖图

```
资产深化 (A) ─┬─► 数据分析深化 (C) ─┬─► 交互生态深化 (D)
              │                     │
执行可观测 (B)┴─────────────────────┤
                                  │
              TUI 深化 (E) ◄──────┴─── 依赖 A+B+C 的数据面稳定后联调
```

- A/B/C 之间无运行时依赖，仅共享 `args.rs` 枚举命名空间，合并时按字母序解决冲突即可。
- E 必须在 A/B/C 的数据面验收通过后启动，否则 TUI 会因 API 不稳定频繁返工。
- D 依赖 A-C 的底层存储能力（如 `variable history` 需 A 的 workflow 定义稳定）。

---

## 二、整体阶段划分

| 阶段 | 内容 | 产出 | 预计周期 | 验收门槛 |
| :--- | :--- | :--- | :--- | :--- |
| 深化一 | 资产管理深化（A） | `cmd/workflow.rs` 硬化 + `cmd/template.rs` 补齐 + `cmd/trigger|tool|script.rs` 补齐 | 1.5 周 | `wf workflow create --format toml` / `clone` / `delete --force` / `template register` 全链路 e2e |
| 深化二 | 执行可观测性深化（B） | `cmd/execution.rs` 全量 + `cmd/checkpoint|event|audit` 补强 | 2 周 | `wf execution inspect --variables --call-stack` / `event follow --stream` / `checkpoint chain` 双后端通过 |
| 深化三 | 数据查询与分析深化（C） | `cmd/variable|message|query|metrics|analysis|search` 补强 | 1.5 周 | `wf variable history` / `wf query --aggregate group_by` / `wf metrics --top` / `wf analysis errors --similar` |
| 深化四 | 交互与生态深化（D） | `cmd/skill|approval` 补强 + `llm generate` 调试 | 1 周 | `wf skill to-prompt` / `wf approval list --file-provenance` / `wf llm generate --profile` |
| 深化五 | TUI 深化（E） | `screens.rs` 真数据绑定 + `modal.rs` 全栈 + `tui.rs` 会话流式 | 2 周 | `wf --tui` 8 屏可交互、会话流式、alt-screen 恢复、截图快照通过 |

---

## 三、统一改动约束

### 3.1 代码落点

| 变更点 | 文件 | 约定 |
| :--- | :--- | :--- |
| 命令树 | `crates/app/wf-cli/src/args.rs:202` | 仅扩展枚举，不改 `Cli::validate` 之外的校验逻辑；新增域按 `Command::Xxx { sub }` 形式，避免顶层扁平化 |
| 分发 | `crates/app/wf-cli/src/lib.rs:62` | 新增分支仅 `return cmd::xxx::run(&cli, sub).await`，复用 `build_sink` 与 `OutputEnvelope` |
| 业务 | `crates/app/wf-cli/src/cmd/<domain>.rs` | 固定模式 `bootstrap_for_cli → api_context → wf-api 调用 → render_envelope → shutdown`，错误直接 `?` 透传由 `CliError` 统一映射退出码 |
| 共享渲染 | `crates/app/wf-cli/src/cmd/render.rs` | 禁止各域自写 `println!/to_string_pretty` 分支，必须调用 `render_envelope` |
| TUI | `crates/app/wf-cli/src/screens.rs` / `modal.rs` / `tui.rs` / `replay.rs` | `screens` 仅负责布局与 `Frame::render_widget`，数据拉取由 `TuiApp` 在 `event_loop` 前 `tokio::spawn` 预取，禁止在 `draw` 内做 `await` |

### 3.2 测试与质量门槛

- 每个子文档阶段至少 **3 个集成测试**（`crates/app/wf-cli/tests/`，`DomainAdapter::bootstrap` 内存 + `sqlite:/tmp/...` 双后端）
- 增量 `cargo clippy --all-targets --all-features` 零警告，`cargo fmt` 通过
- `OutputFormat::Text/Json/Silent` 三格式均需快照覆盖（`insta` 或 `MemorySink` 断言）

---

## 四、子文档索引

| 序号 | 文档 | 核心问题 | 预期改动量 |
| :--- | :--- | :--- | :--- |
| 1 | `wf-cli-剩余问题-资产管理深化.md` | workflow 格式/级联删除、template/trigger 注册链缺失 | `args.rs` + `cmd/workflow|template|trigger|tool|script|llm.rs` 约 600 行 |
| 2 | `wf-cli-剩余问题-执行可观测性深化.md` | 执行 inspect 不全、checkpoint chain 缺失、event 非流式 | `args.rs` + `cmd/execution|checkpoint|event|audit` 约 500 行 |
| 3 | `wf-cli-剩余问题-数据查询与分析深化.md` | variable/message 仅基础 CRUD、query 高级能力未透传、metrics 仅快照 | `args.rs` + `cmd/variable|message|query|metrics|analysis|search` 约 550 行 |
| 4 | `wf-cli-剩余问题-交互与生态深化.md` | skill 仅 enable/disable、approval 仅 file_approval、缺 llm 调试 | `args.rs` + `cmd/skill|approval|llm` 约 300 行 |
| 5 | `wf-cli-剩余问题-TUI深化.md` | 8 屏占位、modal 仅 2 个、会话无流式 | `screens.rs|modal.rs|tui.rs|replay.rs` 约 800 行 |

---

## 五、风险与跨文档协调

| 风险 | 影响文档 | 缓解 |
| :--- | :--- | :--- |
| `args.rs` 单文件膨胀（已 1496 行） | 全部 | 按域拆 `args/workflow.rs` 等子模块仅在资产深化阶段评估，保持 `lib.rs:1` 扁平 `pub mod` 约束，用 `pub use` 重导出，禁止本轮直接重构 |
| `event follow` 长连接与 `DomainAdapter` 生命周期 | B/E | `follow` 采用 `tokio::select!(event_stream, ctrl_c)`，在 `shutdown` 前显式 `drop(subscription)`，复用 `run.rs:546` 的 `SIGINT` 模式 |
| `workflow delete --force` 级联校验与 `DeleteReference` | A | 直接复用 `wf-api::infra::reference::check_delete_references` 的错误信息，CLI 仅做二次确认（`ConfirmModal` 或 `--force` 跳过），不自实现校验 |
| `metrics registry` 未配置时的空值语义 | C | `metrics show` 未配置时返回 `success "metrics-empty" + {warning}` 而非 `failure`，避免自动化脚本误判 |

---

## 六、变更记录

- 2026-09-02：初版，基于首轮补齐后的二次盘点生成，拆分为 5 个子文档。
