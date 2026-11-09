# wf-tools 总览

> 范围：`crates/engine/wf-tools/`（工具注册、执行器、预定义工具集、MCP 接入、审批与治理、技能加载）
> 定位：引擎层的工具子系统，为智能体循环与工作流节点提供统一的工具定义、执行与治理能力

`wf-tools` 是模型可调用能力的唯一装配点：预定义工具只在此处声明 schema（单一来源），执行只经由注册表路由到五类执行器，治理（审批、作用域门、暴露分级、展示折叠）全部围绕同一注册表展开。上层（`wf-agent`、`wf-workflow`）只消费注册表与回调接口，不直接实现工具逻辑。

## 依赖关系

`wf-core`（注册表 trait）、`wf-types`（工具/消息/审批类型）、`wf-common`（重试/超时/模板）、`wf-llm`（工具协议无关）、`wf-shell`（shell 执行引擎）、`wf-sandbox`（沙箱网关）、`wf-checkpoint`（执行侧写）、`wf-integration`（代码上下文传输）、`rag-core` / `vector-contracts`（检索后端契约）、`wf-metrics`（工具指标）。内部形成 `predefined → filesystem/shell/skill → executor/mcp/approval → registry` 的单向依赖，无循环。

## 模块一览

| 模块 | 职责 |
|------|------|
| `registry` / `tool_call` / `handlers` | 注册中心、单次/批量调用器、默认装配入口 |
| `executor` | 五类执行器（无状态/有状态/内置/MCP/REST/CLI）与参数校验、超时重试、熔断 |
| `predefined/filesystem` + `filesystem` + `patch` | 文件读写改查八工具与路径作用域、补丁引擎 |
| `predefined/shell` + `shell` | 单命令与后台会话十四工具 |
| `predefined/memory` + `predefined/context` | 会话笔记/长期记忆与压缩归档检索 |
| `predefined/web` / `code_context` / `rag` / `integration` | 联网检索与代码智能 |
| `predefined/agent` / `workflow` / `interaction` / `knowledge` / `utility` / `general` + `callback` | 跨执行调度、问答终结、技能与杂项 |
| `mcp` | MCP 服务连接、注册、元数据、分析与健康 |
| `approval` / `protect` / `scope_gate` / `tool_exposure` | 审批协调、写保护、路径门、暴露分级 |
| `skill` / `skill_fs` | 技能扫描、解析、缓存与资源加载 |
| `display` / `fold` / `risk` / `schema` + 描述生成 | 风险分级、展示摘要、历史折叠、LLM 描述装配 |
| `error` / `failure_protection` / `ignore` / `sequence_matcher` / `general_history` / `harness` | 错误模型、失败熔断、忽略规则、模糊匹配、历史形态归一、fixture  harness |

## 文档索引

| 文档 | 内容 |
|------|------|
| [01-注册与调用](./01-registry-call.md) | 注册中心、调用器、默认装配、回调契约、错误与失败保护 |
| [02-执行器](./02-executor.md) | 执行上下文、五类执行器、校验、超时重试、熔断与 CLI 执行器 |
| [03-文件系统工具](./03-filesystem.md) | 八个文件工具、作用域门、补丁引擎、模糊匹配与忽略 |
| [04-终端工具](./04-shell.md) | 单命令与后台会话工具族、任务注册与生命周期 |
| [05-记忆与上下文](./05-memory-context.md) | 会话笔记、长期记忆、压缩归档检索 |
| [06-网络代码与检索](./06-web-code-rag.md) | 联网检索、代码上下文、RAG 与通用 MCP 调用 |
| [07-调度与交互工具](./07-agent-workflow-interaction.md) | 智能体/工作流调度、问答终结、技能、待办与 general 代理 |
| [08-MCP 接入](./08-mcp.md) | 连接管理、注册表、动态注册、元数据、分析与健康 |
| [09-治理与呈现](./09-governance-display.md) | 审批、写保护、暴露分级、风险分级、展示与折叠、描述生成 |
| [10-技能系统](./10-skill.md) | 技能加载、解析、提示词注入与资源文件 |
| [11-可见性与可用性变化](./visibility-availability.md) | 可见性意图与注册表定义的关系、可用性变化的冻结与通告 |

文件命名规范说明：具体工具类别的介绍才加数字前缀，特定领域说明不需要加数字前缀
