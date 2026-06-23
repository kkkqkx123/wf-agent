# 工具可见性机制分析与简化方案

## 概述

本文档分析了SDK中workflow的工具可见性传递机制,评估了现有实现的合理性,并提出了基于Function Calling KV缓存限制的根本简化方案。

**核心洞察**: 由于LLM Function Calling的特性,任何工具schema变更都会导致KV缓存完全失效。因此,**动态添加工具到schema是不可行的**,应该采用"固定schema + 运行时拦截"的架构。

---

## 1. 历史架构分析(已废弃)

### 1.1 原有设计思路

原有的工具可见性管理采用了复杂的**双层存储 + 协调器模式**:

```
ToolContextStore (管理工具上下文)
    ↓
ToolVisibilityStore (管理可见性状态)
    ↓
ToolVisibilityCoordinator (协调可见性变化)
    ↓
ToolVisibilityMessageBuilder (生成声明消息)
    ↓
Prompt注入 (告诉LLM哪些工具可用)
```

### 1.2 核心组件

#### ToolContextStore
- **职责**: 管理执行隔离的工具上下文
- **作用域**: 支持 `EXECUTION` (默认) 和 `LOCAL` (子图/隔离上下文)
- **问题**: 维护复杂的状态映射,与作用域切换耦合

#### ToolVisibilityStore
- **职责**: 管理工具的可见性状态
- **数据结构**: `Map<executionId, ToolVisibilityContext>`
- **问题**: 与ToolContextStore存在数据冗余

#### ToolVisibilityCoordinator
- **职责**: 无状态协调器,负责协调可见性变化并生成声明消息
- **功能**: 
  - 初始化可见性上下文
  - 作用域切换时更新可见性
  - 动态添加工具时发送增量通知
- **问题**: 需要生成和注入系统消息,增加prompt复杂度

### 1.3 主要问题

1. **KV缓存失效**: 每次工具集变更都需要重新计算,破坏LLM性能优化
2. **过度复杂**: 3个存储层 + 协调器 + 消息构建器,代码量~800行
3. **作用域管理分散**: 工具集计算逻辑分散在各处
4. **消息索引脆弱**: 依赖消息索引可能导致声明历史与实际消息不一致
5. **动态添加不可行**: 违背了Function Calling的设计原则

---

## 2. 新架构设计(当前实现)

### 2.1 核心设计理念

**从"动态可见性管理"转向"静态配置 + 简单拦截"**

```
AvailableTools Configuration (Schema固定)
    ↓
ToolPermissionManager (运行时权限状态)
    ↓
TOOL_VISIBILITY Node (动态控制)
    ↓
Execution Interception (安全拦截)
```

### 2.2 核心组件

#### AvailableTools Configuration

**位置**: `packages/types/src/workflow/tool-config.ts`

```typescript
interface AvailableTools {
  /**
   * 完整工具池 (schema级别,固定不变)
   * LLM看到的工具列表基于此集合
   */
  available: string[];
  
  /**
   * 初始启用的工具 (subset of available)
   * 如果为空,则所有available工具都启用
   */
  initial?: string[];
  
  /**
   * 需要审批的工具 (subset of available)
   * 这些工具可以调用但需要显式批准
   */
  requireApproval?: string[];
}
```

**关键约束**:
- `initial ⊆ available`
- `requireApproval ⊆ available`
- Tools NOT in `available` are completely excluded

**优势**:
- ✅ Schema固定,避免KV缓存失效
- ✅ 配置清晰,易于理解
- ✅ 支持渐进式工具启用

#### ToolPermissionManager

**位置**: `sdk/core/coordinators/tool-permission-manager.ts`

**职责**: 管理运行时工具权限(enabled/disabled状态)

**核心数据结构**:
```typescript
interface ToolPermissionState {
  enabledTools: Set<string>;   // 当前可用的工具
  disabledTools: Set<string>;  // 当前禁用的工具
  history: PermissionChange[]; // 变更历史(用于checkpoint)
}
```

**关键方法**:
- `isEnabled(toolId)`: 检查工具是否可用
- `isDisabled(toolId)`: 检查工具是否被禁用
- `enableTools(toolIds, reason?, nodeId?)`: 启用工具
- `disableTools(toolIds, reason?, nodeId?)`: 禁用工具
- `getBlockReason(toolId)`: 获取工具被阻止的原因
- `serialize() / deserialize()`: 支持checkpoint序列化

**设计特点**:
- ✅ 无状态协调器,所有状态在state对象中
- ✅ 支持完整的变更历史记录
- ✅ 内置checkpoint支持
- ✅ 清晰的日志记录

#### RejectionMessageBuilder

**位置**: `sdk/core/coordinators/rejection-message-builder.ts`

**职责**: 构建工具被阻止时的提示信息

**配置结构**:
```typescript
interface ToolRejectionConfig {
  globalDefaultTemplate?: string;      // 全局默认模板
  toolSpecificTemplates?: Record<string, string>;  // 工具特定模板
  injectUserMessageHint?: boolean;     // 是否在用户消息中注入提示
  userMessageHintTemplate?: string;    // 用户消息提示模板
}
```

**默认模板**:
```typescript
// 拒绝消息
"Tool '{{toolId}}' is currently unavailable. {{reason}}"

// 用户提示
"[Note: {{disabledTools}} are now disabled. {{enabledTools}} are now available.]"
```

**优势**:
- ✅ 支持自定义模板
- ✅ 可针对特定工具定制消息
- ✅ 可选的用户消息提示

#### TOOL_VISIBILITY Node

**位置**: `sdk/workflow/execution/handlers/node-handlers/tool-visibility-handler.ts`

**节点配置**:
```typescript
interface ToolVisibilityNodeConfig {
  action: 'block' | 'unblock';  // 操作类型
  toolIds: string[];             // 目标工具ID列表
  reason?: string;               // 可选的原因说明
}
```

**执行流程**:
1. 根据action调用 `permissionManager.disableTools()` 或 `enableTools()`
2. 使用 `rejectionBuilder.buildUserMessageHint()` 生成提示
3. 返回执行结果(包含enabled/disabled工具列表)

**示例**:
```toml
[[nodes]]
id = "block_dangerous_tools"
type = "TOOL_VISIBILITY"
config.action = "block"
config.toolIds = ["delete_file", "execute_shell"]
config.reason = "Temporarily disabled for safety"
```

### 2.3 工作流程

```
┌─────────────────────────────────────────────────┐
│ 1. Workflow启动                                  │
│    - 读取AvailableTools配置                      │
│    - 创建ToolPermissionManager                   │
│    - 初始化: enabled = initial or all available  │
└─────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────┐
│ 2. LLM执行前                                     │
│    - 从permissionManager获取enabled tools        │
│    - 只将enabled tools传递给LLM (schema过滤)     │
└─────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────┐
│ 3. TOOL_VISIBILITY节点执行                       │
│    - block: permissionManager.disableTools()     │
│    - unblock: permissionManager.enableTools()    │
│    - 生成用户提示消息                             │
└─────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────┐
│ 4. 工具调用执行                                  │
│    - LLM层: 只能看到enabled的工具                │
│    - Executor层: 拦截disabled的工具调用          │
│    - 返回友好的拒绝消息                           │
└─────────────────────────────────────────────────┘
```

### 2.4 架构优势

| 维度 | 旧架构 | 新架构 |
|------|--------|--------|
| **Schema稳定性** | ❌ 动态变更,缓存失效 | ✅ 固定schema,缓存友好 |
| **代码复杂度** | ❌ ~800行,3个存储层 | ✅ ~400行,2个协调器 |
| **状态管理** | ❌ 分散在多个store | ✅ 集中在PermissionManager |
| **Prompt注入** | ❌ 需要生成声明消息 | ✅ 无需注入(schema已包含) |
| **作用域切换** | ❌ 需要重新计算和声明 | ✅ 只需更新enabled/disabled |
| **可预测性** | ❌ 动态变化,难以调试 | ✅ 配置驱动,易于理解 |
| **KV缓存影响** | ❌ 每次变更都失效 | ✅ 无影响 |

---

## 3. 当前实施状态

### 3.1 已完成 ✅

1. **ToolPermissionManager** - 完整实现,支持enable/disable/history/checkpoint
2. **RejectionMessageBuilder** - 完整实现,支持模板配置
3. **TOOL_VISIBILITY Handler** - 完整实现,集成permissionManager和rejectionBuilder
4. **AvailableTools Type Definition** - 完整定义,包含验证和辅助函数
5. **NodeHandlerContextFactory** - 支持注入permissionManager和rejectionBuilder

### 3.2 待完成 ⏳

#### Phase 1: DI容器集成

**问题**: ToolPermissionManager和RejectionMessageBuilder尚未在DI容器中注册

**需要修改**: `sdk/core/di/container-config.ts`

```typescript
// 需要添加:
container.bind(Identifiers.ToolPermissionManager).toDynamicValue((c) => {
  // 从WorkflowTemplate获取AvailableTools配置
  // 创建ToolPermissionManager实例
}).inTransientScope(); // 每个execution一个实例

container.bind(Identifiers.RejectionMessageBuilder).to(RejectionMessageBuilder).inSingletonScope();
```

#### Phase 2: NodeExecutionCoordinator配置注入

**问题**: NodeExecutionCoordinator的配置中缺少permissionManager和rejectionBuilder

**当前位置**: `sdk/core/di/container-config.ts` 第580-622行

**需要添加**:
```typescript
const config = {
  // ... existing config
  permissionManager: c.get(Identifiers.ToolPermissionManager),
  rejectionBuilder: c.get(Identifiers.RejectionMessageBuilder),
};
```

#### Phase 3: WorkflowExecutionBuilder初始化

**问题**: 需要在WorkflowExecutionBuilder中根据AvailableTools配置初始化ToolPermissionManager

**需要修改**: `sdk/workflow/execution/factories/workflow-execution-builder.ts`

```typescript
// 在buildFromWorkflowGraph方法中:
const workflowTemplate = this.workflowRegistry.get(workflowId);
const availableTools = workflowTemplate?.config?.availableTools;

if (availableTools) {
  const schemaTools = resolveSchemaTools(availableTools);
  const initialTools = resolveInitialTools(availableTools);
  
  const permissionManager = new ToolPermissionManager(initialTools, schemaTools);
  // 将permissionManager传递给NodeHandlerContextFactory
}
```

#### Phase 4: LLMExecutionCoordinator集成

**问题**: LLMExecutionCoordinator仍在使用旧的ToolContextStore

**当前位置**: `sdk/workflow/execution/coordinators/llm-execution-coordinator.ts` 第243-263行

**需要修改**:
```typescript
// 当前代码:
const toolContextStore = this.contextFactory.getToolContextStore();
if (toolContextStore) {
  const availableToolIds = toolContextStore.getTools(executionId);
  // ... 复杂的过滤逻辑
}

// 应改为:
const permissionManager = this.contextFactory.getPermissionManager();
if (permissionManager) {
  const enabledToolIds = permissionManager.getEnabledTools();
  const resolvedTools = enabledToolIds
    .map(id => toolService.getTool(id))
    .filter(Boolean);
  availableTools = resolvedTools;
}
```

#### Phase 5: 删除旧组件

**需要删除的文件**:
1. `sdk/workflow/stores/tool-context-store.ts`
2. `sdk/workflow/stores/tool-visibility-store.ts`
3. `sdk/workflow/execution/coordinators/tool-visibility-coordinator.ts`
4. `sdk/workflow/execution/utils/tool-visibility-message-builder.ts`
5. `sdk/workflow/execution/handlers/node-handlers/add-tool-handler.ts` (遗留代码)

**需要清理的代码**:
1. `ToolCallExecutor` 中的 `toolVisibilityStore` 检查 (第415-476行)
2. DI容器中的相关绑定
3. 相关的类型定义和导入

#### Phase 6: 测试和文档

1. 添加集成测试验证新架构
2. 更新API文档
3. 提供迁移指南

---

## 4. 技术决策理由

### 4.1 为什么不支持动态Schema?

**Function Calling的KV缓存机制**:
- LLM为每个工具schema生成KV缓存
- Schema变更 → 缓存失效 → 重新计算 → 性能下降
- 频繁变更 → 缓存抖动 → 严重影响响应时间

**实际影响**:
```
场景1: 固定Schema (推荐)
- 首次请求: 建立缓存 (~100ms额外开销)
- 后续请求: 复用缓存 (0额外开销)
- 总开销: ~100ms

场景2: 动态Schema (不推荐)
- 每次请求: 缓存失效 + 重新计算 (~100ms)
- N次请求: N × 100ms
- 总开销: N × 100ms (线性增长)
```

### 4.2 为什么选择拦截而非Schema过滤?

**方案对比**:

| 方案 | 优点 | 缺点 |
|------|------|------|
| **Schema过滤** | LLM看不到禁用工具 | 需要频繁修改schema,缓存失效 |
| **运行时拦截** | Schema稳定,缓存友好 | LLM可能尝试调用禁用工具 |

**选择拦截的理由**:
1. ✅ **性能优先**: 避免缓存失效带来的性能损失
2. ✅ **用户体验**: LLM可以尝试调用,得到友好的拒绝消息
3. ✅ **可观测性**: 可以看到LLM想要调用哪些工具(即使被阻止)
4. ✅ **灵活性**: 可以快速切换工具可用性,无需等待LLM重新理解schema

**缓解LLM尝试调用禁用工具的问题**:
- 通过 `RejectionMessageBuilder` 提供清晰的反馈
- 在用户消息中注入提示: "[Note: X tools are now disabled]"
- LLM会根据反馈调整行为

### 4.3 为什么保留ToolFailureProtectionState?

**ToolFailureProtectionState** (工具失败保护) 与 **ToolPermissionManager** (工具可见性管理) 是两个不同的概念:

| 特性 | ToolPermissionManager | ToolFailureProtectionState |
|------|----------------------|---------------------------|
| **触发条件** | 显式的TOOL_VISIBILITY节点 | 连续的工具执行失败 |
| **控制粒度** | 工具级别 | 工具级别 |
| **持续时间** | 直到显式unblock | 自动冷却期后恢复 |
| **用途** | 工作流控制 | 错误恢复和稳定性 |

**结论**: 两者互补,都应该保留。

---

## 5. 实施路线图

### Phase 1: 核心集成 (1-2天)

**目标**: 使新架构能够运行

**任务**:
1. 在DI容器中注册ToolPermissionManager和RejectionMessageBuilder
2. 在NodeExecutionCoordinator中注入这两个组件
3. 在WorkflowExecutionBuilder中根据AvailableTools初始化permissionManager

**验收标准**:
- ✅ TOOL_VISIBILITY节点可以正常执行
- ✅ permissionManager正确跟踪enabled/disabled状态
- ✅ 没有运行时错误

### Phase 2: LLM集成 (1天)

**目标**: LLM只看到enabled的工具

**任务**:
1. 修改LLMExecutionCoordinator使用permissionManager过滤工具
2. 确保只有enabled tools被传递给LLM executor

**验收标准**:
- ✅ LLM只能调用enabled的工具
- ✅ disabled工具不会出现在LLM的tools列表中

### Phase 3: 清理旧代码 (1-2天)

**目标**: 删除所有旧的可见性管理代码

**任务**:
1. 删除ToolContextStore、ToolVisibilityStore、ToolVisibilityCoordinator
2. 移除ToolCallExecutor中的旧检查逻辑
3. 清理DI容器中的相关绑定
4. 删除add-tool-handler.ts (遗留代码)

**验收标准**:
- ✅ 编译无错误
- ✅ 所有测试通过
- ✅ 没有对旧组件的引用

### Phase 4: 测试和文档 (1-2天)

**目标**: 确保新架构的稳定性和可用性

**任务**:
1. 添加集成测试验证完整工作流程
2. 编写迁移指南
3. 更新API文档
4. 提供示例workflow

**验收标准**:
- ✅ 集成测试覆盖主要场景
- ✅ 文档清晰完整
- ✅ 示例可以正常运行

---

## 6. 迁移指南

### 6.1 从ADD_TOOL迁移到TOOL_VISIBILITY

**旧方式 (ADD_TOOL)**:
```toml
[[nodes]]
id = "add_tools"
type = "ADD_TOOL"
config.toolIds = ["read_file", "write_file"]
config.scope = "EXECUTION"
```

**新方式 (TOOL_VISIBILITY)**:
```toml
# 在workflow配置中声明所有可用工具
[config.availableTools]
available = ["read_file", "write_file", "delete_file", "execute_shell"]
initial = ["read_file"]  # 初始只启用read_file

# 使用TOOL_VISIBILITY节点动态启用其他工具
[[nodes]]
id = "enable_write"
type = "TOOL_VISIBILITY"
config.action = "unblock"
config.toolIds = ["write_file"]
config.reason = "User granted write permission"
```

### 6.2 配置AvailableTools

**基本配置** (所有工具初始可用):
```typescript
const config: AvailableTools = {
  available: ["read_file", "write_file", "edit_file"]
};
```

**高级配置** (渐进式启用 + 审批):
```typescript
const config: AvailableTools = {
  available: ["read_file", "write_file", "delete_file", "execute_shell"],
  initial: ["read_file"],  // 初始只读
  requireApproval: ["delete_file", "execute_shell"]  // 危险操作需审批
};
```

### 6.3 使用TOOL_VISIBILITY节点

**Block工具**:
```toml
[[nodes]]
id = "block_dangerous"
type = "TOOL_VISIBILITY"
config.action = "block"
config.toolIds = ["delete_file", "execute_shell"]
config.reason = "Safety mode enabled"
```

**Unblock工具**:
```toml
[[nodes]]
id = "enable_after_approval"
type = "TOOL_VISIBILITY"
config.action = "unblock"
config.toolIds = ["delete_file"]
config.reason = "User approved deletion"
```

---

## 7. 常见问题

### Q1: 为什么不直接在LLM调用时过滤工具?

**A**: 这正是我们做的!LLMExecutionCoordinator会在调用LLM之前从permissionManager获取enabled tools,只传递这些工具给LLM。这样既保持了schema稳定,又实现了运行时控制。

### Q2: 如果LLM尝试调用disabled工具会怎样?

**A**: 有两层防护:
1. **LLM层**: LLM看不到disabled工具,所以不应该尝试调用
2. **Executor层**: 即使LLM尝试调用,ToolCallExecutor会拦截并返回友好的错误消息

第二层是安全网,正常情况下不应该触发。

### Q3: TOOL_VISIBILITY节点会影响checkpoint吗?

**A**: 不会。ToolPermissionManager支持serialize/deserialize,会自动保存和恢复enabled/disabled状态。Checkpoint恢复后,工具权限状态会保持一致。

### Q4: 可以在子图中使用不同的工具集吗?

**A**: 可以。每个WorkflowExecutionEntity有独立的ToolPermissionManager实例。子图可以有自己的AvailableTools配置,或者继承父图的配置并通过TOOL_VISIBILITY节点调整。

### Q5: 如何调试工具可见性问题?

**A**: 
1. 检查ToolPermissionManager的日志,查看enabled/disabled状态变化
2. 查看TOOL_VISIBILITY节点的执行结果
3. 检查LLM收到的tools列表是否正确
4. 使用 `permissionManager.getState()` 查看当前状态

---

## 8. 总结

### 核心改进

1. **✅ 性能优化**: 固定schema,避免KV缓存失效
2. **✅ 架构简化**: 从~800行复杂代码简化为~400行清晰代码
3. **✅ 职责明确**: PermissionManager管理状态,节点负责触发变更
4. **✅ 可维护性**: 配置驱动,易于理解和调试

### 关键设计原则

1. **Schema稳定性优先**: 避免动态修改LLM工具schema
2. **运行时拦截**: 通过enabled/disabled状态控制工具可用性
3. **配置驱动**: 使用AvailableTools配置定义工具池
4. **渐进式启用**: 支持从最小权限开始,逐步授予更多工具

### 下一步行动

按照实施路线图,分4个阶段完成新架构的集成和旧代码的清理。预计总工作量5-7天。

---

## 参考文献

- [Function Calling最佳实践](https://platform.openai.com/docs/guides/function-calling)
- [KV缓存优化](https://huggingface.co/blog/kv-cache)
- [Tool Use安全性考虑](https://docs.anthropic.com/claude/docs/tool-use)

---

**文档版本**: 1.0  
**最后更新**: 2026-05-17  
**作者**: AI Agent  
**审核状态**: Draft
