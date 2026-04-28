# LLMExecutionCoordinator 重构后续工作

## 概述

本文档记录了 LLMExecutionCoordinator 重构的后续工作方向。重构已完成核心部分，将协调器从 Graph 模块迁移到 Core 层，并移除了所有业务特定的功能。

## 已完成的工作

### 1. 创建通用配置接口
- **位置**: `packages/types/src/llm/execution-config.ts`
- **内容**:
  - `LLMExecutionConfig`: 基础配置接口
  - `WorkflowLLMExecutionConfig`: Workflow 特定配置
  - `AgentLLMExecutionConfig`: Agent 特定配置
- **状态**: ✅ 已完成

### 2. 创建纯净的 LLMExecutionCoordinator
- **位置**: `sdk/core/executors/llm-execution-coordinator.ts`
- **功能**:
  - ✅ LLM 调用协调
  - ✅ 工具调用协调
  - ✅ 对话状态管理
  - ✅ Token 使用监控
  - ✅ 中断检测
  - ✅ LLM 相关事件触发
- **移除的功能**:
  - ❌ 工具审批逻辑
  - ❌ Checkpoint 创建
  - ❌ Thread/Node 特定逻辑
  - ❌ WorkflowConfig 依赖
- **状态**: ✅ 已完成

### 3. 更新调用方
- **文件**: `sdk/graph/execution/handlers/node-handlers/llm-handler.ts`
- **修改**: 使用新的纯净协调器和标准化配置
- **状态**: ✅ 已完成

## 后续工作方向

### 优先级 1：工具审批协调器

**目标**: 创建独立的工具审批协调器，处理工具审批逻辑

**位置**: `sdk/core/executors/tool-approval-coordinator.ts`（建议）

**职责**:
- 检查工具是否需要审批
- 请求用户审批
- 处理审批结果
- 支持参数编辑和用户指令

**接口设计**:
```typescript
interface ToolApprovalParams {
  toolCall: LLMToolCall;
  autoApprovedTools?: string[];
  approvalTimeout?: number;
  requestApproval: (toolCall: LLMToolCall) => Promise<ToolApprovalData>;
}

class ToolApprovalCoordinator {
  async processToolApproval(params: ToolApprovalParams): Promise<ToolApprovalResult>;
}
```

**影响范围**:
- Graph 模块的 LLM 节点处理器
- Agent 模块的 LLM 调用
- DI 容器配置

### 优先级 2：事件构建器迁移

**目标**: 将 LLM 相关的事件构建器移到 Core 层，提高复用性

**当前位置**: `sdk/graph/execution/utils/event/event-builder.ts`

**需要迁移的事件**:
- `buildMessageAddedEvent`
- `buildTokenUsageWarningEvent`
- `buildConversationStateChangedEvent`

**目标位置**: `sdk/core/utils/events/llm-event-builder.ts`（建议）

**注意事项**:
- 保持向后兼容，Graph 模块可以继续使用原位置
- 更新 LLMExecutionCoordinator 中的导入路径
- 考虑是否需要创建通用的事件基类

### 优先级 3：DI 容器更新

**目标**: 更新 DI 容器配置以使用新的 LLMExecutionCoordinator

**文件**: `sdk/core/di/container-config.ts`

**修改内容**:
```typescript
// 更新 LLMExecutionCoordinator 的绑定
container.bind(Identifiers.LLMExecutionCoordinator)
  .toDynamicValue((c: any) => {
    const llmExecutor = c.get(Identifiers.LLMExecutor);
    const toolCallExecutor = c.get(Identifiers.ToolCallExecutor);
    return new LLMExecutionCoordinator(llmExecutor, toolCallExecutor);
  })
  .inSingletonScope();
```

**影响范围**:
- 所有使用 LLMExecutionCoordinator 的模块
- 需要更新构造函数调用

### 优先级 4：Graph 模块适配

**目标**: 在 Graph 模块中处理工具审批和 Checkpoint 创建

**方案 1: 在 NodeExecutionCoordinator 中处理**
- 在调用 LLMExecutionCoordinator 之前/之后处理业务逻辑
- 优点：集中管理 Graph 特有逻辑
- 缺点：增加 NodeExecutionCoordinator 的复杂度

**方案 2: 创建 GraphLLMExecutionWrapper**
- 包装 LLMExecutionCoordinator，添加 Graph 特有功能
- 优点：职责分离清晰
- 缺点：增加一层包装

**建议**: 采用方案 1，在 NodeExecutionCoordinator 中处理

### 优先级 5：Agent 模块适配

**目标**: 确保 Agent 模块可以使用新的 LLMExecutionCoordinator

**工作内容**:
- 使用 `AgentLLMExecutionConfig` 配置
- 实现 Agent 特定的事件处理
- 测试 Agent 模块的 LLM 调用流程

### 优先级 6：测试和文档

**测试**:
- 为新的 LLMExecutionCoordinator 编写单元测试
- 测试 Graph 模块的集成
- 测试 Agent 模块的集成

**文档**:
- 更新架构文档
- 更新 API 文档
- 编写迁移指南

## 架构设计原则

### LLM 调用本身包含的功能（保留在 Core 层）
- ✅ 消息事件
- ✅ Token 统计事件
- ✅ 对话状态变化事件
- ✅ Token 统计组件
- ✅ 中断检测

### 业务特定的功能（移到业务层）
- ❌ 工具审批（业务逻辑）
- ❌ Checkpoint 创建（持久化机制）
- ❌ Thread/Node 管理（Graph 特有）
- ❌ Workflow 配置（Graph 特有）

## 配置标准化

### 统一配置接口
所有模块使用 `LLMExecutionConfig` 作为基础配置：

```typescript
// Workflow 使用
const workflowConfig: WorkflowLLMExecutionConfig = {
  profileId: 'gpt-4',
  maxToolCallsPerRequest: 3,
  workflowId: 'wf-123',
  nodeId: 'node-456',
  threadId: 'thread-789'
};

// Agent 使用
const agentConfig: AgentLLMExecutionConfig = {
  profileId: 'gpt-4',
  maxToolCallsPerRequest: 5,
  agentId: 'agent-123',
  sessionId: 'session-456'
};
```

## 时间线建议

### 第一阶段（1-2 天）
- 完成工具审批协调器
- 更新 DI 容器配置
- Graph 模块适配

### 第二阶段（1 天）
- 事件构建器迁移
- Agent 模块适配

### 第三阶段（1-2 天）
- 测试和文档
- 性能优化

## 风险和注意事项

1. **向后兼容性**: 确保现有代码不受影响
2. **性能影响**: 监控重构后的性能变化
3. **测试覆盖**: 确保所有场景都有测试覆盖
4. **文档同步**: 及时更新相关文档

## 参考资料

- [架构设计文档](../architecture/README.md)
- [SDK 开发指南](../../README.md)
- [类型定义文档](../../../packages/types/README.md)
