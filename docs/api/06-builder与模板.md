# wf-api builder / llm / template 模块

对应 `src/builder.rs`（6 个文件约 3,000 行）、`src/llm.rs`（4 个文件约 2,000 行）、`src/template.rs`（7 个文件约 2,300 行）。

## 1. builder 模块 — 类型安全构造器

**两种风格**：经典消费式构造器（TemplateBuilder/ExecutionBuilder/AgentDefinitionBuilder）+ **类型级状态机**（`PhantomData` 阶段标记，非法中间态编译期不可表示）。

### 1.1 builder/workflow.rs（525 行）— 工作流定义构造器

- `WorkflowBuilder<S>`：阶段 `Empty`（无节点）→ `Building`（≥1 节点后可用 `build()`/`save()`）。
- 方法：`add_node`（拒绝重复 id）、`add_start_node`/`add_end_node`、`from_config_json/toml`（经 wf-config 解析）、`add_edge`/`add_conditional_edge`（构建时校验节点存在）、`add_variable`。
- `build()`：完整图校验后返回 `WorkflowDefinition`（不持久化）；`save()`：经 `save_workflow` **持久化**（存储 + 执行注册表）。
- 边界（END 必需、LOOP_END 配对等）由共享校验器保证。

### 1.2 builder/node.rs（437 行）— 节点构造器

- `NodeBuilder<S>`：阶段 `NoType` → `Typed`。
- **类型化便捷构造器**直接产出规范 JSON 配置：`start`/`end`/`llm`(profile_id)/`script`(name+risk)/`variable`/`route`(条件分支)/`fork`/`join`/`sync`(并行)/`loop_start`/`loop_end`/`subgraph`/`user_interaction`/`agent_loop`。
- 泛型设置器：`with_name`/`with_description`/`with_config`/`with_execution_config`/`with_checkpoint`。
- 纯值构造，无持久化。

### 1.3 builder/agent.rs（1008 行）— Agent 定义/循环/执行构造器

六对阶段标记构造器：

| 构造器 | 阶段 | 产出 |
|--------|------|------|
| `AgentToolConfigBuilder` | ToolEmpty→ToolBuilt | `AvailableTools`（add_tool、require_approval、allowed_workflows、discoverable/hidden、enable_general_tool） |
| `AgentHookBuilder` | HookNoType→HookTyped | `AgentHookConfig`（类型化构造器：before/after_iteration、before/after_tool_call、before/after_llm_call + condition/event_payload/create_checkpoint 等） |
| `AgentDefinitionBuilder` | DefUnnamed→DefNamed | `AgentDefinition`（校验后产出；`register()` 注册进共享 agent_templates 注册表——仅注册表，不落存储） |
| `AgentLoopConfigBuilder` | LoopEmpty→LoopConfigured | `AgentLoopConfig`（`model` 必需才能到达配置完成态） |
| `AgentExecutionBuilder` | 无阶段 | 驱动循环执行（with_input/on_completed/execute → `agent_execution::run`） |

### 1.4 builder/execution.rs（542 行）— 工作流执行构造器（TS `ExecutionBuilder`）

- `ExecutionBuilder`：`with_input`/`with_max_steps`/`with_timeout`/`with_checkpoints`/`with_on_failure`/`on_node_executed`/`on_progress`/`on_error`。
- `execute`：**先订阅事件总线再 spawn**（零事件丢失），`CallbackPack` 分发回调，等待输出。
- `execute_with_result`：从持久化记录投影状态/时长；`execute_stream`：返回 `(execution_id, ExecutionEventStream)` + 后台回调转发任务；`cancel`。

### 1.5 builder/template.rs（494 行）— 模板工件构造器

- `NodeTemplateBuilder` / `TriggerTemplateBuilder` / `HookTemplateBuilder`：纯消费式构造器，`build()` 经 wf-config 对应校验器校验。
- `register(ctx)`：**双持久化**——存储适配器元数据 + 共享内存注册表（模板立即可执行/引用）。
- `TriggerTemplateBuilder` 注册时经 `trigger_type_of(condition)`（结构式分类：无 condition→schedule、有 eventType→event、空→condition）派生 `trigger_type`。

**关键要点**：定义构造器 `build()` 返回纯值；`save()`/`register()` 显式持久化——工作流 → 存储 + 执行注册表；模板 → 存储 + 共享注册表；Agent 定义 → 仅共享注册表。

## 2. llm 模块

### 2.1 llm/llm.rs（247 行）— 直接 LLM 生成（TS `GenerateCommand`/`GenerateBatchCommand`）

- `generate`（空消息校验；未知 profile → `NotFound`）、`generate_batch`（`join_all` 并行、fail-fast）、`generate_stream`（`Box<dyn MessageStream>`）、`count_tokens`。
- 全部经共享 `LlmGateway` 路由（profile 解析与引擎行为一致）。

### 2.2 llm/llm_profile.rs（579 行）— LLM Profile 管理（TS `LLMProfileRegistryAPI`）

- CRUD + `set_default`/`get_default`；`query` 子串过滤。
- 导入导出带密钥掩码：`MASKED_API_KEY`（`"***HIDDEN***"`），导出时掩码、导入时拒绝掩码键（防密钥往返泄漏）。
- **Profile 模板**：3 个内建（openai-chat gpt-5、anthropic claude-4.5-opus、gemini gemini-2.5-pro）+ 自定义；自定义模板经 `ctx.persistence.save_snapshot` 持久化（键 `custom:llm_profile_templates`）；`create_from_template` 叠加覆盖生成。

### 2.3 llm/script.rs（646 行）— 脚本执行与注册表（TS `ScriptRegistryAPI` + `ExecuteScriptCommand`）

- `execute`：源解析顺序——内联代码 → 模板渲染（wf-script）+ 参数 → 进程级注册脚本（`wf_workflow::lookup_script`）；语言未指定时从注册表推断，默认 shell；经共享 `wf-sandbox` 执行，与 `SCRIPT` 节点语义一致。
- `default_sandbox_config`：**Strict fail-closed** 按语言策略链——shell（静态分析器 + OS hook）、python（AST 分析器）、javascript（vm-context）、lua（静态分析器 + mlua 沙箱）。
- 注册表 CRUD + 启停；`check_script_delete_references`：删除前扫描存储工作流的 SCRIPT/INTERACTIVE_SCRIPT 节点配置。

### 2.4 llm/tool.rs（514 行）— 工具执行与管理（TS `ToolRegistryAPI` + `ExecuteToolCommand`）

- `execute`：默认 30s 超时，向工具上下文附加调用方 `execution_id`，`ToolError::NotFound` → `ApiError::NotFound`。
- `validate_parameters`：JSON-schema 校验（`BaseExecutor::validate_parameters`）。
- `enable`/`disable`：**双写同步**——存储持久化标志 + live 注册表翻转（`sync_registry_enabled`），视图永不漂移。
- `check_delete_references`：扫描工作流级 `available_tools` 或 TOOL 节点配置引用。

## 3. template 模块 — 模板查询/注册表

### 3.1 template_library.rs（546 行）— 共享模板库（工作流 + Agent）

- 注册表支撑的 `WorkflowTemplate`/`AgentTemplate` CRUD（`ctx.registries`，wf-resource：预定义 + 自定义）。
- 统一查询：`query` + by_category/by_tags/by_author；`featured`（public+enabled、按使用量，默认 10）与 `popular_in_category`。
- 使用追踪：`record_usage`/`usage_count`（内存 `ctx.template_usage`）。
- 克隆：`clone_workflow_template`/`clone_agent_template`（新 id `cloned-{gen}` + 新名 + 重新注册）。
- `TemplateSummary`：两类统一摘要投影。

### 3.2 存储支撑的模板注册表（薄门面模式）

| 文件 | 行数 | 说明 |
|------|------|------|
| `node_template.rs` | 161 | NodeTemplate 存储 CRUD + 摘要投影 + JSON 导入导出 |
| `hook_template.rs` | 163 | 工作流级 HookTemplate 存储 CRUD（区别于 agent hook） |
| `agent_hook_template.rs` | 325 | AgentHookTemplate：query/filter/search/**保存前显式校验**（name + hook_type + 非空 event_name）/导入导出 |
| `agent_trigger_template.rs` | 349 | AgentTriggerTemplate：query/CRUD + **类型推断**——`trigger_type_of`（结构式分类：schedule/event/condition）、`infer_action_type`（pause/stop/checkpoint/custom） |
| `agent_template.rs` | 289 | AgentTemplate 查询面（filter 含 `profile_type`）；排行/摘要委托 template_library；**只读**（注册经 builder 或 library） |

## 4. 关键设计要点

1. **阶段标记状态机**：`PhantomData` 编码构造阶段，非法态编译期不可表示。
2. **持久化边界显式**：`build()` 纯值 / `save()`·`register()` 持久化。
3. **多层模板模式**：模板构造器（工件创建）、模板注册表 API（存储支撑）、模板库（使用/排行/克隆）、LLM profile 模板（内建 + 持久化自定义）。
4. **一致性机制**：执行构造器先订阅后 spawn；工具启停双写；脚本/工具删除前引用扫描。
5. **防泄漏**：API key 掩码导出 + 拒绝掩码导入。

## 5. 已知不一致（后续跟进）

- **Trigger 模板双持久化缺口**：`TriggerTemplateBuilder::register()`（builder/template.rs）同时写存储 + 注册 wf-resource 内存注册表（监听器唯一数据源）；而 `agent_trigger_template::save()`（`POST /templates/trigger`）只写存储、**不注册内存注册表**——经 HTTP 保存的 trigger 模板不会进入 `TriggerEventListener`，无法被执行。对齐方案待定（builder 入口已满足运行时需求，HTTP 面以持久化/导出为主）。
- **触发机制与存储记录的脱节**：真实触发由全局 `TriggerTemplate` 注册表 + EventBus 监听器驱动（纯通用事件匹配，见 wf-workflow/trigger_listener.rs）；`TriggerStorageMetadata`（`/triggers`、`/agent-triggers`）无运行时消费者，仅为外部工具的 CRUD 面。已删除的 TS per-loop 语义遗留：`AgentConfig.triggers`、`AgentTriggerBuilder`、`WorkflowDefinition.triggers` 与 `WorkflowBuilder::add_trigger`（含 `TriggerDefinition` 类型及 `wf-types/trigger/schema.rs` 全部 schema 类型，零消费者）。