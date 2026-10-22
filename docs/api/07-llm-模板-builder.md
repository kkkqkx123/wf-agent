# wf-api llm / template / builder

对应 `src/llm/`（5 文件约 2,520 行）、`src/chat.rs`（多轮对话执行链）、`src/template/`（5 文件 + 根工具面约 2,100 行），并综述分散在各域的 8 个类型化 builder。原顶层 `builder/`、`llm.rs` 时代的 hook 模板（HookTemplate/AgentHookTemplate）实体已删除——hook 现为 workflow/agent 定义的内联配置（`CanonicalHookSpec`），wf-server 无 hook 模板端点。

## 1. llm 与 chat 模块

### 1.1 llm.rs（311 行）— 直接 LLM 生成（TS `GenerateCommand`/`GenerateBatchCommand`）

全部经共享 `LlmGateway`（profile 解析与引擎行为完全一致）；未知 profile → `NotFound`，参数畸形 → `Validation`。

- `generate`（空消息 → Validation）、`generate_text`（显式拒绝带 tools 的请求）、`generate_batch`（逐条预检 + `join_all` 并发、**fail-fast on first error**）、`generate_stream`（`Box<dyn MessageStream>`）、`count_tokens`。
- `generate_with_tools_once`：**单发有界工具轮**——委托 `wf-execution-shared` 共享原语 `SingleShotOutcome`（引擎与 API 同一语义），只执行 allowed_tools 内工具、模型发出的其余工具调用变错误、无第二轮、不落会话（checkpoint/approval 的 LLM 评审走此入口）。

### 1.2 chat.rs（599 行）— 多轮对话执行链

chat 会话以 session id 为锚点：会话历史落在 message 存储，轮次只追加用户消息、调用 LLM、记录 assistant 消息并更新 token usage；无工具、无 iteration、无 checkpoint、不发布 agent loop 事件。会话记录写 `AgentExecution`（`execution_type = chat`），模型写 `context.profile_id`，供回放与审计还原当时的执行器。

会话历史按存储时间戳排序，因此一轮内的两条消息必须落在不同的毫秒上：轮次从上一条已存消息继续编号，保证对话顺序全序。消息入账时补齐自己的 id（provider 回复可能不带 id，否则同 id 的多条回复会互相覆盖），该 id 同时是存储主键。

### 1.3 llm_profile.rs（625 行）— Profile 管理（TS `LLMProfileRegistryAPI`）

后端是 gateway 的 profile registry（所有 LLM 请求解析同一视图）。CRUD + `query`（id/name/model 子串、format 精确）；`validate` 与 create/update 判定恒一致（`wf_llm::config::profile`，**api_key 有意不要求**，可每请求注入）。

- **默认 profile**：`set_default/get_default`；**首个注册者隐式默认**；删除当前默认回落第一个剩余。
- **密钥防泄漏**：`MASKED_API_KEY = "***HIDDEN***"`——导出掩码、`import_json` 拒绝掩码键（报错）；`import_all_json` 静默跳过掩码条目只报成功 id。
- **Profile 模板**：内建恰好 3 个（openai-chat→gpt-5、anthropic→claude-4.5-opus、gemini→gemini-2.5-pro，生成参数一律 temperature 0.7 / max_tokens 8192）；自定义模板经 `ctx.persistence` 快照持久化（键 `custom:llm_profile_templates`），name trim 非空 + 跨内建/自定义唯一，内建不可删；`create_from_template` 模板序列化 → overrides JSON 逐键覆盖 → id 缺省 `profile-{now}` → 走 create 全管线。
- `update_with_impact` → `dependency::check_update_impact(Profile)`（更新总是生效，报告形式校验不过的依赖者并 mark_stale）。
- 带 provider_id 的 profile 在 create 时先经 `llm_provider::assemble_check` 合并并回填。

### 1.4 llm_provider.rs（116 行，新）— Provider 定义与模型发现

Provider = 连接模板（base URL/auth/headers/模型发现），按名引用一个线协议 format；profile 经 `LlmProfile::provider_id` 指向，**注册时显式 profile 字段必胜**。

- `list/get/create/delete`：create 校验 id 与 `LlmFormat::Custom(name)` 非空；注册/替换时**驱逐缓存网关客户端**使引用它的 profile 取到新默认值；删除后既有 profile 保留**已合并快照**不回退。
- `list_models`：热路径之外显式调用，api_key 取第一个匹配 profile；**此处发现失败报错**（与装配期降级相反）。
- `assemble_check(ctx, profile)`（供 profile create 调用）：无 provider 直通；发现失败 warn + 原样；目录非空且不含该 model 仅 warn（stale/自定义）；唯一改数据行为 = 回填缺失的 `context_window_size`。

### 1.5 script.rs（923 行）— 脚本执行与注册表（TS `ScriptRegistryAPI` + `ExecuteScriptCommand`）

- `execute` 源解析顺序：**内联 code → template 渲染（wf-script + 参数 default_arguments）→ 进程级注册脚本（`wf_workflow::lookup_script`，与 `ExecuteScript` 触发动作同一注册表）**；仅存 metadata（无内容）明确拒绝。语言：params.language → 注册项推断（shell/python/javascript|js/lua）→ 默认 shell。经共享 `wf-sandbox::SandboxRuntime` 执行，与 `SCRIPT` 节点同路由。
- `default_sandbox_config`：**`SandboxMode::Strict` fail-closed**（注释理由：LLM 生成脚本是最不可信输入，不得静默降级到记录模式；需要宽松必须自带 config——`ScriptExecuteParams.sandbox` 字段上一句 "defaults to a Lenient config" 注释是陈旧的）。按语言策略链（每链至少保留一个 Analysis 策略，保证 runtime 门成立）：shell = 默认 [static-analyzer, os-hook]；python = [ast-analyzer, direct]；javascript = [vm-context]；lua = [static-analyzer, mlua-sandbox]。
- **脚本变更捕获（与 checkpoint 子系统的桥）**：执行前后对 sandbox 允许写路径做 `WorkspaceChangeCollector` diff，非空则 `FileCheckpointManager.apply_workspace_changes`（actor = `Wf:{script_name}`，非法名回退 `Wf:"script"`）；**best-effort**（扫描/apply 失败只 warn，执行失败也照样记录——报错前可能已改文件）；失败路径另记 `script_execution` 指标。
- 输出封顶 `apply_output_cap`：stdout/stderr 各保 tail，spill 文件 `{name}-stdout/-stderr`，截断 note 以 "; " 拼进 error。结果经 `OnceLock` 无锁一次性槽捕获。
- 注册表 CRUD（`&StorageContext`）：`save_script`（纯 storage，不双写）、`save_script_with_impact`、get/delete/list/by_language/search、`set_script_enabled`（原子）+ enable/disable/is_enabled；`check_script_delete_references`（候选 id+name，节点 Script|InteractiveScript，键 `script_name/scriptName`）；`validate` 汇总 `ScriptValidation{valid, errors}`。

### 1.6 tool.rs（545 行）— 工具执行与管理（TS `ToolRegistryAPI` + `ExecuteToolCommand`）

执行走 live `ToolRegistry`（与引擎一致：禁用被拒、内置 handler、同 timeout/retry 语义）；管理经持久化 metadata + live registry **双视图同步（"never drift"）**。

- `execute`：默认 30s 超时（options 缺省注入 `ToolExecutionOptions{timeout: Some(30000)}`）；`ToolExecutionContext::new(execution_id)` 附加调用方上下文；`ToolError::NotFound` → `NotFound`。`execute_with_checkpoint_session` 可注入 `CheckpointSession` 作文件归因观察者（文档明言通用调用必须传 None、**不得臆造 agent 所有权**）。
- `validate_parameters`：JSON-schema 校验（`BaseExecutor::validate_parameters`），返回错误列表不阻断执行；`search_tools` fuzzy。
- `enable/disable(_with_impact)`：**双写**——storage 原子翻转 + `sync_registry_enabled` 重注册 live tool；with_impact 追加 Tool 依赖报告。
- `get_tool_stats`（按 builtin/mcp 类型计数）、`get_tool_enabled_stats -> (enabled, disabled)`；`check_delete_references`：候选 [id, tool_id]；workflow 级 `available_tools`（available ∪ initial）命中记 `"(workflow-level)"` 并跳过该工作流节点级扫描；再对**所有节点类型**扫 config 键 `tool_id/toolId/tool_name/toolName`。

## 2. template 模块

### 2.1 根 template.rs（97 行）— 共享工具面

`NamedTemplate` trait（当前唯一实现：TriggerTemplateStorageMetadata）；泛型 `export_by_name`（按名字导出 pretty JSON，未知 → NotFound）；`parse_import`；`pub(crate) BasicTemplateFilter::matches`（name 大小写不敏感子串、category/author 精确、tags any）。

### 2.2 template_library.rs（1132 行）— 共享模板库（registry 支撑）

读 wf-resource 注册表（预定义 + 自定义），用量计数在 `ctx.template_usage` 内存（`record_usage`/`usage_count`，不落盘）。统一 `query`（TemplateFilter kind + BasicTemplateFilter）、`featured`（public && enabled、usage 降序、`DEFAULT_FEATURED_LIMIT=10`）、`popular_in_category`（仅 enabled，不要求 public）、`TemplateSummary` 两类统一投影、clone（新 id `cloned-{gen}`，description 改 "Clone of {…}"）。

- 两类持久化面现在**相同**：都是 **storage + registry 双写**。agent 模板 delete 先删 storage 再 unregister；workflow 模板 delete 只撤下库里的存在（见下）。
- **库事实由定义承载**：发布标记、是否公开、是否系统、是否启用都是 `WorkflowMetadata` 的字段——定义是唯一能活过重启的形态，注册表条目由定义派生（`library_facts` / `library_entry`）：定义写明的取定义的，没写明的保留条目已有的。注册表条目与工作流存储因此不再各持一份定义。
- **模板库写入路径落盘**：register / update / clone 把定义一并写进工作流存储。工作流编辑器的保存路径写定义、同步注册表条目，编辑一条已发布的工作流不会把它撤下模板库。
- **发布 / 取消发布**：`publish_workflow_template` / `unpublish_workflow_template` 把定义的发布标记落盘并更新注册表条目。预定义资源没有存储形态，发布态只留在注册表条目里，资源装配每次启动重新发布它。
- **移出模板库 ≠ 销毁工作流**：workflow 模板的 delete 清发布标记（落盘）+ 条目离开注册表 + 用量记录清掉，工作流本身留在用户的工作流列表里，随时可以再次发布；真正删除工作流走工作流自己的删除入口。

### 2.3 agent_template.rs（281 行）— Agent 模板查询面

纯 registry 视图同步查询：`AgentTemplateFilter`（Basic 四维 + **profile_type** 精确匹配 `config.profile_id`）；category/tags 取 `template_category` 回退 definition.metadata；featured/popular/summaries 委托 library 过滤 kind。只读——注册经 `save_agent_template`/library/builder。

### 2.4 node_template.rs（248 行）— 节点模板存储 CRUD

纯 storage 薄门面（`&StorageContext`）：save/get/delete/list/by_type、`NodeTemplateSummary`、export（**按 id**，与 trigger 按 name 不同）/import。不触碰注册表（不一致点见文档 01 双写矩阵）。

### 2.5 composition.rs（213 行）与 builder.rs（132 行）

composition 见文档 01 "Composition 边界"（`NODE_TEMPLATE_KEYS = ["template_id","node_template_id"]`、default_config 对象对对象深合并垫底、合并输出剔除两引用键、未知模板 id 原样保留）。`NodeTemplateBuilder`：消费式，`build()` 过 `wf-config` 模板校验器（空 name 拒绝）；`register(ctx)` **双持久化**（storage metadata + 严格注册 registry → 重名 Conflict）。

## 3. builder 总览

八值构造器 + 两执行构造器，模式（PhantomData 阶段标记、边界校验归属）见文档 01。持久化边界矩阵：

| Builder | 位置 | build() | save()/register() 落点 |
|---------|------|---------|------------------------|
| `WorkflowBuilder` | workflow/builder.rs | 两级校验纯值 | `save()` = definition 正式管线（存储 + registry + stale 清除 + 影响报告） |
| `NodeBuilder` | workflow/node_builder.rs | 纯值（13 个类型化便捷构造器） | — |
| `AgentToolConfigBuilder` | agent/builder.rs | `AvailableTools`（空桶序列化为 None；`enable_general_tool` 逃生门） | — |
| `AgentHookBuilder` | agent/builder.rs | `AgentHookConfig`（11 个具名构造点 + condition/event_payload/create_checkpoint） | —（同步 handler 与 trigger 路径无顺序保证） |
| `AgentDefinitionBuilder` | agent/builder.rs | 校验后 `AgentDefinition` | `register()` = `save_agent_template`（**存储 + 注册表**，即 2.2 的 agent 模板双写路径） |
| `AgentLoopConfigBuilder` | agent/builder.rs | `AgentLoopConfig`（`model()` 是进入完成态唯一门；history_normalization 带 KV-cache 搅动警告） | — |
| `NodeTemplateBuilder` | template/builder.rs | 模板工件 | `register()` = 存储 + 严格注册 |
| `TriggerTemplateBuilder` | trigger/builder.rs | 模板工件 | `register()` = scope 预检 + 存储 + 严格注册 |
| `ExecutionBuilder` | workflow/execution_builder.rs | 无（fluent 执行器）：先订阅后 spawn、CallbackPack、`"{id}:callbacks"` 任务注册 | execute/execute_stream/execute_with_result/cancel |
| `AgentExecutionBuilder` | agent/builder.rs | 无（fluent 执行器）：**在 composition 边界 resolve_run_params 后**驱动 `agent_execution::run`，与 HTTP/CLI 共享模板默认值；`on_completed` 回调 | execute |
