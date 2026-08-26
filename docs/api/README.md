# API 层功能分析

> 范围：`crates/app/wf-api`（业务 API 门面层）与 `crates/app/wf-server`（HTTP 传输层）
> 状态：基于当前代码整理的静态分析

## 文档索引

| 文档 | 内容 |
|------|------|
| [01-wf-api-总览与架构.md](./01-wf-api-总览与架构.md) | wf-api crate 定位、依赖关系、横切设计模式 |
| [02-agent域.md](./02-agent域.md) | agent / entity 模块：Agent Loop 查询、执行、注册表、实体 CRUD |
| [03-workflow域.md](./03-workflow域.md) | workflow 模块：定义生命周期、执行控制、检查点、审批、执行分析 |
| [04-infra基础设施.md](./04-infra基础设施.md) | infra 模块：ApiContext、错误、持久化层、事件系统、订阅/流、插件桥 |
| [05-查询与分析.md](./05-查询与分析.md) | query / audit / analysis 模块：记录查询、审计、错误分析、性能、搜索 |
| [06-builder与模板.md](./06-builder与模板.md) | builder / llm / template 模块：构造器、LLM 与脚本/工具 API、模板库 |
| [07-wf-server-HTTP层.md](./07-wf-server-HTTP层.md) | wf-server：路由结构、HTTP 端点清单、信封/SSE/WS/中间件 |

## 核心结论

- **wf-api** 是 Rust 迁移的**应用面向 API 门面层**，是已废弃 TS SDK（`packages/sdk-kit`、`packages/sdk/services`）查询/控制 API 的 Rust 对应实现，约 3.4 万行。
- 不包含执行引擎本身：引擎在 `wf-workflow` / `wf-agent` / `wf-checkpoint` 等下层 crate，wf-api 通过 `ApiContext`（组合根）持有引擎句柄、存储适配器与事件总线，对外提供**函数式 API**（`async fn(&ApiContext)`）。
- 最显著的横切模式：**live-entity 优先、持久化记录兜底**（"重启后可降级"），以及**视图结构体**（仅 Serialize 的 DTO，映射 TS 对应类型）。
- **wf-server** 是纯 HTTP 传输层（axum），约 200 个端点，全部逻辑委托给 wf-api，自身只做信封封装、SSE/WS 帧与中间件。
