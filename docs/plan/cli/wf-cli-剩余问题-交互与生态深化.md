# wf-cli 剩余问题 - 交互与生态深化方案

> 状态：方案设计 / 待评审
> 日期：2026-09-02
> 范围：`crates/app/wf-cli` 交互与生态域（skill / approval / user_interaction / llm generate / hook / trigger_execution）
> 关联文档：`docs/plan/cli/wf-cli-剩余问题-分阶段深化总览.md`、`docs/plan/wf-cli-api-gap-analysis.md:4.3/4.6`、`docs/api/02-agent域.md:技能与审批`、`docs/architecture/user-interaction/overview.md`
> 源码锚点：`args.rs:749` `SkillSub` / `cmd/skill.rs:1` / `args.rs:1109` `ApprovalSub` / `cmd/approval.rs:1` / `wf-api/src/entity/skill.rs` / `wf-api/src/workflow/approval.rs` / `wf-api/src/workflow/file_approval.rs` / `wf-api/src/llm/llm_profile.rs`

---

## 一、现状与剩余问题

### 1.1 已完成

- `skill list/query/show/enable/disable/scan/reload/clear-cache` 8 子命令（`cmd/skill.rs:14-72`）已打通。
- `approval list/approve/reject` 3 子命令（`cmd/approval.rs:13-45`）已打通 `file_approval` 形态（`list_pending_approvals/approve_changes/reject_changes`）。
- `mini` 与 `run` 的交互路径（`crates/app/wf-cli/src/approval.rs` + `question.rs` + `run.rs:402` 的 `HeadlessApprovalHandler`）已覆盖审批降级与追问拒绝。

### 1.2 剩余问题

| 编号 | 域 | 现状 | 剩余缺口 | 对应 `wf-api` | 影响 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| D1 | skill | 仅 `list/query/enable/disable/scan/reload/clear-cache`，缺 `to_prompt/load_content/list_resources/is_available/query_by_type` | 未暴露 `skill::{to_prompt, load_content, list_resources, is_available, query_by_type}` + `SkillFilter{type, category}` | `entity::skill::*` | 技能无法注入提示词 |
| D2 | approval | 仅 `file_approval`（文件变更审批），缺 `workflow approval`（`check_and_request_approval/execute_tool_with_approval`）与 `file_provenance` 溯源 | 未暴露 `workflow::approval::{check_and_request_approval, execute_tool_with_approval, ApprovalStatus}` + `file_provenance::{get_provenance, list_changes}` | `workflow::approval::*` + `workflow::file_provenance::*` | 审批仅文件维度 |
| D3 | user_interaction | 仅 `mini` 的 `QuestionView`，缺 headless 的 `list/respond/is_pending` 运维能力 | 未暴露 `entity::user_interaction::{list_interactions, respond_interaction, is_pending, get_interaction}` | `entity::user_interaction::*` + `agent::agent_user_interaction::*` | 追问无法在自动化脚本中闭环 |
| D4 | llm | 仅 `llm-profile` 资产管理，缺调试形态 `generate/generate_stream/count_tokens` | 未暴露 `llm::llm_profile::{generate, generate_stream, count_tokens}` 或 `wf-llm` 的 `LlmClient::generate` 薄封装 | `llm::*` + `wf-llm` | LLM 链路无法CLI侧自测 |
| D5 | hook / trigger_execution | 完全未暴露（`hook register/list/show/delete/export`、`trigger_execution list/stats`） | 未暴露 `entity::trigger_execution::{list_trigger_executions, trigger_execution_history, trigger_statistics}` + `infra::handler_chain::hooks` | `entity::trigger_execution::*` + `wf-api::infra::handler_chain` | 钩子与触发执行历史不可见 |

---

## 二、修改目标

1. 技能与审批从"开关"补齐至"内容与溯源"（`to_prompt` + `file_provenance`）。
2. 交互追问从"仅 UI"补齐至"可脚本化"（`interaction list/respond`）。
3. LLM 调试链路在 CLI 侧可独立验证（`llm generate`），不依赖 `wf run` 间接。

---

## 三、分阶段修改方案

### 阶段 D1 - 技能内容化

**改动**：

- `args.rs:749` `SkillSub` 扩展：
  - `IsAvailable { name }` → `skill::is_available(ctx, name).await?`（或 `is_available` 同步）
  - `ToPrompt { name }` → `skill::to_prompt(ctx, name).await?` → `success "skill-to-prompt"` 的 `prompt` 字段
  - `Resources { name }` → `skill::list_resources(ctx, name).await?`
  - `Load { name, resource }` → `skill::load_content(ctx, name, resource).await?`
  - `Query` 已有 `filter`，增强 `--type <type> --category <cat>` 透传 `SkillFilter`
- `cmd/skill.rs:14` 新增分支，直通 `wf-api::entity::skill::*`，`ToPrompt` 的 `Text` 模式直接 `println!("{}", prompt)`（便于管道 `wf skill to-prompt x | pbcopy`）。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/skill.rs`

**验收**：`wf skill to-prompt <name> -o text` 输出提示词；`wf skill resources <name> -o json` 返回资源列表；`wf skill is-available <name> -o json` 返回 `available` 布尔。

---

### 阶段 D2 - 审批溯源

**改动**：

- `args.rs:1109` `ApprovalSub` 扩展：
  - `Provenance { instance, path }` → `file_provenance::get_provenance(ctx, instance, path).await?`
  - `History { instance }` → `file_provenance::list_changes_by_instance(ctx, instance).await?`
  - `Check { instance, tool, params }` → `approval::check_and_request_approval(ctx, instance, tool, params).await?`（`tool/params` 为 `JSON`）
  - `Execute { instance, tool, params }` → `approval::execute_tool_with_approval(ctx, instance, tool, params).await?`
- `cmd/approval.rs:13` 新增分支，直通 `wf-api::workflow::approval::*` + `file_provenance::*`，`Check` 的 `Text` 模式打印 `ApprovalStatus::{Approved, Pending, Rejected}`。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/approval.rs`

**验收**：`wf approval provenance <instance> --path <file> -o json` 返回溯源；`wf approval check <instance> --tool <id> --params '{"x":1}'` 返回 `status`。

---

### 阶段 D3 - 交互追问可脚本化

**改动**：

- `args.rs:202` 新增顶层 `Command::Interaction { sub: InteractionSub }`（与 `Skill/Approval` 并列）。
- `InteractionSub` 定义 `List { execution, status }` / `Show { id }` / `Respond { id, response }` / `IsPending { id }`。
- 新建 `crates/app/wf-cli/src/cmd/interaction.rs` 直通 `entity::user_interaction::{list_interactions, get_interaction, respond_interaction, is_pending}`：
  - `List` → `list_interactions(ctx, Some(Filter{execution_id: Some(execution), status}))`
  - `Respond` → `respond_interaction(ctx, id, response).await?`（`response` 为 `JSON` 或纯文本，按 `InteractionType` 区分）
- 与 `mini` 的 `QuestionView` 共享同一 `UserInteractionHandler`，`Text` 模式 `List` 打印 `id status question` 表格行。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/lib.rs:62`（新增分发）、`crates/app/wf-cli/src/cmd/interaction.rs`（新建）、`crates/app/wf-cli/src/cmd/mod.rs:1`（注册）

**验收**：`wf interaction list --execution <id> -o json` 返回待答追问；`wf interaction respond <id> --response '{"answer": "yes"}' -o json` 响应成功；`wf interaction is-pending <id>` 返回布尔。

---

### 阶段 D4 - LLM 调试命令

**改动**：

- `args.rs:676` `LlmProfileSub` 新增 `Generate { profile, prompt, stream }` / `CountTokens { profile, text }` / `GenerateStream { profile, prompt }`（或合并为 `Generate --stream`）。
- `cmd/llm.rs:10` 新增：
  - `Generate { profile, prompt, stream }` → `llm::llm_profile::generate(ctx, profile, prompt).await?`（或 `wf-llm::LlmClient::generate` 经 `llm_gateway`）
  - `CountTokens { profile, text }` → `llm::count_tokens(ctx, profile, text).await?`
  - 当 `stream=true` 时复用 `run.rs` 的 `LlmDelta` 流式管线，按 `Text` 流式 `print!` + `flush`，`Json` 时聚合成 `success "llm-generate"` 的 `content` 字段
- 增加 `--approve-prefix` 复用（当生成触发工具时走同一审批降级）。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/llm.rs`

**验收**：`wf llm-profile generate --profile <id> --prompt "hello" -o json` 返回 `content`；`--stream -o text` 流式增量可见；`wf llm-profile count-tokens --profile <id> --text "hi"` 返回 `count`。

---

### 阶段 D5 - Hook 与触发执行历史

**改动**：

- `args.rs:202` 新增 `Command::Hook { sub: HookSub }` 与 `TriggerExecutionSub`（或复用 `TriggerSub` 扩展 `History { trigger }` / `Stats { trigger }`）。
- `HookSub` 定义 `List` / `Show { id }` / `Register { file }` / `Delete { id }` / `Export { id, output }`，直通 `wf-api::infra::handler_chain::{list_hooks, get_hook, register_hook, delete_hook}`（需核对实际导出名，缺失时走 `wf-api::template::agent_trigger_template` 替代）。
- `TriggerSub` 扩展 `History { trigger, limit }` → `trigger_execution::list_trigger_executions` + `Stats { trigger }` → `trigger_execution::trigger_execution_history`。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/trigger.rs`、新建 `crates/app/wf-cli/src/cmd/hook.rs`（可选）

**验收**：`wf trigger history --trigger <id> -o json` 返回执行历史；`wf hook list -o json` 返回钩子；`wf hook register --file hook.json` 成功。

---

## 四、依赖与顺序

```
D1 技能内容 ─► D2 审批溯源 ─► D3 交互可脚本化 ─► D4 LLM 调试 ─► D5 Hook/触发历史
     │              │
     └──────────────┴── 可与 C 阶段并行，但 D3 依赖 D2 的 approval 状态语义
```

- D1/D2 可并行，D3 依赖 D2 的 `ApprovalStatus`，D4 独立，D5 依赖 D2 的触发器定义。

---

## 五、测试

| 用例 | 覆盖 |
| :--- | :--- |
| `tests/skill_content.rs` | `skill list → to-prompt → resources → load → query --type x` |
| `tests/approval_provenance.rs` | `approval list → provenance → history → check → execute`（`file_checkpoint` 启用时） |
| `tests/interaction.rs` | `interaction list → respond → is-pending`（`MockLlmClient` 的追问合成） |
| `tests/llm_generate.rs` | `llm-profile generate --stream -o text` 的 `MemorySink` 流式断言（`MockLlmClient` 返回 `LLMEvent::Delta`） |

---

## 六、风险

| 风险 | 缓解 |
| :--- | :--- |
| `skill to_prompt` 依赖 `SkillLoader` 的文件扫描路径 | `SkillSub::ToPrompt` 优先走 `ApiContext.skill_loader` 的已加载缓存，缺失时回退 `scan_skills(ctx, ".")` 后重试一次 |
| `check_and_request_approval` 的 `tool params` 为任意 JSON | `params` 解析为 `serde_json::Value`，`value` 为 `String` 时尝试 `serde_json::from_str` 二次解析，失败保留原字符串 |
| `llm generate --stream` 与 `OutputFormat::Silent` 冲突 | `validate` 中禁止 `generate --stream + Silent`，返回 `CliError::Arguments` |

