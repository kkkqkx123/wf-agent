# wf-api 总览与架构

## 1. 定位

`crates/app/wf-api`（约 3.4 万行）是 Rust 迁移的**应用面向 API 门面层**。

- 不含执行引擎：引擎位于 `wf-workflow`（WorkflowCoordinator）、`wf-agent`（AgentLoopCoordinator）、`wf-checkpoint`、`wf-tools`、`wf-script`、`wf-sandbox` 等下层 crate。
- wf-api 是**函数式 API 层**：模块以自由函数 `async fn(&ApiContext)` 形式暴露，而非 trait/struct 方法。
- 消费方：`wf-server`（HTTP 传输层）、CLI、测试。

## 2. 模块结构

顶层 `lib.rs` 声明 10 个模块（`src/` 下均为 `xxx.rs` + `xxx/` 子目录，无 `mod.rs`）：

| 模块 | 行数 | 职责 |
|------|------|------|
| `workflow` | ~8,400 | 工作流定义生命周期、执行控制、检查点、审批、执行图/状态分析 |
| `agent` | ~6,300 | Agent Loop 定义/执行/注册表查询、决策图、错误分析、检查点 |
| `infra` | ~4,500 | 组合根 ApiContext、统一错误、持久化层、事件系统、订阅/流、插件桥 |
| `analysis` | ~4,000 | 错误根因分析、性能剖析、进度跟踪、跨资源搜索、LLM 指标、统计 |
| `builder` | ~3,000 | 类型安全构造器：工作流/节点/Agent 定义/执行/模板 |
| `entity` | ~2,900 | 低层存储实体 CRUD：消息、任务、触发器、变量、交互、技能、资源 |
| `audit` | ~1,200 | 执行审计：迭代/工具调用/LLM 调用/节点执行三维数据源解析 |
| `query` | ~1,000 | 执行记录查询：双层过滤、聚合、分组、CSV/XML/JSON 导出 |
| `llm` | ~2,000 | LLM 直调、Profile 管理、脚本执行、工具执行与注册表 |
| `template` | ~2,300 | 节点/钩子/触发器/Agent 模板与共享模板库 |

## 3. 依赖关系

`Cargo.toml` 依赖 15 个内部 crate（严格 DAG，无循环）：

```
wf-types wf-common wf-storage wf-core wf-config wf-resource wf-metrics
wf-checkpoint wf-llm wf-tools wf-agent wf-workflow wf-execution-shared
wf-script wf-sandbox
```

特征：`sqlite` / `postgres`（转发到 `wf-storage`）。**不依赖 wf-runtime**——由 wf-runtime 组装后通过 `ApiContext::from_runtime_parts` 注入。

## 4. 横切设计模式

### 4.1 Live-entity 优先、持久化记录兜底（贯穿所有查询模块）

```
查询路径：内存运行时注册表（live entity）
        → 兜底：wf-storage 持久化记录（重启后）
        → 兜底：checkpoint 快照（仅 audit，记录被清理时）
```

- 视图携带 `source: "live" | "persisted"` 字段（execution_state）。
- 无数据时返回空/零值而非报错（search、llm_metrics、error_analysis、performance）。

### 4.2 视图结构体（View DTO）

仅 `Serialize` 的只读 DTO，从域状态计算而来，字段命名与 TS 对应类型对齐（camelCase serde 命名）。

### 4.3 组合根 ApiContext

`infra::context::ApiContext` 是唯一组合根，持有：存储上下文、资源注册表（wf-resource）、事件总线、指标注册表、LlmGateway、ToolRegistry、沙箱、两类执行引擎的 live 注册表、持久化层、插件桥、任务注册表。详见 [04-infra基础设施.md](./04-infra基础设施.md)。

### 4.4 三源一致性与降级

- 统计聚合集中在共享 helper（`aggregate_execution_statistics`）复用。
- 持久化：事件/状态快照/指标经统一 `PersistenceLayer`（trait + 缓冲装饰器 + NoOp 兜底）。
- 删除保护：tool/script 删除前扫描引用（`infra::reference`）。

### 4.5 与 TS 的关系

- 模块 doc 显式标注 TS 对应物（`QueryAPI`、`AgentLoopRegistryAPI`、`WorkflowGraphQueryAPI`、`ToolApprovalCoordinator` 等）。
- TS 层为只读行为参考，**禁止修改 `packages/` 下任何文件**。
