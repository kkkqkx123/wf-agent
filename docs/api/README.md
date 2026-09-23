# API 层功能分析

> 范围：`crates/app/wf-api`（业务 API 门面层，约 3.98 万行）与 `crates/app/wf-server`（HTTP 传输层，约 1.27 万行）
> 状态：基于当前代码（"refactor api crate" 之后）整理的静态分析

## 文档索引

| 文档 | 内容 |
|------|------|
| [01-wf-api-总览与架构.md](./01-wf-api-总览与架构.md) | wf-api crate 定位、11 模块地图、依赖、横切设计模式（composition 边界、draft 三态、执行域解析、双写矩阵） |
| [02-agent域.md](./02-agent域.md) | agent / entity 模块：Agent Loop 执行与查询、决策图、手动 checkpoint、composition、实体 CRUD、执行域歧义解析 |
| [03-workflow域.md](./03-workflow域.md) | workflow 模块：定义生命周期与草稿、执行控制、校验、builder、审批闭环、执行图/状态分析 |
| [04-infra基础设施.md](./04-infra基础设施.md) | infra 模块：ApiContext、错误、持久化层、事件、依赖索引、订阅/流、插件桥、共享校验 |
| [05-查询与分析.md](./05-查询与分析.md) | query / audit / analysis 模块：记录查询、三源审计、错误根因、性能、进度、搜索、统计 |
| [06-checkpoint与trigger域.md](./06-checkpoint与trigger域.md) | checkpoint 模块（统一记录/文件/审批/来源）与 trigger 模块（模板注册、执行历史、builder、校验） |
| [07-llm、模板与builder.md](./07-llm、模板与builder.md) | llm / template 模块与跨域 builder 总览：LLM/Profile/Provider/脚本/工具、模板库、类型状态机构造器 |
| [08-wf-server-HTTP层.md](./08-wf-server-HTTP层.md) | wf-server：路由结构、契约端点清单、信封/SSE/WS/中间件/配置 |

## 核心结论

- **wf-api** 是 Rust 迁移的**应用面向 API 门面层**，是已废弃 TS SDK（`packages/sdk-kit`、`packages/sdk/services`）查询/控制 API 的 Rust 对应实现。不含执行引擎本身：引擎在 `wf-workflow` / `wf-agent` / `wf-checkpoint` 等下层 crate，wf-api 通过 `ApiContext`（组合根）持有引擎句柄、存储适配器与事件总线，对外提供**函数式 API**（`async fn(&ApiContext)`）。
- 近期重构要点：原顶层 `builder/`、`composition/` 模块**按域解散**（builder/composition 下沉进 agent/workflow/trigger/template）；`checkpoint` 从 workflow 内部升格为独立共享域（统一记录 + 文件 + 审批 + 来源）；`trigger` 从 entity 升格为独立域（旧 per-loop 触发器实体模型整体删除，改为全局 TriggerTemplate）。
- 最显著的横切模式：**live-entity 优先、持久化兜底、checkpoint 保底**（降级读）；**composition 边界**（模板解析/选项合并集中在 composition，执行 API 保持纯执行器）；**类型化 builder**（`PhantomData` 阶段状态机）；**Draft/Formal/Expired 三态生命周期**（反向依赖索引 + 更新影响检查）；**统一执行域解析**（同 id 双域命中返回 `Conflict`，强制显式覆盖）；**双写一致性矩阵**（storage + 内存注册表，并记录已知漂移点）。
- **wf-server** 是纯 HTTP 传输层（axum），路由清单以契约端点为准，`api/` 按域分目录；全部逻辑委托给 wf-api，自身只做信封封装、SSE/WS 帧、中间件与分层配置加载。文件级 checkpoint 与 webhook 入站网关为新增面。
