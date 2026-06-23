# SDK/API 执行历史、错误、中断查询能力分析

## 1. 现有支持情况

### 1.1 执行状态与结果查询

| 能力 | Workflow API | Agent API | 状态 |
|---|---|---|---|
| 执行状态查询 | `getExecutionStatus()` | `getStatus()` | 完整 |
| 执行结果查询 | `getExecutionResult()` | 无专用 API | 部分 |
| 执行摘要列表 | `getExecutionSummaries()` | 缺失 | 缺口 |
| 执行统计 | `getExecutionStatistics()` | 缺失 | 缺口 |

**相关文件：**
- `sdk/api/workflow/resources/workflow-execution-registry-api.ts:169` — `getExecutionSummaries()`
- `sdk/api/workflow/resources/workflow-execution-registry-api.ts:211` — `getExecutionStatus()`
- `sdk/api/workflow/resources/workflow-execution-registry-api.ts:224` — `getExecutionResult()`
- `sdk/api/workflow/resources/workflow-execution-registry-api.ts:260` — `getExecutionStatistics()`

### 1.2 执行历史与时间线

| 能力 | Workflow API | Agent API | 状态 |
|---|---|---|---|
| 执行时间线 | `getExecutionTimeline()` | 缺失 | 缺口 |
| 变量历史 | `getVariableHistory()` | 缺失 | 缺口 |
| 变量快照 | `getVariableSnapshot()` / `getVariableSnapshots()` | 缺失 | 缺口 |
| 上下文演进 | `getContextEvolution()` | 缺失 | 缺口 |
| 节点转换 | `getNodeTransitions()` | 缺失 | 缺口 |
| 关键上下文快照 | `getKeyContextSnapshots()` | 缺失 | 缺口 |
| 节点执行分析 | `getExecutionNodeAnalyses()` | 不适用 | 完整 |
| 执行路径 | `getExecutionPath()` | 缺失 | 缺口 |
| 优化机会 | `getOptimizationOpportunities()` | 缺失 | 缺口 |

**相关文件：**
- `sdk/api/workflow/resources/workflow-execution-context-api.ts:342` — `getVariableSnapshot()`
- `sdk/api/workflow/resources/workflow-execution-context-api.ts:472` — `getVariableHistory()`
- `sdk/api/workflow/resources/workflow-execution-context-api.ts:531` — `getContextEvolution()`
- `sdk/api/workflow/resources/workflow-iteration-analysis-api.ts:417` — `getExecutionNodeAnalyses()`
- `sdk/api/workflow/resources/workflow-iteration-analysis-api.ts:434` — `getExecutionPath()`

### 1.3 错误处理与查询

| 能力 | Workflow API | Agent API | 状态 |
|---|---|---|---|
| 错误记录查询 | `getErrorRecords()` | `getErrorRecords()` | 完整 |
| 错误链分析 | `getErrorChain()` | `getErrorChain()` | 完整 |
| 根因分析 | `analyzeRootCause()` | `analyzeErrorPattern()` | 完整 |
| 错误统计 | `getErrorStatistics()` | 缺失 | 缺口 |
| 恢复建议 | `getRecoveryProposal()` | `getRecommendedRecoveryAction()` | 完整 |
| 相似错误查询 | `getSimilarErrors()` | 缺失 | 缺口 |

**相关文件：**
- `sdk/api/workflow/resources/errors/workflow-error-analysis-api.ts:140` — `getErrorChain()`
- `sdk/api/workflow/resources/errors/workflow-error-analysis-api.ts:170` — `analyzeRootCause()`
- `sdk/api/workflow/resources/errors/workflow-error-analysis-api.ts:219` — `getErrorStatistics()`
- `sdk/api/workflow/resources/errors/workflow-error-analysis-api.ts:295` — `getRecoveryProposal()`
- `sdk/agent/state-managers/agent-loop-state.ts:532` — `recordError()`
- `sdk/agent/state-managers/agent-loop-state.ts:632` — `analyzeErrorPattern()`

### 1.4 中断查询

| 能力 | Workflow | Agent | 状态 |
|---|---|---|---|
| 中断检测 | `InterruptionDetector` | `checkAgentInterruption()` | 完整 |
| 中断类型查询 | `getInterruptionType()` | `getAgentInterruptionType()` | 完整 |
| 中断历史查询 | 仅状态记录，无 API | 仅状态记录，无 API | 缺口 |
| 中断统计 | 缺失 | 缺失 | 缺口 |

**相关文件：**
- `sdk/workflow/execution/interruption-detector.ts:31` — `InterruptionDetector` 接口
- `sdk/agent/execution/utils/agent-interruption-utils.ts:35` — `checkAgentInterruption()`
- `sdk/agent/state-managers/agent-loop-state.ts:171` — `_interruptionRecords`

### 1.5 事件系统

| 能力 | 实现 | 状态 |
|---|---|---|
| 事件历史查询 | `getEvents()` / `getRecentEvents()` | 部分 |
| 事件搜索 | `searchEvents()` | 部分 |
| 执行时间线事件 | `getExecutionTimeline()` | 部分 |
| Agent 事件 | `getAgentEvents()` / `getAgentTurnEvents()` | 部分 |
| 持久化 | 仅内存循环缓冲区(1000条) | 缺口 |

**相关文件：**
- `sdk/api/shared/resources/events/event-resource-api.ts:372` — `getEvents()`
- `sdk/api/shared/resources/events/event-resource-api.ts:453` — `getEventTimeline()`
- `sdk/api/shared/resources/events/event-resource-api.ts:824` — `getAgentEvents()`

### 1.6 持久化层

| 能力 | 状态 |
|---|---|
| `PersistenceLayer` 接口定义 | 完整 |
| `NoOpPersistenceLayer` 实现 | 仅测试用 |
| 数据库实现(SQLite/Postgres/File) | 缺失 |
| 历史执行重建 | 缺失 |
| Agent Loop 持久化 | 仅基础元数据 |

**相关文件：**
- `sdk/api/shared/core/persistence-interfaces.ts:53` — `PersistenceLayer` 接口
- `sdk/api/shared/core/persistence-interfaces.ts:139` — `NoOpPersistenceLayer`

### 1.7 设计问题诊断

#### 1.7.1 API 不对称问题

Workflow 和 Agent 执行历史查询能力的对标分析：

| 维度 | Workflow | Agent | 差异 | 影响 |
|---|---|---|---|---|
| 执行摘要列表 | ✓ `getExecutionSummaries()` | ✗ | 缺失 | 无法批量查询执行历史 |
| 执行统计 | ✓ `getExecutionStatistics()` | ✗ | 缺失 | 无法获取系统级执行统计 |
| 执行时间线 | ✓ `getExecutionTimeline()` | ✗ | 缺失 | 无法追踪执行进度 |
| 变量历史 | ✓ `getVariableHistory()` | ✗ | 缺失 | Agent 调试困难 |
| 上下文演进 | ✓ `getContextEvolution()` | ✗ | 缺失 | 无法追踪状态变化 |
| 执行路径 | ✓ `getExecutionPath()` | ✗ | 缺失 | 无法回放执行分支 |

**根本原因**：Workflow 作为先行系统已建立完整查询体系，Agent Loop 仅复用基础状态接口，API 设计明显滞后。

**影响范围**：所有使用 Agent 执行的开发者体验不一致，学习成本高。

#### 1.7.2 数据持久化缺陷

持久化层设计与实现的关键问题：

| 方面 | 现状 | 问题 | 影响 |
|---|---|---|---|
| 事件存储 | 内存循环缓冲区(1000 条固定) | 容量固定，长时间运行丢失数据 | 查询无法回溯完整历史 |
| 错误记录 | 有上限 `EXECUTION_STATE_MAX_ERROR_RECORDS` | 超出限制后历史丢失 | 无法进行完整错误审计 |
| 执行历史 | 仅内存存储 + NoOp 实现 | 重启后全部丢失 | 历史查询功能形同虚设 |
| 中断记录 | 内部属性 `_interruptionRecords` 私有 | 无外部访问接口 + 无持久化 | 无法审计中断频率和恢复历史 |

**结论**：所有历史数据不可靠，系统仅适合单次执行查询。

#### 1.7.3 记录内容完整性评估

当前系统缺失的关键数据维度：

**执行流追踪缺失**
- ✗ 变量演进路径（何时改变、如何改变、为何改变）
- ✗ 决策分支选择依据（条件判断过程、条件值）
- ✗ 上下文转移时刻（关键状态切换、快照对比）

**性能可观测性缺失**
- ✗ 端到端时间线（完整执行耗时分布、关键路径识别）
- ✗ 操作级进度 API（细粒度进度跟踪、预估完成时间）
- ✗ 性能瓶颈识别（哪些节点最耗时、资源消耗热点）

**错误可观测性缺失**
- ✗ 错误频率统计（按类型、时间段、错误率趋势）
- ✗ 错误聚合分析（相似错误合并、错误模式识别）
- ✗ 错误热点识别（易出错代码路径、高风险操作）
- ✗ 错误关联分析（错误间的因果关系、错误链路）

**中断管理缺失**
- ✗ 中断频率和模式统计
- ✗ 中断恢复成功率追踪
- ✗ 中断-错误关联分析
- ✗ 中断恢复时间预估

---

## 2. 缺失项优先级矩阵

### 2.1 P0 - 影响功能完整性（必做）

这些缺失会导致核心功能不可用，是优先实现的基础。

| 缺失项 | 影响范围 | 根本原因 | 建议方案 | 文件位置 |
|---|---|---|---|---|
| **Agent 执行历史 API** | 所有使用 Agent 的用户 | API 设计不对称，Agent 缺少对标 Workflow 的查询接口 | 实现 `getExecutionSummaries()`/`getExecutionStatistics()`/`getExecutionTimeline()` 等 | `sdk/agent/state-managers/agent-loop-state.ts` |
| **事件系统持久化** | 所有执行历史查询场景 | 事件仅内存存储(1000 条循环缓冲) | 实现数据库持久化，支持事件历史恢复 | `sdk/api/shared/core/persistence-interfaces.ts:53` |
| **中断历史查询 API** | 中断调试和恢复分析 | `_interruptionRecords` 是私有属性，无公开 API | 添加 `getInterruptionHistory()` 公共接口 | `sdk/agent/state-managers/agent-loop-state.ts:171` |

### 2.2 P1 - 提升可观测性（应做）

这些缺失影响系统可观测性和问题诊断，建议在 P0 之后立即实施。

| 缺失项 | 影响范围 | 关键指标 | 工作量 | 依赖关系 |
|---|---|---|---|---|
| **错误统计 API** | 系统可靠性分析 | 错误频率、错误热点、错误率趋势 | 中 | 事件持久化 |
| **变量/上下文历史** | Agent 调试和问题追踪 | 变量演进路径、上下文快照对比 | 中 | Agent API 完成 |
| **性能时间线 API** | 瓶颈识别、SLA 监控 | 端到端耗时、节点耗时分布、关键路径 | 中 | 事件持久化 |
| **错误搜索 API** | 大规模错误排查 | 按消息/类型/时间段搜索 | 小 | 错误统计 API |
| **中断频率统计** | 中断模式识别 | 中断频率、恢复成功率、平均暂停时间 | 小 | 中断历史 API |

### 2.3 P2 - 长期优化（可选）

这些缺失适用于特定场景，可在后期按需实现。

| 缺失项 | 适用场景 | 优先级理由 | 预期工作量 |
|---|---|---|---|
| 跨执行类型统一查询 API | 复杂多层次执行(Agent + Workflow 混合) | 较少用户需求，可使用分别查询代替 | 大 |
| 操作级进度 API | 实时执行监控面板、进度条展示 | 可用事件系统替代，且需要前端配合 | 中 |
| 历史执行重建 API | 执行回放、调试工具 | 仅用于高级调试场景 | 大 |
| 相似错误聚合查询 | 错误模式识别、根因分析自动化 | 依赖复杂算法，后期优化 | 大 |

---

## 3. 数据覆盖度评估

### 3.1 应记录内容完整性矩阵

当前系统在不同数据维度的覆盖情况：

| 内容类别 | 具体内容 | Workflow | Agent | 持久化 | 覆盖度 |
|---|---|---|---|---|---|
| **执行元数据** | ID、状态、时间戳 | ✓ | ✓ | ✗ | 66% |
| | 输入输出参数 | ✓ | ✓ | ✗ | 66% |
| **执行流追踪** | 节点执行序列 | ✓ | ✗ | ✗ | 33% |
| | 变量演进历史 | ✓ | ✗ | ✗ | 33% |
| | 决策分支选择 | ✓ | ✗ | ✗ | 33% |
| | 上下文快照 | ✓ | ✗ | ✗ | 33% |
| **性能指标** | 节点耗时 | ✓ | ✗ | ✗ | 33% |
| | 端到端耗时 | ✗ | ✗ | ✗ | 0% |
| | 资源消耗(CPU/Memory) | ✗ | ✗ | ✗ | 0% |
| **错误信息** | 错误链和根因 | ✓ | ✓ | ✗ | 66% |
| | 错误频率统计 | ✗ | ✗ | ✗ | 0% |
| | 错误聚合分析 | ✗ | ✗ | ✗ | 0% |
| **中断事件** | 中断检测和类型 | ✓ | ✓ | ✗ | 66% |
| | 中断历史 | ✗ | ✗ | ✗ | 0% |
| | 中断频率和恢复率 | ✗ | ✗ | ✗ | 0% |
| **总体覆盖度** | | | | | **32%** |

### 3.2 覆盖度缺失分析

#### 第一层缺失（功能不存在）

这些数据在系统中完全没有实现：
- 变量演进路径（无法追踪变量如何、何时、为何改变）
- 性能端到端追踪（无法识别执行瓶颈）
- 错误热点识别（无法统计错误频率）
- 中断模式分析（无法识别中断规律）
- 资源消耗指标（无内存/CPU 跟踪）

**影响**：系统可观测性严重不足。

#### 第二层缺失（存在但无持久化）

这些数据在内存中存在，但无法跨会话保留：
- 所有执行历史在重启后丢失
- 无法进行历史趋势分析
- 审计日志不完整
- 长期执行中数据被覆盖

**影响**：历史查询功能形同虚设。

#### 第三层缺失（API 不对称）

Workflow 和 Agent 的查询能力差异：
- Agent 无 Workflow 等价的查询接口（缺少 7 个对标 API）
- 开发者体验不一致
- 学习成本高

**影响**：使用体验分裂。

---

## 4. 实现路线图

### 4.1 S1 - 短期（P0 优先级）

**目标**：实现功能完整性，使系统可用

| 任务 | 工作量 | 关键文件 | 预期周期 | 验收标准 |
|---|---|---|---|---|
| 实现 Agent 执行历史 API | 中 | `sdk/agent/state-managers/agent-loop-state.ts` | 1-2 周 | 支持 getExecutionSummaries/Statistics/Timeline 等 7 个 API |
| 实现公开中断历史查询 API | 小 | `sdk/agent/state-managers/agent-loop-state.ts:171` | 2-3 天 | `getInterruptionHistory()` 可公开访问 |
| 实现事件系统持久化 | 大 | `sdk/api/shared/core/persistence-interfaces.ts` | 2-3 周 | SQLite/Postgres 双后端支持 |
| 修复错误记录上限 | 小 | `sdk/agent/state-managers/agent-loop-state.ts:532` | 2-3 天 | 错误记录无上限（或可配置） |

**前置条件**：
- Agent API 需对齐 Workflow 设计模式
- 持久化实现需支持异步写入，避免阻塞执行
- 错误记录迁移需保证向后兼容

**成功指标**：
- Agent 用户可用 Workflow 等价的查询 API
- 重启后执行历史仍可查询
- 中断频率统计可用

### 4.2 S2 - 中期（P1 优先级）

**目标**：提升可观测性，支持问题诊断

| 任务 | 依赖 | 预期效果 | 预期周期 |
|---|---|---|---|
| 实现错误统计 API | S1 持久化完成 | 支持按类型、时间段、错误率统计 | 1 周 |
| 实现变量/上下文历史 API | S1 Agent API 完成 | 追踪变量演进和上下文变化 | 1-2 周 |
| 实现性能时间线 API | S1 事件持久化完成 | 完整的端到端耗时分析 | 1 周 |
| 实现错误搜索 API | 错误统计 API 完成 | 按消息/类型/时间段搜索 | 3-5 天 |

**成功指标**：
- 错误率趋势可视化
- 变量演进路径可追踪
- 性能瓶颈可识别
- 错误快速搜索可用

### 4.3 S3 - 长期（P2 优先级）

**目标**：高级功能和优化

| 任务 | 适用场景 | 预期周期 |
|---|---|---|
| 跨执行类型统一查询 API | Agent + Workflow 混合执行 | 2-3 周 |
| 操作级进度 API | 实时进度面板 | 1 周 |
| 历史执行回放工具 | 调试和根因分析 | 2-3 周 |
| 相似错误聚合 | 错误模式识别 | 2-3 周 |

---

## 5. 数据一致性建议

### 5.1 API 设计对齐标准

所有执行历史查询 API 应遵循统一接口模式，便于开发者使用：

**单个查询接口**
```typescript
// 获取单次执行的详细信息
getExecution<T>(executionId: string): Promise<T>
```

**列表查询接口（分页）**
```typescript
// 获取执行列表，支持过滤和分页
getExecutions<T>(filter: ExecutionFilter, pagination: { limit: number; offset: number }): Promise<{ items: T[]; total: number }>
```

**统计查询接口**
```typescript
// 获取执行统计数据
getStatistics(filter: ExecutionFilter): Promise<{ total: number; success: number; failed: number; avgDuration: number }>
```

**时间线查询接口**
```typescript
// 获取执行的时间线事件
getTimeline(executionId: string): Promise<TimelineEvent[]>
```

**历史查询接口**
```typescript
// 获取特定资源的历史记录（变量、上下文等）
getHistory<T>(executionId: string, resource: string, pagination?: { limit: number; offset: number }): Promise<T[]>
```

### 5.2 数据完整性约束

| 约束项 | 现状 | 建议 | 优先级 |
|---|---|---|---|
| 错误记录上限 | `EXECUTION_STATE_MAX_ERROR_RECORDS`(有限制) | 可配置，默认 10000+ | P0 |
| 事件缓冲区大小 | 1000 条固定 | 可配置，长期使用无损 | P0 |
| 数据保留期 | 无 | 配置保留策略(30/90/180/365 天) | P1 |
| 数据完整性校验 | 无 | 持久化时验证，失败时报警 | P1 |
| 并发写入控制 | 无 | 队列化异步写入，避免丢失 | P0 |

### 5.3 向后兼容性

- 现有 Agent API 需保持不变，新 API 作为扩展
- 持久化层迁移时需无损转换现有数据
- 错误记录格式升级需支持旧版本识别
