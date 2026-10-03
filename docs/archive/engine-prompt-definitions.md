# Engine 层提示词定义盘点

本文记录 engine 层（`crates/engine/`）各 crate 中提示词的定义位置、迁移决策与架构约定，作为后续修改提示词相关代码时的参考。

## 架构约定

项目采用「**内容归 wf-resource，逻辑归执行层**」的分工：

- 所有面向模型的成段提示词文本（agent 模板、摘要、规划器、工具可见性文案）统一定义在 `wf-resource/src/predefined/`，注册进统一模板注册表，支持用户自定义资源覆盖与热更新。
- `wf-execution-shared`（`agent_prompt.rs`）提供提示词的解析与组装管线（resolve / enrich / 注入顺序），不持有提示词内容。它是 agent loop 与轻量 LLM 节点两条路径的单一入口，防止两条路径的提示词组装逻辑漂移。
- `wf-agent` / `wf-workflow` 只做占位符注入（`{SKILLS_METADATA}`、`{DISCOVERABLE_TOOLS_METADATA}`）与渲染调用，不内嵌提示词文本。

## 提示词定义清单

### 已集中在 wf-resource（标准做法）

| 位置 | 内容 |
|---|---|
| `wf-resource/src/predefined/agent_templates/` | main / worker / explorer / reviewer / executor 五套内置 Agent 模板的完整 system prompt（如 `WORKER_AGENT_SYSTEM_PROMPT`），每个模板带 prompt 版本号常量（如 `WORKER_AGENT_PROMPT_VERSION`），checkpoint 恢复时用于识别驱动会话的提示词版本 |
| `wf-resource/src/predefined/resource_assembler/workflow.rs` | `DEFAULT_PLANNER_PROMPT`（goal-review 规划器提示词） |
| `wf-resource/src/predefined/workflow/child/prefetch.rs` | `PREFETCH_AGENT_SYSTEM_PROMPT`（子工作流预取 agent） |
| `wf-resource/src/predefined/workflow/child/summary_stage.rs` | `DEFAULT_LLM_SUMMARY_PROMPT`（上下文压缩摘要提示词） |
| `wf-resource/src/predefined/tool_visibility.rs` | 4 个 tool-visibility 模板（activation / block / discoverable_metadata / general_description）以及通用兜底文案 `GENERIC_VISIBILITY_CONTENT` + `generic_visibility_text()`，常量同时作为渲染引擎无注册表时的回退 single truth |
| `wf-resource/src/predefined/prompts.rs` | `system.default` / `system.code` / `system.agent` 三个系统提示词模板，通过 fragments 片段组合 |
| `wf-resource/src/predefined/fragments.rs` | `fragments.role.*` / `fragments.capability.*` / `fragments.constraint.*` 等提示词片段库 |
| `wf-resource/src/predefined/tool_descriptions.rs` | 内置工具的 LLM-facing 描述 |

### 保留在 wf-tools（有意不迁移）

| 位置 | 内容 | 不迁移的原因 |
|---|---|---|
| `wf-tools/src/skill.rs`（`generate_skill_metadata_prompt`） | `"Available skills:"` 技能元数据注入块的文案框架 | 结构化数据的程序化渲染，格式与数据强耦合；迁到 wf-resource 会引入反向依赖或异步渲染开销 |
| `wf-tools/src/tool_description_generator.rs` | discoverable tools 元数据块（`DISCOVERABLE_TOOLS_METADATA_PLACEHOLDER`）的生成逻辑与文案 | 同上 |

占位符锚点的规范文本（`SKILLS_METADATA_PLACEHOLDER` 等）统一定义在 `wf-common::template`，各注入阶段引用同一拼写，防止锚点漂移。

### 曾内嵌、现已迁移

- `wf-workflow/src/handler/tool_visibility.rs` 中原有的两处兜底字符串 `"Tool visibility changed ({action}): ..."` 已迁移至 `wf-resource/src/predefined/tool_visibility.rs`（`GENERIC_VISIBILITY_CONTENT` / `generic_visibility_text()`），wf-workflow 侧仅通过 `wf_resource::generic_visibility_text` 调用。后续修改通用可见性文案只需改 wf-resource 一处。

## 新增提示词的放置规则

1. 面向模型的成段提示词（system prompt、模板文案）→ `wf-resource/src/predefined/` 下的对应类别模块，注册为模板或片段。
2. 程序化生成的结构化注入块（元数据列表等）→ 留在所属执行 crate，与生成逻辑同文件。
3. 占位符锚点拼写 → `wf-common::template`，禁止在各 crate 内重复定义字面量。
4. 回退文案必须与注册模板常量同源（single truth），不允许在调用方 `format!` 出提示词文本。
