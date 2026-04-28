# Prompt Tree 设计思想借鉴分析

本文档分析 Prompt Tree 项目的设计思想，并探讨如何将其核心设计理念应用到当前项目的 SDK 提示词处理和 `packages/prompt-templates` 包中。

## 一、Prompt Tree 核心设计思想回顾

### 1.1 从线性对话到树形对话

Prompt Tree 将传统线性对话模式升级为**节点化的 DAG（树 + 分支）结构**：

- 每条消息都是图中的一个**节点（Node）**
- 节点之间通过 `parentId` 建立父子关系
- 支持从任意历史节点创建新分支
- 同一父节点可以有多个子节点（多分支对比）

### 1.2 显式上下文管理

引入 **Context Box（上下文组装台）** 概念：

- Context Box 是独立容器，包含多个 **ContextBlock**
- ContextBlock 可以是 **NodeBlock**（引用树中节点）或 **FileBlock**（文件内容）
- 用户可以**显式地**选择哪些节点加入上下文
- 支持**拖拽排序**、**预览最终上下文**、**查看 Token 占用**

### 1.3 压缩与解压机制

- `COMPRESSED` 类型节点表示被压缩的连续消息序列
- `summary` 字段存储压缩后的摘要
- `compressedNodeIds` 记录被压缩的原始节点 ID

## 二、当前项目架构分析

### 2.1 SDK 提示词处理架构

当前 SDK 的提示词处理采用**分层架构**：

```
┌─────────────────────────────────────────┐
│            API Layer                     │
│  prompt-template-loader / config-parser │
├─────────────────────────────────────────┤
│         Core Prompt Layer                │
│  system-prompt-resolver /               │
│  build-initial-messages                 │
├─────────────────────────────────────────┤
│         Template Registry                │
│  PromptTemplateRegistry (Singleton)     │
├─────────────────────────────────────────┤
│         Static Templates                 │
│  @wf-agent/prompt-templates          │
└─────────────────────────────────────────┘
```

**关键组件**：

| 组件 | 职责 |
|------|------|
| `PromptTemplateRegistry` | 模板注册、检索、渲染（单例模式） |
| `system-prompt-resolver` | 系统提示词解析（templateId > direct string） |
| `build-initial-messages` | 初始消息构建（system + user + history） |
| `MessageHistory` | 消息历史管理，支持批量可见性控制 |

### 2.2 packages/prompt-templates 架构

采用**纯静态定义包**设计：

```
packages/prompt-templates/
├── src/
│   ├── types/           # 类型定义（纯类型）
│   │   ├── template.ts  # PromptTemplate, VariableDefinition
│   │   └── composition.ts
│   ├── templates/       # 模板内容定义（纯常量）
│   │   ├── fragments/   # 可复用片段
│   │   ├── dynamic/     # 动态上下文模板
│   │   ├── rules/       # 规则模板
│   │   └── tools/       # 工具描述模板
│   └── constants/       # ID 常量
```

**设计原则**：
- 纯静态包，无运行时逻辑
- 无状态，无注册表
- 可导入、可覆盖

### 2.3 当前消息历史管理

`MessageHistory` 类提供：

- 基本的增删查操作
- **批量可见性控制**（通过 `MessageMarkMap`）
- 状态快照与恢复
- Checkpoint 内存优化

## 三、功能定位与集成策略

### 3.1 核心定位：补充 API 而非替代

经过分析，Prompt Tree 的设计思想**不应取代**现有的上下文管理体系，而应作为**独立的上下文构建 API** 存在。

#### 两套体系的不同定位

| 维度 | 现有上下文体系 | 新增 Context Builder API |
|------|---------------|-------------------------|
| **核心职责** | 历史记录、检查点回退 | 上下文重组、手动干预 |
| **使用场景** | 自动化工作流 | 特定干预场景 |
| **生命周期** | 贯穿整个执行过程 | 按需创建、一次性使用 |
| **数据来源** | 自动追加消息 | 手动选择/合并消息 |
| **主要用户** | Agent/Graph 自动使用 | 协调 Agent、压缩服务调用 |

#### 为什么不应取代现有体系

1. **自动化工作流不需要**：正常的 Agent 循环或 Graph 工作流中，消息按顺序追加，无需手动干预
2. **现有体系职责清晰**：`MessageHistory` 的核心是历史记录和检查点回退，这已经足够
3. **复杂度不匹配**：树形结构、分支管理在自动化场景下是过度设计

### 3.2 适用场景分析

#### 场景一：上下文压缩与剪枝

当对话包含大量中间信息，需要压缩上下文时：

```
原始历史：
[System] -> [User: 问题] -> [Assistant: 分析] -> [Tool: 中间结果1] 
-> [Assistant: 继续分析] -> [Tool: 中间结果2] -> ... -> [Assistant: 最终答案]

压缩后新 Batch：
[System] -> [Compressed: 问题分析摘要] -> [Assistant: 最终答案]
```

**关键需求**：
- 选择性提取关键消息
- 重写/压缩中间步骤
- 构建新的干净上下文

#### 场景二：多智能体协作

协调 Agent 需要合并多个子 Agent 的上下文：

```
协调 Agent 需要构建新上下文：
┌─────────────────┐
│  Agent A 历史    │ ─┐
└─────────────────┘  │
┌─────────────────┐  ├──> [协调 Agent 新上下文]
│  Agent B 历史    │ ─┤
└─────────────────┘  │
┌─────────────────┐  │
│  Agent C 历史    │ ─┘
└─────────────────┘
```

**关键需求**：
- 从多个来源选择消息
- 合并、去重、排序
- 构建统一的新上下文

#### 场景三：批处理任务衔接

批处理任务之间需要传递精简上下文：

```
Task 1 完成 -> 提取关键结论 -> 构建 Task 2 的初始上下文
```

### 3.3 设计原则

基于以上分析，新增功能应遵循以下原则：

1. **独立性**：作为独立 API，不侵入现有 `MessageHistory`
2. **一次性**：构建的 Context Box 是临时性的，用完即弃
3. **可组合**：支持从多个来源（历史、文件、模板）组装
4. **可审计**：记录构建过程，便于调试

## 四、具体设计方案

### 4.1 Context Builder API 设计

作为独立的服务层 API，而非核心数据结构：

```typescript
// sdk/core/services/context-builder-service.ts

interface ContextBuilderOptions {
  maxTokens?: number;
  preserveSystemPrompt?: boolean;
}

interface ContextSource {
  type: "history" | "file" | "template" | "custom";
  // history: 从 MessageHistory 提取
  historyRef?: { history: MessageHistory; indices?: number[] };
  // file: 文件内容
  fileContent?: string;
  // template: 模板渲染
  templateRef?: { id: string; variables: Record<string, unknown> };
  // custom: 自定义消息
  customMessage?: LLMMessage;
}

class ContextBuilderService {
  createBuilder(options?: ContextBuilderOptions): ContextBuilder;
}

class ContextBuilder {
  addSource(source: ContextSource): this;
  addSources(sources: ContextSource[]): this;
  compress(options: CompressionOptions): this;
  build(): LLMMessage[];
  getTokenCount(): number;
}
```

### 4.2 使用示例

#### 压缩剪枝场景

```typescript
const builder = contextBuilderService.createBuilder({ maxTokens: 4000 });

builder
  .addSource({
    type: "history",
    historyRef: { history: agentLoop.getMessageHistory() },
  })
  .compress({
    strategy: "keep-ends",
    preserveRoles: ["system", "user"],
    summarizeMiddle: true,
  });

const compressedMessages = builder.build();
agentLoop.startNewBatch();
agentLoop.addMessages(compressedMessages);
```

#### 多智能体协调场景

```typescript
const builder = contextBuilderService.createBuilder();

builder
  .addSource({
    type: "history",
    historyRef: { history: agentA.getHistory(), indices: [0, 1, 5] },
  })
  .addSource({
    type: "history",
    historyRef: { history: agentB.getHistory(), indices: [0, 2, 3] },
  })
  .addSource({
    type: "template",
    templateRef: { id: "coordinator.summary", variables: { task: "merge" } },
  });

const mergedContext = builder.build();
coordinatorAgent.setInitialMessages(mergedContext);
```

### 4.3 与现有体系的关系

```
┌─────────────────────────────────────────────────────────────┐
│                    自动化工作流（现有体系）                    │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐     │
│  │ Agent Loop  │───>│ MessageHistory│───>│ Checkpoint  │     │
│  │ (自动追加)   │    │ (历史记录)    │    │ (回退恢复)  │     │
│  └─────────────┘    └─────────────┘    └─────────────┘     │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ 特定场景需要干预时
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                 手动干预场景（新增 API）                       │
│  ┌─────────────────┐                                        │
│  │ ContextBuilder  │ <── 从 MessageHistory 读取（不修改）    │
│  │ (一次性构建)     │ <── 从文件/模板读取                     │
│  └────────┬────────┘                                        │
│           │                                                  │
│           ▼                                                  │
│  ┌─────────────────┐                                        │
│  │ 新的 LLMMessage[]│ ──> 用于新 Batch / 新 Agent           │
│  └─────────────────┘                                        │
└─────────────────────────────────────────────────────────────┘
```

**关键点**：
- `ContextBuilder` **只读取** `MessageHistory`，不修改它
- 构建结果是**新的消息数组**，可注入到新 Batch 或新 Agent
- 现有的检查点、回退机制**不受影响**

### 4.4 可选增强：消息节点标识

如果需要支持更精细的消息引用（如"选择第 3、5、7 条消息"），可以为 `LLMMessage` 添加可选 ID：

```typescript
interface LLMMessage {
  // 现有字段...
  id?: string;  // 可选的唯一标识，用于 ContextBuilder 引用
}
```

**注意**：这是可选增强，不影响现有逻辑。`MessageHistory` 内部仍使用索引管理。

### 4.5 压缩服务设计

压缩是 `ContextBuilder` 的一个重要能力：

```typescript
interface CompressionOptions {
  strategy: "keep-ends" | "keep-key" | "summarize-all";
  preserveRoles?: MessageRole[];
  summarizeMiddle?: boolean;
  maxTokens?: number;
}

interface CompressionResult {
  messages: LLMMessage[];
  compressionRatio: number;
  originalTokens: number;
  compressedTokens: number;
}

class ContextCompressor {
  compress(messages: LLMMessage[], options: CompressionOptions): Promise<CompressionResult>;
  
  summarize(messages: LLMMessage[]): Promise<string>;
}
```

**实现策略**：
- `keep-ends`：保留首尾，压缩中间
- `keep-key`：保留关键消息（如 system、user 问题）
- `summarize-all`：全部压缩为摘要

### 4.6 模板片段化组装增强

当前 `packages/prompt-templates` 已有 `FragmentRegistry`，可以增强以支持 `ContextBuilder`：

```typescript
interface FragmentCompositionConfig {
  fragmentIds: string[];
  separator?: string;
  prefix?: string;
  suffix?: string;
  conditionalFragments?: {
    fragmentId: string;
    condition: (context: EvaluationContext) => boolean;
  }[];
}

class ContextBuilder {
  addFragmentComposition(config: FragmentCompositionConfig): this;
}
```

## 五、实施建议

### 5.1 第一阶段：基础 ContextBuilder API

**目标**：提供最小可用的上下文构建能力

**实现内容**：
1. `ContextBuilderService` 和 `ContextBuilder` 类
2. 支持从 `MessageHistory` 提取消息
3. 支持添加自定义消息和模板
4. 基础 Token 计数

**文件位置**：
- `sdk/core/services/context-builder-service.ts`
- `sdk/core/services/context-compressor.ts`

### 5.2 第二阶段：压缩能力

**目标**：支持上下文压缩与剪枝

**实现内容**：
1. `ContextCompressor` 类
2. 多种压缩策略
3. LLM 辅助摘要生成

### 5.3 第三阶段：多源合并

**目标**：支持多智能体协调场景

**实现内容**：
1. 多 `MessageHistory` 合并
2. 去重和排序逻辑
3. 冲突处理

## 六、架构对比总结

| 维度 | 现有体系 | 新增 API | 关系 |
|------|----------|----------|------|
| **核心职责** | 历史记录、检查点回退 | 上下文重组、压缩 | 互补 |
| **消息结构** | 线性数组 + 索引 | 引用现有消息 | 只读引用 |
| **生命周期** | 贯穿执行过程 | 一次性构建 | 独立 |
| **使用方式** | 自动追加 | 手动调用 | 按需 |

## 七、总结

Prompt Tree 的设计思想为当前项目提供了重要借鉴，但**集成策略**需要谨慎：

### 核心结论

1. **不应取代现有体系**：`MessageHistory` 的历史记录和检查点回退功能已足够支撑自动化工作流

2. **作为补充 API**：`ContextBuilder` 作为独立服务，用于特定干预场景

3. **适用场景明确**：
   - 上下文压缩与剪枝
   - 多智能体协调合并上下文
   - 批处理任务衔接

### 设计原则

- **独立性**：不侵入现有 `MessageHistory`
- **只读引用**：从历史读取，构建新数组
- **一次性使用**：构建完即弃，不持久化
- **可审计**：记录构建过程便于调试

### 实施路径

1. 第一阶段：基础 `ContextBuilder` API
2. 第二阶段：压缩能力
3. 第三阶段：多源合并

## 八、参考资源

- [Prompt Tree 设计思想分析](../../ref/prompt-tree.md)
- [prompt-templates 设计文档](../packages/prompt-templates/design.md)
- [消息历史批量内存优化](../storage/message-history-batch-memory-optimization.md)
