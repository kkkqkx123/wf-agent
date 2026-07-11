# 残留 TODO 分析与实施计划

## 概述

搜索项目源码（packages/ 目录），共发现 **17 处** 代码级别的 TODO 注释（排除工具描述/文档中非代码的"TODO"文本引用），可分为 6 个领域。

---

## 1. 自动审批增强（1 项）

### 1.1 提取审批上下文（P2）

**文件**：`packages/sdk/shared/coordinators/tool-approval-coordinator.ts:792`

**现状**：`extractAutoApprovalContext()` 返回最小化上下文，`isOutsideWorkspace` / `isProtected` 均硬编码为 `false`。

**分析**：该函数目前接收 `_tool` 和 `_contextId` 参数但未使用。要实现完整的上下文提取，需要：
1. 通过 `contextId` 获取当前执行上下文（工作目录、执行环境）
2. 通过 `tool` 参数获取工具执行的文件路径/命令参数
3. 对比文件路径与工作目录边界，判断是否 `isOutsideWorkspace`
4. 检查文件路径是否属于受保护的系统路径

**实施建议**：
- 创建 `WorkspaceBoundaryChecker` 服务，注入 `AutoApprovalCoordinator`
- 在 `ToolContext` 中增加 `workingDirectory` 字段
- 从 `tool.parameters` 中提取文件路径参数（需要统一路径参数命名规范）

---

## 2. 元数据注入（2 项）

### 2.1 工作流元数据注入（P2）

**文件**：`packages/sdk/shared/utils/metadata-injection.ts:312`

**现状**：被注释掉的代码，`injectWorkflowMetadata` 调用被跳过。当前工作流元数据通过 `agent-loop-adapter.ts` 单独处理。

**分析**：需要 `WorkflowRegistry` 可用才能获取工作流定义。当前工作流列表在 `agent-loop-adapter.ts` 中通过 `listWorkflows()` 获取。

**实施建议**：
- 整合 `agent-loop-adapter.ts` 中的工作流列表逻辑到 `metadata-injection.ts`
- 创建 `WorkflowMetadataProvider` 接口，统一元数据源
- 调用 `injectWorkflowMetadata(workflowRegistry, systemPrompt, tools)` 注入

### 2.2 Agent 元数据注入（P2）

**文件**：`packages/sdk/shared/utils/metadata-injection.ts:316`

**现状**：被注释掉的代码，`injectAgentMetadata` 调用被跳过。

**分析**：需要 `AgentRegistry` 可用。当前 Agent 列表没有统一注册机制。

**实施建议**：
- 实现 `AgentRegistry` 接口，支持 Agent 定义注册与查询
- 在 `metadata-injection.ts` 中调用 `injectAgentMetadata(agentRegistry, systemPrompt, tools)`

---

## 3. LOOP 作用域隔离（Phase 2，3 项）

### 3.1 循环变量显式导入（P1）

**文件**：`packages/sdk/workflow/execution/handlers/node-handlers/loop-start-handler.ts:318`

**现状**：注释掉的 `enterSubgraphScope()` 调用。当前所有变量直接写入扁平结构，没有作用域隔离。

**分析**：当前实现不创建作用域隔离，循环内变量直接写入父作用域。`Phase 2` 需要：
1. 在 `LoopStart` 时调用 `enterSubgraphScope()` 创建隔离作用域
2. 根据 `variableInputs` 显式导入父作用域变量
3. 循环体变量修改不会污染父作用域

**实施建议**：
- 在 `loopStartHandler` 创建 `LoopState` 后调用 `enterSubgraphScope()`
- 遍历 `config.variableInputs`，从父作用域读取变量并设置到循环作用域
- 删除后续的 `for (const variable of workflowExecution.variables)` 全量导入（已不再需要）

### 3.2 循环变量显式导出（P1）

**文件**：
- `packages/sdk/workflow/execution/handlers/node-handlers/loop-start-handler.ts:385`
- `packages/sdk/workflow/execution/handlers/node-handlers/loop-end-handler.ts:66`

**现状**：注释掉的 `exitSubgraphScope()` 调用。循环结束时没有作用域清理。

**分析**：与 3.1 对应，需要：
1. 在循环结束时调用 `exitSubgraphScope()` 退出隔离作用域
2. 实现 `variableOutputs` 机制，将循环作用域的特定变量导出到父作用域

**实施建议**：
- 在 `loopStartHandler` 的 `shouldContinue` 为 false 的分支中，调用 `exitSubgraphScope()` 前先导出变量
- 遍历 `config.variableOutputs`（需新增），将循环变量复制到父作用域
- 在 `loopEndHandler` 的 `clearLoopState` 中同样需要处理导出

---

## 4. 消息上下文映射（1 项）

### 4.1 触发子工作流消息上下文映射（P1）

**文件**：`packages/sdk/workflow/execution/handlers/triggered-subworkflow-handler.ts:307`

**现状**：当 `config.inputMapping.messageContexts` 存在时，仅打印警告日志，未实际映射。

**分析**：需要实现消息上下文的引用架构。当前暂未设计消息引用的序列化/反序列化机制。

**实施建议**：
- 设计 `MessageContextRef` 类型，包含 `contextId`、`path` 和 `serialization` 字段
- 在 `triggered-subworkflow-handler.ts` 中实现 `mapMessageContexts()` 函数
- 从父执行上下文中获取指定 `contextId` 的消息，序列化后传递给子工作流

---

## 5. 基础设施缺失（5 项）

### 5.1 Prompt 注册（P2）

**文件**：`packages/sdk/resources/custom/registration.ts:196`

**现状**：`registerCustomPrompts()` 函数接受所有 prompt 定义为成功注册，无实际注册逻辑。

**分析**：需要 `PromptRegistry` 组件。目前 prompt 定义虽然被收集，但没有持久化和查询能力。

**实施建议**：
- 实现 `PromptRegistry` 接口（注册、查询、反注册）
- 在 `registration.ts` 中调用 `promptRegistry.register(promptDef)`
- 在 `metadata-injection.ts` 中集成 prompt 注入

### 5.2 用户上下文构建（P2）

**文件**：`packages/sdk/resources/dynamic/user-context/builder.ts:32`

**现状**：`buildUserContextContent()` 返回空字符串，所有功能未实现。

**分析**：需要 TODO 列表、固定文件、工作区状态等功能。当前资源模块已定义 `DynamicRuntimeContext` 类型，但未集成到执行流程中。

**实施建议**：
- 实现 TODO 列表注入：从执行上下文获取 TODO 列表，格式化为 markdown checklist
- 实现固定文件内容注入：从 `pinnedFiles` 列表读取文件内容
- 实现工作区状态注入：获取当前工作目录的文件变更状态
- 在 `agent-loop-coordinator.ts` 的上下文构建流程中集成 `buildUserContextContent`

### 5.3 标签过滤（P3）

**文件**：`packages/sdk/api/agent/resources/agent-loop-resource-api.ts:216`

**现状**：`getAgentLoops()` 的过滤逻辑中，标签过滤被跳过。

**分析**：需要 `AgentLoopEntity` 支持标签字段。当前 `AgentLoopState` 可能没有 `tags` 字段。

**实施建议**：
- 在 `AgentLoopEntity` 或 `AgentLoopState` 中增加 `tags` 字段
- 在 `getAgentLoops()` 中实现 `filter.tags` 的过滤逻辑

### 5.4 工具启用状态过滤（P3）

**文件**：`packages/sdk/api/shared/resources/tools/tool-registry-api.ts:190`

**现状**：`filterTools()` 的启用过滤被跳过，因为 `Tool` 接口没有 `enabled` 字段。

**分析**：需要 `Tool` 接口增加 `enabled` 字段，或通过外部配置获取启用状态。

**实施建议**：
- 在 `Tool` 接口中增加 `enabled` 字段（可选，默认 `true`）
- 在 `ToolRegistry` 中实现启用/禁用工具的方法
- 在 `filterTools()` 中直接使用 `tool.enabled` 过滤

### 5.5 工作流版本号硬编码（P2）

**文件**：`packages/sdk/workflow/stores/workflow-execution-registry.ts:313`

**现状**：`workflowVersion` 硬编码为 `"1.0"`。

**分析**：需要从 `WorkflowDefinition` 中获取实际版本号。

**实施建议**：
- 在 `WorkflowExecutionEntity` 中增加 `getWorkflowVersion()` 方法
- 在 `WorkflowExecutionEntity` 创建时从 `WorkflowDefinition` 读取版本号
- 在 `workflow-execution-registry.ts` 中使用 `entity.getWorkflowVersion()` 替代硬编码

---

## 6. 其他（3 项）

### 6.1 暂停时长追踪（P2）

**文件**：`packages/sdk/agent/execution/coordinators/agent-loop-coordinator.ts:929`

**现状**：`recordResume()` 调用时传入 `0` 作为暂停时长。

**分析**：需要记录暂停开始时间，以便在恢复时计算暂停时长。

**实施建议**：
- 在 `AgentLoopEntity` 中增加 `pauseStartTime` 字段
- 在 `pause()` 方法中设置 `entity.pauseStartTime = Date.now()`
- 在 `resume()` 方法中计算 `pauseDuration = Date.now() - entity.pauseStartTime`
- 传入 `this.metricsCollector.recordResume(id, pauseDuration)`

### 6.2 暴露 HierarchyRegistry（P2）

**文件**：`packages/sdk/workflow/execution/builders/hierarchy-builder.ts:247`

**现状**：通过 `as unknown` 强制类型转换访问私有 `hierarchyManager`。

**分析**：需要 `WorkflowExecutionEntity` 暴露 `getRegistry()` 方法。

**实施建议**：
- 在 `WorkflowExecutionEntity` 接口中增加 `getRegistry(): ExecutionHierarchyRegistry`
- 在实现中返回 `this.hierarchyManager.registry`
- 删除 `hierarchy-builder.ts` 中的强制类型转换

### 6.3 测试用例更新（P1）

**文件**：`packages/sdk/workflow/execution/handlers/node-handlers/__tests__/agent-loop-handler.test.ts:121`

**现状**：`it.skip("should add input prompt from variables when available")` 测试被跳过。

**分析**：该测试需要适配 `VariableManager` 架构。原始代码使用 `mockEntity.variableStateManager.setVariable('input', 'User query here', 'execution')`。

**实施建议**：
- 更新 mock 实体以支持新的 `VariableManager` API
- 取消 `it.skip` 恢复测试执行
- 验证 `agentLoopHandler` 是否能从变量管理器读取 input 并注入到 prompt 中

---

## 优先级与实施顺序

| 优先级 | 项目 | 原因 | 影响范围 |
|---|---|---|---|
| **P0** | — | 当前无 P0 级别的残留 TODO | — |
| **P1** | LOOP 作用域隔离（3 项） | 变量隔离是正确性保障，当前未隔离可能导致变量污染 | loop-start-handler / loop-end-handler |
| **P1** | 消息上下文映射 | 功能缺失，触发子工作流无法传递消息上下文 | triggered-subworkflow-handler |
| **P1** | 测试用例更新 | 被跳过的测试降低代码覆盖率 | agent-loop-handler.test.ts |
| **P2** | 自动审批上下文提取 | 影响自动审批的准确性 | tool-approval-coordinator |
| **P2** | 暂停时长追踪 | 指标准确性 | agent-loop-coordinator |
| **P2** | 工作流版本号 | 数据完整性 | workflow-execution-registry |
| **P2** | 暴露 HierarchyRegistry | 代码质量（消除强制类型转换） | hierarchy-builder |
| **P2** | 元数据注入（2 项） | 功能增强 | metadata-injection |
| **P2** | 用户上下文构建 | 功能增强 | user-context/builder.ts |
| **P2** | Prompt 注册 | 功能增强 | custom/registration.ts |
| **P2** | 自动审批成本/时间限制（2 项） | 功能增强 | tool-approval-coordinator |
| **P3** | 标签过滤 | 低影响 | agent-loop-resource-api |
| **P3** | 工具启用状态过滤 | 低影响 | tool-registry-api |

## 文件索引

| 文件路径 | 行号 | 内容 | 优先级 |
|---|---|---|---|
| `agent/execution/coordinators/agent-loop-coordinator.ts` | 929 | Store pauseStartTime in entity state | P2 |
| `api/agent/resources/agent-loop-resource-api.ts` | 216 | Add tag filtering | P3 |
| `api/shared/resources/tools/tool-registry-api.ts` | 190 | Add enabled filtering | P3 |
| `resources/custom/registration.ts` | 196 | Implement prompt registration | P2 |
| `resources/dynamic/user-context/builder.ts` | 32 | Implement user context features | P2 |
| `shared/coordinators/tool-approval-coordinator.ts` | 792 | Extract workspace boundary context | P2 |
| `shared/coordinators/tool-approval-coordinator.ts` | 867 | Add cost-based limits | P2 |
| `shared/coordinators/tool-approval-coordinator.ts` | 877 | Add time-window resets | P2 |
| `shared/utils/metadata-injection.ts` | 312 | Implement workflow metadata injection | P2 |
| `shared/utils/metadata-injection.ts` | 316 | Implement agent metadata injection | P2 |
| `workflow/execution/builders/hierarchy-builder.ts` | 247 | Add getRegistry() to entity | P2 |
| `workflow/execution/handlers/node-handlers/loop-start-handler.ts` | 318 | Phase 2: explicit variable import | P1 |
| `workflow/execution/handlers/node-handlers/loop-start-handler.ts` | 385 | Phase 2: explicit variable export | P1 |
| `workflow/execution/handlers/node-handlers/loop-end-handler.ts` | 66 | Phase 2: scope exit handling | P1 |
| `workflow/execution/handlers/triggered-subworkflow-handler.ts` | 307 | Implement message context mapping | P1 |
| `workflow/execution/handlers/node-handlers/__tests__/agent-loop-handler.test.ts` | 121 | Update test for VariableManager | P1 |
| `workflow/stores/workflow-execution-registry.ts` | 313 | Get version from workflow definition | P2 |

**说明**：路径均相对于 `packages/sdk/`。所有路径中的 `/` 无需在文件系统中转义。