# MCP / Skill 模块 RS 版与 TS 版差异分析与改进方案

> 状态：分析完成，改进方案按阶段实施中
> 范围：`crates/wf-tools/src/mcp/`、`crates/wf-tools/src/skill.rs`、`crates/wf-tools/src/predefined/knowledge/`
> 对照：`packages/sdk/services/executors/mcp/`、`packages/sdk/shared/registry/skill-registry.ts`、`packages/sdk/services/skill-loader/`

## 一、MCP 差异分析

### 1.1 客户端协议层

| 能力 | TS（`mcp-client.ts`） | RS（`client.rs`） |
|------|----------------------|-------------------|
| 并发请求 | `pendingRequests` Map，按 id 分发，支持并发 | `wait_for_response` 循环持锁 `receive()`，同一 server 同时只能 1 个 in-flight 请求，其他响应被丢弃 |
| 握手 | connect 发 `initialize` + `notifications/initialized`，保存 `instructions` | 只发 `initialize`（且连接管理器从未调用），**不发 initialized 通知、不保存 instructions** |
| 协议版本 | 硬编码 `2024-11-05` | 同（均硬编码） |

**注意**：RS 的 `McpClient::connect()` 只做 `transport.start()`，`initialize()` 方法从未被连接管理器调用，即 RS 现有实现**完全没有执行 MCP 初始化握手**。

### 1.2 连接管理层

TS `connection-manager.ts`（747 行）比 RS `connection.rs`（572 行）多：

- 生命周期三模式 lazy/eager/keep-alive（RS 定义了 `McpLifecycleMode` 但从未使用）
- idle 超时断开、health check strategy（list-tools/light）、自动重连
- 错误历史、事件发射（`server:connecting/connected/error`）、metadata cache、并发连接锁

### 1.3 功能扩展（TS 有、RS 无）

- `features/registration/dynamic-registrar.ts` — MCP 工具动态注册
- `features/metadata/` — 工具元数据导出、LLM 上下文注入
- `features/analytics/` — 使用统计
- `config-processor/mcp-settings-loader.ts` — 全局/项目 `.wf/mcp.json`/`.agent/mcp.json` 合并加载、preset 支持

### 1.4 集成点（最严重差异）

**RS 的 MCP 完全没有接线**：

- `McpExecutor::with_connection_manager` 无任何调用点，`connection_manager` 恒为 `None`，任何 MCP 工具执行必然报 "No connection manager"
- `McpServerRegistry`/`McpConnectionManager` 在 `wf-runtime` bootstrap、`wf-agent` coordinator 中均未构造
- `use_mcp` 预定义工具未注册为 `Tool` 实例
- `wf-config` 只有 `McpPresets` 索引类型，无 MCP 设置解析
- 审批路径存在但依赖未注入的 mcp settings

## 二、Skill 差异分析

| 能力 | TS `skill-registry.ts` (933 行) | RS `skill.rs` (724 行) |
|------|--------------------------------|------------------------|
| enabled/disabled 状态 | 有 | **无** |
| 变量替换 `{{var}}`（args 参数） | 有 | **无**（skill 工具只有 `skill` 参数） |
| 权限校验（allowedTools vs 可用工具） | 有 | **无** |
| 事件发射 | 有 | **无** |
| 元数据提示词（渐进式披露 L1） | 有 | **无** |
| reload / onCacheClear | 有 | **无** |
| 文件系统抽象 | `SkillFileLoader` trait | 直接 std::fs |
| 资源目录遍历 | **非递归** `listFiles` | **递归**（保留 RS 行为，能力超集） |
| 缓存 | TTL 300s / content 100 / resource 500 | 一致（镜像实现） |

**集成点差异**：TS 在 agent 启动时将已启用 skill 元数据注入 system prompt，且自动添加 `skill` 工具；RS 的 `wf-agent` 完全不引用 skill，LLM 只能靠显式调用 `skill` 工具才发现内容。

## 三、改进方案（分阶段）

### Stage 1（P0）：MCP 配置加载 + 运行时接线

1. **wf-types**：新增 `McpSettings`（`mcpServers: HashMap<String, McpServerConfig>`）
2. **wf-config**：新增 MCP 设置加载模块（对齐 TS 全局/项目优先级合并逻辑），接入 `RuntimeConfig`
3. **wf-tools**：`ToolRegistry` 增加共享 `McpConnectionManager` 注入点，McpExecutor 工厂接线；`from_tool_config` 容忍缺失 server_name（use_mcp 场景）
4. **wf-runtime**：bootstrap 构造 `McpServerRegistry` + `McpConnectionManager`，加载配置、注册 server、注入共享 ToolRegistry、shutdown 时断开；`WorkflowRunner` 改为使用共享 ToolRegistry

### Stage 2（P1）：客户端并发 + 握手补全

1. **transport.rs**：重构 trait — `start()` 返回 `TransportHandle { request_tx, response_rx }`，删除 trait 上的 send/receive
2. **client.rs**：pending-requests Map + 独立分发任务 + 原子 id + 超时；connect 完成 initialize + `notifications/initialized` + instructions 保存
3. **connection.rs**：`connect()` 走完整握手

### Stage 3（P1）：生命周期管理

实现已定义未使用的 `McpLifecycleMode`：`connect_server(name, config, lifecycle, idle_timeout, health_interval)`；lazy 首次使用自动连接；keep-alive 启动健康检查循环；idle 超时断开。

### Stage 4（P1）：MCP 工具动态注册 + LLM 可见性

连接后发现工具，注册为 `ToolType::Mcp`（`config.server_name`），id 形如 `mcp_{server}_{tool}`；description/schema 生成接入 `ToolDescriptionGenerator`；注册 `use_mcp` 通用工具。

### Stage 5（P0）：Skill 功能对齐

1. `SkillLoader` 增加 enabled 状态 + enable/disable API
2. `load_content` 支持变量替换 `{{var}}`（skill 工具新增 `args` 参数）
3. 支持 allowedTools 权限校验（上下文工具列表）
4. skill 工具定义补 `args` 参数，`handle_skill` 透传变量

### Stage 6（P0）：Skill 渐进式披露

`wf-workflow` AGENT_LOOP handler 构建 system prompt 时，若 `skill` 工具在可用工具列表中且 loader 有启用 skill，则注入元数据列表（对齐 TS `injectSkillMetadata`）。

### Stage 7（P1）：验证

全量 `cargo build` / `cargo test` / `cargo clippy` 通过，补充 MCP 客户端并发与 skill 变量替换单测。

## 四、实施记录

（按阶段更新）

| 阶段 | 内容 | 状态 |
|------|------|------|
| Stage 1 | MCP 配置加载 + 接线 | 待实施 |
| Stage 2 | 客户端并发 + 握手 | 待实施 |
| Stage 3 | 生命周期管理 | 待实施 |
| Stage 4 | 工具注册 + LLM 可见性 | 待实施 |
| Stage 5 | Skill 功能对齐 | 待实施 |
| Stage 6 | Skill 渐进式披露 | 待实施 |
| Stage 7 | 验证 | 待实施 |
