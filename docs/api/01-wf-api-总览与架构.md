# wf-api 总览与架构

## 1. 定位

`crates/app/wf-api`（约 39,800 行，含约 25% 单元测试）是 Rust 迁移的**应用面向 API 门面层**。

- 不含执行引擎：引擎位于 `wf-workflow`（WorkflowCoordinator）、`wf-agent`（AgentLoopCoordinator）、`wf-checkpoint`、`wf-tools`、`wf-script`、`wf-sandbox` 等下层 crate。
- wf-api 是**函数式 API 层**：模块以自由函数 `async fn(&ApiContext)`（或低层 `&StorageContext`）形式暴露，而非 trait/struct 方法。
- 同时是**引擎类型门面**：`lib.rs` 大量再导出 `wf-execution-shared`/`wf-tools`/`wf-agent`/`wf-workflow`/`wf-checkpoint` 的公共类型，使下游消费者（wf-server、wf-tui、wf-cli-shared）无需直接依赖引擎 crate。
- 消费方：`wf-server`（HTTP 传输层）、CLI（wf-cli-shared/wf-headless/wf-tui）、测试。

## 2. 模块结构

顶层 `lib.rs`（330 行）声明 11 个模块，每个模块 = `src/<模块>.rs` 声明文件 + `src/<模块>/` 平铺子文件（无 `mod.rs`）。曾经的顶层 `builder/`、`composition/`、`checkpoint-in-workflow` 结构在 "refactor api crate" 提交中**按域解散重组**——builder/composition 不再是独立模块，而是下沉到各业务域：

| 模块 | 行数 | 文件数 | 职责 |
|------|------|--------|------|
| `workflow` | ~10,700 | 23 | 工作流定义生命周期（含 draft 三态）、执行控制、builder、审批、执行图/状态分析 |
| `agent` | ~7,400 | 17 | Agent Loop 执行/查询、决策图、checkpoint、builder、composition、draft |
| `infra` | ~6,000 | 17 | 组合根 ApiContext、统一错误、持久化层、事件系统、订阅/流、插件桥、依赖索引 |
| `analysis` | ~3,900 | 8 | 错误根因分析、性能剖析、进度跟踪、跨资源搜索、LLM 指标、统计 |
| `entity` | ~2,700 | 8 | 低层存储实体 CRUD：消息、任务、交互、变量、技能、资源、执行域解析 |
| `llm` | ~2,400 | 6 | LLM 直调、Profile/Provider 管理、脚本执行、工具执行与注册表 |
| `template` | ~1,400 | 6 | 节点/Agent 模板与共享模板库、节点模板 composition |
| `audit` | 1,270 | 1 | 执行审计：三源解析、迭代/工具/LLM/节点四面目、离线时间线 |
| `query` | 1,036 | 1 | 执行记录查询：双层过滤、聚合、分组、CSV/XML/JSON 导出 |
| `trigger` | ~1,200 | 6 | 触发器域：TriggerTemplate 存储/注册、执行历史、builder、校验 |
| `checkpoint` | ~1,400 | 5 | 共享 checkpoint 域：统一记录 CRUD 与链分析、文件检查点、变更审批、来源查询 |

`workflow.rs`/`checkpoint.rs` 等根文件对高频 API 做扁平再导出（如 `workflow::save_workflow`），其余通过模块路径访问。

## 3. 依赖关系

`Cargo.toml` 依赖 15 个内部 crate（严格 DAG，无循环）：

```
wf-types wf-common wf-storage wf-core wf-config wf-resource wf-metrics
wf-checkpoint wf-llm wf-tools wf-agent wf-workflow wf-execution-shared
wf-script wf-sandbox
```

- 唯一 feature：`lua`（转发到 `wf-sandbox/lua`）。存储后端不再有编译期 feature（sqlite/postgres 由 `wf-storage` 运行时选择）。
- **不依赖 wf-runtime / wf-plugin**：由 wf-runtime 组装后经 `ApiContext::from_runtime_parts` 注入；插件能力经 `infra::handler_chain::PluginHandlerSource` trait 反注入。
- checkpoint 子系统只经 `wf-checkpoint` facade 使用，不触碰内部 `checkpoint-*` crate。

## 4. 横切设计模式

### 4.1 Live-entity 优先、持久化兜底、checkpoint 保底

```
查询路径：内存运行时注册表（live entity）
        → 兜底：wf-storage 持久化记录（重启后）
        → 兜底：checkpoint 快照（仅 audit，执行记录被清理时）
```

- 视图携带 `source: "live" | "persisted" | "unknown"` 字段（execution_state），audit 另有 `CheckpointSnapshot`。
- 无数据时返回空/零值而非报错（search、llm_metrics、error_analysis、performance）。
- 降级方向有一个例外：`analysis::stats::registry(ctx)` 缺 metrics 注册表时报错（门面函数本身依赖它）。

### 4.2 Composition 边界模式

"选项合并 / 模板解析"逻辑集中到各域的 `composition.rs`，**执行 API 保持纯执行器、永不查模板**。四个成员：

| 成员 | 职责 |
|------|------|
| `agent/composition.rs` | Agent 模板解析唯一权威位置：`resolve_run_params` 把调用方意图 + 注册模板 defaults 合成完整 `RunAgentLoopConfig`（精确匹配、无回退、空 id 拒绝），并做 exposure artifacts 渲染与 prompt assembly（稳定 header + 易失 tail 独立消息） |
| `workflow/composition.rs` | 工作流执行选项合并：caller options 必胜，存储 `WorkflowConfig` 补默认，最终兜底启用 checkpoints |
| `template/composition.rs` | 节点模板展开：node config 里的 `template_id`（或 legacy `node_template_id`）→ 模板 `default_config` 深合并垫底，仅对象对对象合并 |
| `trigger/active.rs` | 触发模板激活判定：`enabled` 缺省即启用（与 hook 约定一致），`sort_winners_deterministically` 定序 |

`apply_templates_to_definition` 唯一生产调用点在 `workflow_execution::resolve_execution_graph`（图校验之前展开）。

### 4.3 类型化 builder（PhantomData 阶段状态机）

非法中间态编译期不可表示，builder 分散在各域：

| Builder | 位置 | 阶段 |
|---------|------|------|
| `WorkflowBuilder` | `workflow/builder.rs` | `Empty` → `Building` |
| `NodeBuilder` | `workflow/node_builder.rs` | `NoType` → `Typed` |
| `AgentToolConfigBuilder` | `agent/builder.rs` | `ToolEmpty` → `ToolBuilt` |
| `AgentHookBuilder` | `agent/builder.rs` | `HookNoType` → `HookTyped` |
| `AgentDefinitionBuilder` | `agent/builder.rs` | `DefUnnamed` → `DefNamed` |
| `AgentLoopConfigBuilder` | `agent/builder.rs` | `LoopEmpty` → `LoopConfigured` |
| `NodeTemplateBuilder` | `template/builder.rs` | 无阶段（消费式） |
| `TriggerTemplateBuilder` | `trigger/builder.rs` | 无阶段（消费式） |

`ExecutionBuilder`（`workflow/execution_builder.rs`）与 `AgentExecutionBuilder`（`agent/builder.rs`）不是值构造器，是驱动执行的 fluent 入口。**持久化边界显式**：`build()` 纯值；`save()`/`register()` 落存储。

### 4.4 Draft / Formal / Expired 三态生命周期

workflow 与 agent 两域镜像实现（`workflow/draft.rs`、`agent/agent_draft.rs`）：

- **Draft**：可编辑、允许残缺/悬挂引用、**永不直接执行**；仅 id/name 非空的 parse 级校验。
- **Formal**：必须过 shape + graph + reference closure 完整校验，是执行、默认列表、反向依赖索引的唯一来源。
- **Expired**：上游共享资源更新后，`infra::dependency::check_update_impact` 重验证失败时经 `ctx.mark_stale()` 打在正式工作流上的内存 stale 标记；正式重保存即清除。`lifecycle_of` 判定顺序：draft 存储存在 → Draft；`is_stale` → Expired；否则 Formal。
- promote 管线：draft → 完整 publish 校验（失败保留 draft）→ 正式保存（同名正式版本先快照为版本）→ 删 draft。

### 4.5 统一执行域解析（`entity/execution.rs`）

同一 execution id 可能同时是 agent loop 与 workflow 执行，`ExecutionDomain { AgentLoop, Workflow }` + `resolve_execution` 按成本序探测：双 live registry（内存）→ 持久 execution 记录 → checkpoint 分区存在性探测（ghost id）。双域命中 → `ApiError::Conflict`（要求显式指定域）；端点域已隐含时用 `ensure_execution_domain` 断言。checkpoint 存储以 `entity_type` 列做域分区：`"agent_loop"` / `"checkpoint"`（workflow 域）。

歧义处理按面分策略（有意设计）：**只读聚合面（audit）静默 agent-loop 优先回落、兜底 Unknown 摘要**；**变更面（checkpoint/执行命令）要求显式覆盖，绝不静默猜测**。

### 4.6 双写一致性矩阵（storage + 内存注册表）

多数共享资源的运行时可见性取决于 registry，各资源路径的写入面并不完全一致：

| 资源 | storage | registry | 现状 |
|------|---------|----------|------|
| workflow 定义 | ✔ | ✔（upsert，注册失败=保存失败） | 全管线一致 |
| agent 定义（模板） | ✔（先持久化） | ✔（后注册） | 一致 |
| trigger 模板 save（HTTP 面） | ✔ | ✔（upsert） | 已修复为双写 |
| TriggerTemplateBuilder register | ✔ | ✔（严格注册，重名 Conflict） | 一致 |
| trigger 模板 delete | ✔ | ✘（不清 registry） | **已知漂移** |
| node 模板 builder register | ✔ | ✔ | 一致 |
| node 模板 HTTP save | ✔ | ✘ | **已知漂移**；且 metadata 不含 `default_config`，重启水化后丢失模板默认值 |
| tool 启停 | ✔（原子翻转） | ✔（重注册同步） | 一致 |
| script CRUD | ✔ | ✘（进程级注册表另行） | 设计如此 |
| LLM profile/provider | gateway registry（唯一存储） | — | custom profile 模板另存 persistence 快照键 `custom:llm_profile_templates` |

### 4.7 其他约定

- **视图结构体（View DTO）**：仅 `Serialize` 的只读 DTO，camelCase/snake_case 与存储/引擎类型对齐。
- **确定性输出**：所有列表/聚合带显式 tie-break 排序（score 降序 → type → id 等）+ `BTreeMap`，结果不依赖 HashMap/注册表迭代序。
- **RAII 守卫**：`EventSubscription`、`Unsubscribe`（progress）、`ErrorSubscription`（subscribe_to_errors）Drop 即清理。
- **与 TS 的关系**：模块 doc 标注 `packages/sdk-kit/src/**` 对应物；TS 层为只读行为参考，禁止修改。
