# SDK Agent 能力增强方案

## 一、背景

基于 Lim-Code 的 LLM Agent 实现分析，SDK 需要增强以下 Agent 相关能力，以支持类似 Lim-Code 的 VSCode 应用开发。

## 二、能力差距总览

| 能力领域 | Lim-Code 实现 | SDK 现状 | 优先级 |
|---------|--------------|---------|-------|
| Agent 循环取消支持 | `AbortSignal` | ❌ | P0 |
| 检查点创建钩子 | `createBeforeModelCheckpoint` | ❌ | P0 |
| 上下文裁剪服务 | `ContextTrimService` | 事件触发 | P0 |
| 工具确认暂停机制 | `toolNeedsConfirmation` | 部分支持 | P0 |
| 自动总结服务 | `SummarizeService` | ❌ | P1 |
| 会话元数据管理 | `ConversationMetadata` | ❌ | P1 |
| 消息操作 API | `deleteToMessage` | ❌ | P1 |

## 三、实施计划

### Phase 1: 核心循环增强 (P0)

#### 1.1 AgentLoopService 增强

**目标**：为 AgentLoopService 添加取消支持、检查点钩子、上下文裁剪集成。

**修改文件**：
- `sdk/agent/agent-loop-service.ts`
- `packages/types/src/agent.ts`

**新增接口**：

```typescript
// packages/types/src/agent.ts

/**
 * Agent 循环配置（增强版）
 */
export interface EnhancedAgentLoopConfig extends AgentLoopConfig {
    /** 取消信号 */
    abortSignal?: AbortSignal;

    /** 检查点配置 */
    checkpointConfig?: {
        /** LLM 调用前创建检查点 */
        beforeModel?: boolean;
        /** LLM 调用后创建检查点 */
        afterModel?: boolean;
        /** 工具执行后创建检查点 */
        afterTool?: boolean;
        /** 检查点创建回调 */
        createCheckpoint?: (phase: 'before_model' | 'after_model' | 'after_tool', iteration: number) => Promise<string | null>;
    };

    /** 上下文裁剪配置 */
    contextTrimConfig?: {
        /** 最大 Token 数 */
        maxTokens: number;
        /** 裁剪策略 */
        strategy: 'oldest_first' | 'preserve_system' | 'smart';
        /** 是否启用自动总结检测 */
        enableAutoSummarize?: boolean;
    };

    /** 工具确认处理器 */
    toolConfirmationHandler?: (toolCall: LLMToolCall) => Promise<ToolConfirmationResult>;
}

/**
 * 工具确认结果
 */
export interface ToolConfirmationResult {
    /** 是否批准执行 */
    approved: boolean;
    /** 修改后的参数（可选） */
    modifiedArgs?: Record<string, unknown>;
    /** 拒绝原因（可选） */
    rejectionReason?: string;
}

/**
 * Agent 流事件类型（扩展）
 */
export enum AgentStreamEventType {
    START = 'agent_start',
    THINKING = 'agent_thinking',
    CONTENT_CHUNK = 'agent_content_chunk',
    TOOL_CALL_START = 'agent_tool_call_start',
    TOOL_CALL_END = 'agent_tool_call_end',
    TOOL_CONFIRMATION_REQUIRED = 'agent_tool_confirmation_required',
    ITERATION_COMPLETE = 'agent_iteration_complete',
    CHECKPOINT_CREATED = 'agent_checkpoint_created',
    CONTEXT_TRIMMED = 'agent_context_trimmed',
    CANCELLED = 'agent_cancelled',
    COMPLETE = 'agent_complete',
    ERROR = 'agent_error',
}
```

#### 1.2 ContextTrimService 实现

**目标**：实现智能上下文裁剪服务。

**新增文件**：
- `sdk/agent/services/context-trim-service.ts`

**接口设计**：

```typescript
// packages/types/src/context-trim.ts

/**
 * 上下文裁剪配置
 */
export interface ContextTrimConfig {
    /** 最大 Token 数 */
    maxTokens: number;
    /** 裁剪策略 */
    strategy: 'oldest_first' | 'preserve_system' | 'smart';
    /** 保留最近 N 条消息 */
    preserveRecentCount?: number;
    /** 是否启用自动总结检测 */
    enableAutoSummarize?: boolean;
    /** 触发自动总结的消息数量阈值 */
    autoSummarizeThreshold?: number;
}

/**
 * 上下文裁剪结果
 */
export interface ContextTrimResult {
    /** 裁剪后的消息列表 */
    trimmedMessages: LLMMessage[];
    /** 裁剪起始索引 */
    trimStartIndex: number;
    /** 是否需要自动总结 */
    needsAutoSummarize: boolean;
    /** 移除的 Token 数量 */
    tokensRemoved: number;
    /** 移除的消息数量 */
    messagesRemoved: number;
}
```

#### 1.3 工具确认暂停机制

**目标**：在 Agent 循环中实现工具确认暂停和恢复机制。

**修改文件**：
- `sdk/agent/agent-loop-service.ts`

**实现要点**：
1. 检查工具是否需要确认
2. 发送 `TOOL_CONFIRMATION_REQUIRED` 事件
3. 暂停循环等待确认
4. 根据确认结果继续或终止

### Phase 2: 上下文管理增强 (P1)

#### 2.1 SummarizeService 实现

**目标**：实现自动总结服务，压缩历史消息。

**新增文件**：
- `sdk/agent/services/summarize-service.ts`

**接口设计**：

```typescript
// packages/types/src/summarize.ts

/**
 * 总结配置
 */
export interface SummarizeConfig {
    /** LLM Profile ID */
    profileId: string;
    /** 总结提示词模板 */
    promptTemplate?: string;
    /** 最大总结长度 */
    maxSummaryLength?: number;
}

/**
 * 总结结果
 */
export interface SummarizeResult {
    /** 总结消息 */
    summaryMessage: LLMMessage;
    /** 被替换的消息数量 */
    replacedMessageCount: number;
    /** 节省的 Token 数量 */
    tokensSaved: number;
}
```

#### 2.2 会话元数据管理

**目标**：实现会话级别的元数据管理。

**新增文件**：
- `sdk/agent/services/conversation-metadata-service.ts`

**接口设计**：

```typescript
// packages/types/src/conversation.ts

/**
 * 会话元数据
 */
export interface ConversationMetadata {
    /** 会话 ID */
    id: string;
    /** 标题 */
    title: string;
    /** 创建时间 */
    createdAt: number;
    /** 更新时间 */
    updatedAt: number;
    /** 工作区 URI */
    workspaceUri?: string;
    /** 自定义字段 */
    custom: {
        /** 检查点记录 */
        checkpoints?: CheckpointRecord[];
        /** TODO 列表 */
        todoList?: unknown;
        /** 固定文件 */
        pinnedFiles?: string[];
        /** Skills */
        skills?: string[];
        /** 其他自定义数据 */
        [key: string]: unknown;
    };
}

/**
 * 检查点记录
 */
export interface CheckpointRecord {
    id: string;
    conversationId: string;
    messageIndex: number;
    toolName?: string;
    phase: 'before' | 'after';
    timestamp: number;
    description?: string;
}
```

## 四、架构设计

### 4.1 服务层架构

```
┌─────────────────────────────────────────────────────────────────┐
│                    AgentLoopService (增强)                       │
│                   (工具迭代循环核心)                              │
│  - runToolLoop (流式)                                            │
│  - runNonStreamLoop (非流式)                                     │
│  - 支持取消、检查点、上下文裁剪、工具确认                          │
└─────────────────────────────────────────────────────────────────┘
                                │
            ┌───────────────────┼───────────────────┐
            ▼                   ▼                   ▼
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│   LLMWrapper     │  │   ToolService    │  │ConversationManager│
│   (LLM 通信)     │  │   (工具执行)      │  │   (对话历史)      │
└──────────────────┘  └──────────────────┘  └──────────────────┘
            │                                       │
            ▼                                       ▼
┌──────────────────┐                    ┌──────────────────┐
│ContextTrimService│                    │ SummarizeService │
│   (上下文裁剪)    │                    │   (自动总结)      │
└──────────────────┘                    └──────────────────┘
```

### 4.2 事件流设计

```
AgentLoopService
    │
    ├── [迭代开始]
    │   ├── 检查取消信号 ──→ CANCELLED 事件
    │   ├── 创建检查点 ──→ CHECKPOINT_CREATED 事件
    │   └── 上下文裁剪 ──→ CONTEXT_TRIMMED 事件
    │
    ├── [LLM 调用]
    │   ├── THINKING 事件
    │   └── CONTENT_CHUNK 事件
    │
    ├── [工具调用]
    │   ├── TOOL_CALL_START 事件
    │   ├── 检查确认需求 ──→ TOOL_CONFIRMATION_REQUIRED 事件
    │   └── TOOL_CALL_END 事件
    │
    └── [迭代结束]
        └── ITERATION_COMPLETE 事件
```

## 五、文件修改清单

### Phase 1 文件

| 操作 | 文件路径 | 说明 |
|-----|---------|------|
| 修改 | `packages/types/src/agent.ts` | 新增增强配置接口 |
| 修改 | `packages/types/src/index.ts` | 导出新类型 |
| 修改 | `sdk/agent/agent-loop-service.ts` | 增强循环逻辑 |
| 新增 | `sdk/agent/services/context-trim-service.ts` | 上下文裁剪服务 |
| 新增 | `packages/types/src/context-trim.ts` | 裁剪相关类型 |

### Phase 2 文件

| 操作 | 文件路径 | 说明 |
|-----|---------|------|
| 新增 | `sdk/agent/services/summarize-service.ts` | 自动总结服务 |
| 新增 | `packages/types/src/summarize.ts` | 总结相关类型 |
| 新增 | `sdk/agent/services/conversation-metadata-service.ts` | 会话元数据服务 |
| 新增 | `packages/types/src/conversation.ts` | 会话相关类型 |

## 六、测试计划

### Phase 1 测试

1. **取消信号测试**
   - 测试在 LLM 调用期间取消
   - 测试在工具执行期间取消
   - 测试取消后状态正确性

2. **检查点测试**
   - 测试 beforeModel 检查点创建
   - 测试 afterModel 检查点创建
   - 测试 afterTool 检查点创建

3. **上下文裁剪测试**
   - 测试 oldest_first 策略
   - 测试 preserve_system 策略
   - 测试 Token 计算准确性

4. **工具确认测试**
   - 测试需要确认的工具
   - 测试自动执行的工具
   - 测试确认后恢复执行

### Phase 2 测试

1. **自动总结测试**
   - 测试总结触发条件
   - 测试总结内容质量
   - 测试历史替换正确性

2. **会话元数据测试**
   - 测试元数据创建
   - 测试元数据更新
   - 测试自定义字段存储

## 七、风险与缓解

| 风险 | 影响 | 缓解措施 |
|-----|-----|---------|
| Token 计算不准确 | 上下文裁剪失效 | 使用 tiktoken 库，支持多种模型 |
| 检查点创建性能 | 循环延迟 | 异步创建，不阻塞主循环 |
| 工具确认超时 | 循环卡住 | 添加超时机制，默认拒绝 |
| 总结质量差 | 信息丢失 | 提供可配置的总结模板 |

## 八、后续规划

### Phase 3: 工作区集成 (P2)

- 工作区文件快照
- 文件变更追踪
- 动态上下文注入器

### Phase 4: 流程编排 (P2)

- AgentFlowService 实现
- 重试和删除消息流程
- 完善事件系统
