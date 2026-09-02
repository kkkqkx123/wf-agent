# wf-cli 剩余问题 - 资产管理深化方案

> 状态：方案设计 / 待评审
> 日期：2026-09-02
> 范围：`crates/app/wf-cli` 资产域（workflow / llm-profile / template / trigger / tool / script）的硬化与补齐
> 关联文档：`docs/plan/cli/wf-cli-剩余问题-分阶段深化总览.md`、`docs/plan/wf-cli-api-gap-analysis.md:4.1/4.3/4.4`、`docs/cli/01-功能清单.md:4.1`、`docs/api/03-workflow域.md`
> 源码锚点：`args.rs:375` `WorkflowSub` / `cmd/workflow.rs:9` / `cmd/llm.rs:9` / `cmd/template.rs:1` / `cmd/trigger.rs:1` / `cmd/tool.rs:1` / `cmd/script.rs:1` / `wf-api/src/workflow.rs:25` / `wf-api/src/template/template_library.rs`

---

## 一、现状与剩余问题

### 1.1 已完成

- `workflow create/update/delete/clone/validate/export/import/version/rollback/graph` 已打通（`cmd/workflow.rs:140-302`），但存在浅实现。
- `llm-profile create/show/update/delete/validate/default/template/export/import` 已打通（`cmd/llm.rs:32-120`）。
- `template list/show/clone`（`cmd/template.rs`）、`trigger list/show/enable/disable`（`cmd/trigger.rs`）、`tool list/show/validate/execute`（`cmd/tool.rs`）、`script list/show/validate/execute`（`cmd/script.rs`）已打通基础查询。

### 1.2 剩余问题清单

| 编号 | 命令面 | 现状 | 剩余缺口 | 对应 `wf-api` | 影响 |
| :--- | :--- | :--- | :--- | :--- | :--- |
| A1 | `workflow create/update --format` | `format` 参数被 `_` 忽略，固定走 JSON | 未支持 `toml/auto`；未复用 `wf-config` 的 `parse_workflow` 校验；错误信息为 `invalid JSON` 而非结构化 `ValidationError` | `workflow::save_workflow` + `wf-config::parse_workflow_file` + `workflow::validate_workflow` | 资产无法从现有 `configs/` TOML 直接导入 |
| A2 | `workflow delete --force` | `force` 被 `_` 忽略；未做 `DeleteReference` 级联校验提示 | 缺二次确认；缺 `--force` 跳过确认；缺 `check_delete_references` 的引用列举 | `infra::reference::check_delete_references` + `workflow::delete_workflow` | 误删风险；与 `wf-server` 行为不一致 |
| A3 | `workflow clone --as` | 已实现但未校验新 ID 冲突 | 缺 `workflow_exists` 预检；缺自动生成 ID 时的命名冲突重试 | `workflow::clone_workflow` + `workflow_exists` | 克隆后 `already_exists` 报错不友好 |
| A4 | `workflow version` | 仅 `list/show/bump`，缺 `diff/changelog` | 未暴露 `list_workflow_versions` 的 `changelog` 聚合；缺 `version::diff` 能力 | `workflow::version::{list, get}` + `versioning::WorkflowChanges` | 无法追溯变更 |
| A5 | `workflow graph` | `reachability` 自行 `analyze_reachability`，缺 `neighbors/summary` 精细度 | 缺 `graph_nodes_by_type`、`graph_node_neighbors`、`graph_reachability` 的 API 侧直通 | `graph_query::{graph_nodes_by_type, graph_node_neighbors}` | 图分析能力停留在占位 |
| A6 | `llm-profile validate/export` | `validate` 仅返回 `{valid, errors}`，`export` 未脱敏提示 | `mask_profile` 未显式说明；`list_templates` 未支持 `filter` | `llm_profile::{validate, mask_profile, list_templates}` | 敏感信息风险 |
| A7 | `template` | 仅 `list/show/clone`，缺 `register/delete/query` | 未暴露 `register_workflow_template / delete_workflow_template / query_by_category|tags|author|featured|popular` | `template_library::{register, delete, query_by_*}` + `template::agent_template` 等 | 模板库只读 |
| A8 | `trigger` | 仅 `list/show/enable/disable`，缺 `register/save/search/statistics` | 未暴露 `save_trigger/register_trigger/search_triggers/trigger_statistics/cleanup` | `entity::trigger::*` + `trigger_execution::*` | 触发器无法闭环 |
| A9 | `tool/script` | 仅 `list/show/validate/execute`，缺 `save/delete/enable/disable/search` | 未暴露 `save_tool/delete_tool/enable/disable/search_tools` | `llm::tool::*` + `llm::script::*` | 工具资产只读 |

---

## 二、修改目标

1. 资产写入链达到 `wf-server` 同等能力：TOML 导入、级联删除防护、版本追溯。
2. 模板/触发器/工具/脚本四域从只读补齐至读写闭环。
3. 所有新增命令统一走 `render_envelope`，错误映射退出码 1/2/3 保持与 `run.rs` 一致。

---

## 三、分阶段修改方案

### 阶段 A1 - 工作流文件管线硬化（前置）

**目标**：`--format json|toml|auto` 真正生效。

**改动**：

- `args.rs:420` `WorkflowSub::Create/Update.file` 增加 `value_hint = ValueHint::FilePath`，`format` 增加 `value_parser = ["json","toml","auto"]`。
- `cmd/workflow.rs:309` `load_workflow_file` 重构为 `load_workflow_file(path, format)`：
  - `auto` 时按扩展名判定（`.toml` → toml，`.json` → json，缺省 json）
  - `toml` 分支调用 `toml::from_str` → `serde_json::Value` → `WorkflowDefinition`（复用 `wf-config` 的 `parse_workflow` 错误归一）
  - 保持 `validate_workflow` 在 `save` 前二次校验，错误展开为 `CliError::Arguments` 的 `ValidationError[]`
- `Validate` 子命令同步复用同一管线，`Text` 输出时打印 `✓ valid` / `✗ errors[]` 行，`Json` 输出时保持现有信封。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/workflow.rs`

**验收**：`wf workflow create --file wf.toml --format toml -o json` 成功；`wf workflow validate bad.toml` 返回 `errors[]` 非空，退出码 1。

---

### 阶段 A2 - 删除防护与克隆健壮性

**目标**：`delete` 与 `clone` 达到生产可用。

**改动**：

- `args.rs:440` `Delete` 的 `force` 保留，新增 `--yes` 别名兼容。
- `cmd/workflow.rs:163` `Delete` 分支：
  - 先 `workflow::workflow_exists(ctx, id).await?` 判空（`failure "workflow-delete"` + `not found`）
  - 调用 `infra::reference::check_delete_references(ctx, id).await?`，若有引用且未 `--force`，返回 `failure` 并列出 `references[]`，提示 `use --force to cascade`
  - `--force` 时二次确认仅在 `OutputFormat::Text + TTY` 时交互式 `ConfirmModal`（复用 `modal.rs:88`），`Json/Silent` 或管道时直接执行
- `Clone` 分支：`as_id` 非空时先 `workflow_exists` 冲突检测，返回 `failure "workflow-clone" already_exists` 而非 `ApiError` 原样透传。

**涉及文件**：`crates/app/wf-cli/src/cmd/workflow.rs`、`crates/app/wf-cli/src/modal.rs`（可选 `ConfirmModal` 复用）

**验收**：`wf workflow delete <id>` 在 Text/TTY 下二次确认；`--force` 跳过确认；`wf workflow clone <id> --as <existing>` 返回 `already_exists` 友好信息。

---

### 阶段 A3 - 版本追溯与图查询补强

**目标**：版本与图查询不再是最小子集。

**改动**：

- `args.rs:507` `WorkflowVersionSub` 新增 `Diff { id, from, to }` 与 `Changelog { id }`。
- `cmd/workflow.rs:233` `Version` 匹配新增：
  - `Changelog` → `version::list_workflow_versions` 后按 `version` 排序聚合 `changes` 字段
  - `Diff` → `get_workflow_version(ctx, id, from)` + `get(..., to)` 后 `serde_json::to_value` 差分（新增/删除/修改 `nodes/edges` 计数）
- `WorkflowSub::Graph` 新增 `--neighbors <node>` 与 `--type <nodeType>`：
  - `--neighbors` → `graph_query::graph_node_neighbors(ctx, id, node).await?`
  - `--type` → `graph_query::graph_nodes_by_type(ctx, id, type).await?`

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/workflow.rs`

**验收**：`wf workflow version changelog <id> -o json` 返回聚合变更；`wf workflow graph <id> --neighbors node-a` 返回邻接表。

---

### 阶段 A4 - LLM Profile 脱敏与模板过滤

**目标**：敏感信息与模板查询对齐 `wf-api`。

**改动**：

- `cmd/llm.rs:94` `Export` 分支显式调用 `llm_profile::mask_profile` 后输出，增加 `Text` 模式的 `warning: api_key masked` 行。
- `args.rs:730` `LlmTemplateSub::List` 增加 `--kind --category --tags --author` 过滤，`cmd/llm.rs:84` 按 `LlmProfileFilter` 透传（需新增 `wf-api::llm::llm_profile::TemplateFilter` 透传或客户端过滤二选一，优先客户端过滤保持无 API 变更）。

**涉及文件**：`crates/app/wf-cli/src/cmd/llm.rs`、`crates/app/wf-cli/src/args.rs`

**验收**：`wf llm-profile export <id> -o json` 的 `api_key` 字段为 `***MASKED***`；`wf llm-profile template list --category x` 过滤生效。

---

### 阶段 A5 - 模板库写闭环

**目标**：`template` 从只读补齐至 `register/delete/query`。

**改动**：

- `args.rs:1074` `TemplateSub` 扩展 `Register { file, kind, format }` / `Delete { id, kind }` / `Query` 已有 `list` 增强。
- `cmd/template.rs:1` 新增：
  - `Register` → `template_library::register_workflow_template` / `agent_template::register_agent_template` 按 `kind` 分发（`kind` 枚举 `workflow|agent|node|trigger`）
  - `Delete` → `template_library::delete_workflow_template` 等
  - `List` 已有 `kind/category/tags/author` 过滤改为直通 `TemplateFilter`（`wf-api::template::template_library::TemplateFilter`）
- 复用 `load_workflow_file` 的 `format` 管线。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/template.rs`

**验收**：`wf template register --file tmpl.json --kind workflow -o json` 成功；`wf template delete <id> --kind workflow` 成功；`wf template list --kind workflow --category demo` 过滤生效。

---

### 阶段 A6 - 触发器/工具/脚本写闭环

**目标**：`trigger/tool/script` 达到 `save/delete/enable/disable/search` 闭环。

**改动**：

- `args.rs:1049` `TriggerSub` 新增 `Register { file }` / `Delete { id }` / `Search { query }` / `Stats`。
- `args.rs:978` `ToolSub` 新增 `Save { file }` / `Delete { id }` / `Enable { id }` / `Disable { id }` / `Search { query }`。
- `args.rs:1012` `ScriptSub` 新增 `Save { file }` / `Delete { id }` / `Enable { id }` / `Disable { id }` / `Search { query }`。
- `cmd/trigger.rs` / `cmd/tool.rs` / `cmd/script.rs` 分别新增分支，直通 `entity::trigger::{save_trigger, delete_trigger, enable, disable, search_triggers, trigger_statistics}` / `llm::tool::{save_tool, delete_tool, enable, disable, search_tools}` / `llm::script::{save_script, delete_script, enable, disable, search_scripts}`。
- 保持 `render_envelope` 统一，`Text` 模式补充 `✓ registered` 行。

**涉及文件**：`crates/app/wf-cli/src/args.rs`、`crates/app/wf-cli/src/cmd/trigger.rs`、`crates/app/wf-cli/src/cmd/tool.rs`、`crates/app/wf-cli/src/cmd/script.rs`

**验收**：每个域至少 `register → list → show → disable → enable → delete` 6 步 e2e（内存 + sqlite 双后端），`search` 返回非空。

---

## 四、依赖与顺序

```
A1 文件管线 ─► A2 删除防护 ─► A3 版本图
                    │
                    ├─► A4 LLM 脱敏 （可与 A2 并行）
                    └─► A5 模板写闭环 ─► A6 触发器/工具/脚本闭环
```

- A1 为前置，A5/A6 依赖 A1 的 `load_*_file` 复用。
- A2/A4 可并行。

---

## 五、测试与验收

| 层 | 用例 |
| :--- | :--- |
| 单测 | `load_workflow_file` 的 `json/toml/auto` 三分支；`check_delete_references` 的 `has_references` 分支（mock storage） |
| 集成 | `tests/workflow_asset.rs`：`create toml → show → graph --neighbors → version bump → changelog → clone → delete --force` 全链路；`tests/template_trigger.rs`：`register → search → delete` |
| 快照 | `OutputFormat::Text/Json` 双格式快照（`insta`），`--force` 时的 `failure` 信封包含 `references[]` |

---

## 六、风险

| 风险 | 缓解 |
| :--- | :--- |
| `toml` 解析与 `serde_json::Value` 互转精度丢失 | 优先复用 `wf-config` 已有 `toml → WorkflowDefinition` 直转，缺失时回退 `toml::from_str<WorkflowDefinition>` 直解 |
| `trigger register` 的双持久化（`wf-server` T5-1 已修）回归 | CLI 侧不自实现去重，直接透传 `save_trigger`，由 `wf-api` 保证 |

