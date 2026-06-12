# SDK Timeout 控制机制统一重构方案

## 1. 背景与目标

### 1.1 现状分析

当前 SDK 中存在多种 timeout 控制实现，分布在不同的模块中：

- **TimeoutManager** (`sdk/core/state-managers/timeout-manager.ts`): 功能最完善的超时管理器，支持生命周期管理、状态快照、警告阈值、中断绑定和统计信息收集
- **TimeoutRegistry** (`sdk/core/registry/timeout-registry.ts`): 全局注册表，按执行 ID 管理多个 TimeoutManager，提供跨执行的批量操作和全局统计
- **TimeoutController** (`sdk/services/executors/tools/core/base/TimeoutController.ts`): 简单的 Promise.race 实现，主要用于工具执行超时控制（已重构为复用核心工具函数）
- **PauseTimeoutManager** (`sdk/workflow/execution/utils/pause-timeout-manager.ts`): 监控工作流暂停状态的超时管理
- **Utility Functions** (`sdk/core/utils/timeout/*.ts`): 基础工具函数，如 withTimeout、createTimeoutPromise 等

**已移除的组件**:

- ~~**InterruptionTimeoutManager**~~: 已在阶段二移除，功能由 TimeoutManager 完全覆盖

### 1.2 存在的问题

1. **功能重叠**: TimeoutController 的核心逻辑与 TimeoutManager 存在重复（已在阶段二解决）
2. **碎片化管理**: 不同模块使用不同的超时管理机制，导致资源清理不统一，可能存在内存泄漏风险
3. **可观测性不一致**: 各实现提供的统计信息和监控能力参差不齐
4. **维护成本高**: 多处实现相似的超时逻辑，增加了代码维护和测试的复杂度

### 1.3 重构目标

1. **统一架构**: 建立清晰的三层架构（基础层、状态管理层、全局协调层）
2. **消除冗余**: 整合功能重叠的实现，减少代码重复
3. **增强可观测性**: 通过统一的注册表和标签系统，提供完整的超时监控能力
4. **简化使用**: 提供一致的 API，降低开发者的使用门槛
5. **保证兼容性**: 在重构过程中保持对外 API 的稳定性

## 2. 重构方案设计

### 2.1 三层架构设计

#### 第一层：基础工具层 (Foundation Layer)

**位置**: `sdk/core/utils/timeout/`

**职责**:

- 提供无状态的超时辅助函数
- 处理底层的 Promise 包装和 AbortSignal 组合
- 不涉及状态管理和资源追踪

**包含组件**:

- `withTimeout`: 执行函数并应用超时
- `createTimeoutPromise`: 为现有 Promise 添加超时保护
- `combineTimeoutWithSignal`: 组合超时与中止信号
- `calculateAdaptiveTimeout`: 计算自适应超时时间（用于重试场景）
- `delay`: 带中止支持的延迟函数
- `isTimeoutError`: 判断错误是否为超时错误

**复用策略**:

- 这些工具函数保持独立，供任何需要简单超时控制的场景使用
- 不依赖 TimeoutManager 或 TimeoutRegistry
- 适用于一次性、无状态的超时需求

#### 第二层：状态管理层 (State Management Layer)

**位置**: `sdk/core/state-managers/timeout-manager.ts`

**职责**:

- 管理单个执行上下文中的超时生命周期
- 提供注册、取消、刷新、查询剩余时间等操作
- 支持警告阈值和回调
- 与中断状态系统集成，实现自动取消
- 提供状态快照功能，支持断点续传
- 收集详细的统计信息

**核心特性**:

- **唯一入口**: 所有需要状态管理的超时都应通过 TimeoutManager 注册
- **标签系统**: 支持为超时打上标签，便于批量操作
- **中断绑定**: 原生支持绑定到 InterruptionState，在中断时自动清理
- **Checkpoint 支持**: 可序列化状态，支持工作流恢复场景
- **统计收集**: 记录注册数、超时数、取消数、平均时长等指标

**重构重点**:

- 成为 SDK 内部超时管理的核心组件
- 吸收 InterruptionTimeoutManager 的功能
- 为 PauseTimeoutManager 提供底层支持

#### 第三层：全局协调层 (Registry Layer)

**位置**: `sdk/core/registry/timeout-registry.ts`

**职责**:

- 按执行 ID 管理多个 TimeoutManager 实例
- 提供跨执行的批量操作（如按标签取消所有相关超时）
- 聚合全局统计信息
- 确保执行结束时的资源清理
- 防止资源泄漏

**核心特性**:

- **自动清理**: 执行结束时自动调用 cleanup，取消所有活跃超时
- **标签索引**: 维护跨执行的标签索引，支持高效的批量取消
- **配置继承**: 为所有子 Manager 提供默认配置
- **全局视图**: 提供整个 SDK 的超时使用情况概览

**使用规范**:

- 所有 TimeoutManager 实例应通过 TimeoutRegistry 获取，而非直接创建
- 业务代码不应直接持有 TimeoutManager 引用，而应通过 Registry 间接访问

### 2.2 组件整合方案

#### 方案一：TimeoutController 重构

**现状**:

- 位于 `sdk/services/executors/tools/core/base/TimeoutController.ts`
- 在 BaseExecutor 中用于控制工具执行超时
- 实现简单，仅支持基本的超时和 AbortSignal

**重构策略**:

**选项 A（推荐）**: 保持独立但内部复用

- TimeoutController 保留在 services 层，作为工具执行专用的轻量级封装
- 内部实现改为调用 `core/utils/timeout` 中的 `withTimeout` 函数
- 优点：services 层不依赖 core 的状态管理层，保持层次清晰
- 缺点：仍存在一定程度的代码重复

**选项 B**: 完全统一到 TimeoutManager

- 移除 TimeoutController，BaseExecutor 直接使用 TimeoutManager
- 需要在 BaseExecutor 中注入 TimeoutRegistry 或 TimeoutManager
- 优点：彻底消除重复，统一管理
- 缺点：增加了 services 对 core 的依赖耦合

**建议**: 采用选项 A，因为工具执行超时通常是短暂的、无状态的，不需要复杂的生命周期管理。TimeoutController 作为轻量级封装更合适，但应复用底层工具函数。

#### 方案二：InterruptionTimeoutManager 整合

**现状**:

- 位于 `sdk/core/utils/interruption/interruption-timeout-manager.ts`
- 专门用于中断相关操作（Hook 执行、LLM 调用等）的超时保护
- 支持按操作类型配置不同的超时时间

**问题分析**:

- TimeoutManager 已经原生支持绑定到 InterruptionState
- InterruptionTimeoutManager 的功能完全可以被 TimeoutManager 覆盖
- 独立存在导致两套相似的管理逻辑

**重构策略**:

**步骤 1**: 扩展 TimeoutManager 的中断集成能力

- 确保 TimeoutManager 能够方便地绑定到当前的 InterruptionState
- 提供便捷的 API，如 `registerWithInterruptionProtection()`

**步骤 2**: 迁移使用场景

- 识别所有使用 InterruptionTimeoutManager 的地方
- 改为使用 TimeoutManager，并通过标签（如 'interruption-hook'、'interruption-llm'）区分不同类型的中断超时
- 利用 TimeoutManager 的 interruptionState 绑定功能实现自动取消

**步骤 3**: 废弃 InterruptionTimeoutManager

- 标记为 deprecated
- 提供迁移指南
- 在下一个大版本中移除

**优势**:

- 统一的中断超时管理
- 可以利用 TimeoutRegistry 的全局统计和批量操作
- 减少代码维护负担

#### 方案三：PauseTimeoutManager 适配

**现状**:

- 位于 `sdk/workflow/execution/utils/pause-timeout-manager.ts`
- 监控工作流暂停状态，在超过最大暂停时间时触发警告或取消
- 使用独立的 setTimeout 实现

**重构策略**:

**选项 A**: 保持独立（推荐）

- PauseTimeoutManager 的业务逻辑较为特殊（监控暂停状态、发射警告事件）
- 可以继续使用独立的实现，但在内部使用 `core/utils/timeout` 的工具函数
- 当需要与工作流的全局状态同步时，可以通过 TimeoutRegistry 注册一个特殊的 TimeoutManager

**选项 B**: 完全整合

- 将暂停监控逻辑抽象为 TimeoutManager 的一个特殊用例
- 通过定时刷新（refresh）来实现"只要未恢复就持续计时"的逻辑
- 优点：完全统一
- 缺点：可能使 TimeoutManager 的职责过于复杂

**建议**: 采用选项 A，因为暂停超时监控是一个特定的业务场景，与通用的超时管理有所区别。但应确保它遵循统一的超时配置规范和错误处理标准。

### 2.3 标签系统设计

为了支持高效的批量操作和细粒度的监控，建议在 TimeoutRegistry 中引入标准化的标签系统。

#### 标签命名规范

定义以下标准标签前缀：

- **llm-\***: LLM 相关超时
  - `llm-call`: 单次 LLM 调用
  - `llm-stream`: LLM 流式响应
  - `llm-retry`: LLM 重试

- **tool-\***: 工具执行超时
  - `tool-execution`: 通用工具执行
  - `tool-shell`: Shell 命令执行
  - `tool-api`: API 调用工具

- **workflow-\***: 工作流相关超时
  - `workflow-execution`: 工作流整体执行
  - `workflow-pause`: 暂停状态监控
  - `workflow-node`: 节点执行

- **interruption-\***: 中断相关超时
  - `interruption-hook`: 中断钩子执行
  - `interruption-cleanup`: 中断清理操作

- **user-\***: 用户交互超时
  - `user-input`: 等待用户输入
  - `user-approval`: 等待用户批准

#### 标签使用示例

```typescript
// 注册一个 LLM 调用超时
registry.register(executionId, {
  id: "llm-call-001",
  duration: 120000,
  tag: "llm-call",
  onTimeout: async () => {
    /* 处理超时 */
  },
  metadata: { model: "gpt-4", node: "agent-node" },
});

// 批量取消所有 LLM 相关超时
registry.cancelByTag("llm-call");

// 查询特定标签的统计信息
const stats = registry.getStats();
console.log(stats.byTag["llm-call"]); // LLM 调用超时数量
```

### 2.4 配置管理规范

#### 分层配置

**全局配置** (TimeoutRegistryConfig):

- 定义所有 TimeoutManager 的默认行为
- 设置最大超时数限制
- 配置自动清理策略
- 设置指标收集间隔

**执行级配置** (TimeoutManagerConfig):

- 针对特定执行上下文定制超时行为
- 可覆盖全局默认值
- 例如：某些关键执行可能需要更长的超时时间或禁用警告

**超时级配置** (TimeoutRegistration):

- 每个具体超时的个性化设置
- 包括持续时间、警告阈值、标签、元数据等

#### 配置优先级

超时级配置 > 执行级配置 > 全局配置

#### 配置验证

在注册时进行严格的配置验证：

- 超时 duration 必须为正数
- 不能超过全局最大超时限制
- 标签必须符合命名规范
- 执行 ID 不能为空

### 2.5 错误处理标准化

#### 统一错误类型

所有超时触发的错误应使用统一的 `TimeoutError` 类型（已在 `@wf-agent/types` 中定义）。

**错误信息规范**:

- 包含超时 ID
- 包含实际经过的时间
- 包含配置的超时时长
- 可选：包含标签和元数据

**示例**:

```
TimeoutError: Timeout 'llm-call-001' expired after 120000ms (tag: llm-call)
```

#### 错误传播

- TimeoutManager 在执行 onTimeout 回调时捕获异常，避免回调错误影响其他超时
- 记录详细的错误日志，包括堆栈信息
- 向 EventRegistry 发射超时事件，供监控系统消费

## 3. 实施计划

### 阶段一：基础设施强化（1-2 周）

**目标**: 完善核心组件，为后续整合打下基础

**任务**:

1. 审查并优化 `TimeoutManager` 的实现
   - 确保中断绑定功能稳定可靠
   - 完善统计信息收集
   - 增强错误处理和日志记录

2. 强化 `TimeoutRegistry` 的标签索引功能
   - 优化跨执行标签查询性能
   - 添加标签验证逻辑
   - 完善全局统计聚合

3. 丰富 `core/utils/timeout` 工具函数
   - 补充缺失的工具函数
   - 编写全面的单元测试
   - 完善文档和示例

4. 定义标准标签规范
   - 制定标签命名约定
   - 创建标签枚举或常量文件
   - 更新文档

**交付物**:

- 强化后的 TimeoutManager 和 TimeoutRegistry
- 完整的工具函数库
- 标签规范文档

### 阶段二：组件整合（2-3 周）

**目标**: 逐步整合分散的超时管理实现

**任务**:

**第 1 步**: TimeoutController 重构

- 修改 TimeoutController 内部实现，复用 `withTimeout` 工具函数
- 保持对外 API 不变，确保向后兼容
- 更新单元测试

**第 2 步**: ~~InterruptionTimeoutManager 迁移~~ (已完成)

- ✅ 已完全移除 InterruptionTimeoutManager
- ✅ 从 index.ts 中移除了导出
- ✅ 提供了详细的迁移指南

**第 3 步**: PauseTimeoutManager 适配

- 评估是否需要整合或保持独立
- 如果保持独立，确保其使用统一的工具函数和错误类型
- 如果需要整合，设计专门的 API 扩展 TimeoutManager

**第 4 步**: 其他散落超时的清理

- 搜索代码库中直接使用 setTimeout 的地方
- 评估是否应纳入统一的超时管理体系
- 逐步重构，优先处理关键路径

**交付物**:

- 整合后的代码库
- 迁移指南
- 更新的测试套件

### 阶段三：可观测性增强（1-2 周）

**目标**: 提升超时管理的监控和诊断能力

**任务**:

1. 集成 Metrics Registry
   - 将 TimeoutManager 的统计数据上报到 Metrics Registry
   - 定义标准的超时相关指标（如超时率、平均时长、按标签分布等）
   - 支持 Prometheus 或其他监控系统

2. 增强事件系统
   - 确保所有超时事件（注册、过期、取消、警告）都正确发射
   - 提供事件订阅接口，供调试和监控使用
   - 记录详细的事件日志

3. 开发诊断工具
   - 提供 API 查询当前活跃的超时列表
   - 支持按执行 ID、标签、状态过滤
   - 提供超时历史查询功能

4. 完善文档
   - 更新架构文档
   - 编写使用指南和最佳实践
   - 添加故障排查手册

**交付物**:

- 完整的监控指标体系
- 诊断工具和 API
- 完善的文档

### 阶段四：测试与验证（1-2 周）

**目标**: 确保重构后的系统稳定可靠

**任务**:

1. 单元测试全覆盖
   - 为所有修改的组件编写或更新单元测试
   - 确保测试覆盖率不低于 90%
   - 特别关注边界条件和异常场景

2. 集成测试
   - 编写端到端测试，验证完整的工作流
   - 测试并发执行场景下的超时管理
   - 验证资源清理的正确性（无内存泄漏）

3. 性能测试
   - benchmark 大量超时注册和取消的性能
   - 测试标签索引的查询效率
   - 验证在高负载下的稳定性

4. 回归测试
   - 运行现有的测试套件，确保没有破坏现有功能
   - 在实际应用场景中验证重构效果

**交付物**:

- 完整的测试报告
- 性能基准数据
- 回归测试结果

## 7. 附录

### 7.1 相关文件清单

**核心组件**:

- `sdk/core/state-managers/timeout-manager.ts`
- `sdk/core/registry/timeout-registry.ts`
- `sdk/core/utils/timeout/*.ts`
- `sdk/core/types/timeout.ts`
- `sdk/core/types/timeout-config.ts`

**待整合组件**:

- `sdk/services/executors/tools/core/base/TimeoutController.ts` (已重构)
- `sdk/workflow/execution/utils/pause-timeout-manager.ts` (保持独立)

**已移除组件**:

- ~~`sdk/core/utils/interruption/interruption-timeout-manager.ts`~~ (已在阶段二移除)

**测试文件**:

- `sdk/core/state-managers/__tests__/timeout-manager.test.ts`
- `sdk/core/registry/__tests__/timeout-registry.test.ts`
- `sdk/services/executors/tools/core/base/__tests__/TimeoutController.test.ts`

**文档**:

- `sdk/docs/architecture/timeout-management.md`
- `sdk/core/utils/timeout/README.md`

### 7.2 术语表

- **TimeoutManager**: 管理单个执行上下文中超时生命周期的组件
- **TimeoutRegistry**: 全局注册表，管理多个 TimeoutManager 实例
- **TimeoutHandle**: 注册超时后返回的句柄，用于后续操作
- **InterruptionState**: 中断状态对象，表示执行是否应停止
- **Checkpoint**: 状态快照，用于断点续传
- **Tag**: 标签，用于分类和批量操作超时

### 7.3 参考资料

- [Timeout Management Architecture](sdk/docs/architecture/timeout-management.md)
- [Interruption Future Improvements](sdk/docs/architecture/interruption-future-improvements.md)
- [Terminal Service Design](sdk/docs/services/terminal-service-design.md)

---

**文档版本**: 1.0  
**最后更新**: 2026-05-19  
**作者**: AI Assistant  
**审核状态**: 待审核
