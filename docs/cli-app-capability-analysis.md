# CLI-App 功能分析报告

## 1. CLI-App 当前实现的功能

### 1.1 核心命令模块（16个）

| 命令模块 | 主要功能 | 子命令 |
|---------|---------|--------|
| **workflow** | 工作流管理 | register, list, show, delete, batch |
| **execution** | 执行控制 | run, pause, resume, stop, status |
| **checkpoint** | 检查点管理 | create, load, list, show, delete |
| **template** | 模板管理 | register, list, delete |
| **llm-profile** | LLM配置 | list, register, update, delete |
| **agent** | 代理循环执行 | run, configure |
| **skill** | 技能管理 | register, list, delete |
| **tool** | 工具管理 | register, list, delete, execute |
| **script** | 脚本管理 | register, list, delete |
| **trigger** | 触发器配置 | list, enable, disable |
| **message** | 消息处理 | list, show |
| **variable** | 变量管理 | list, set, get, delete |
| **event** | 事件管理 | list, filter |
| **agent-profile** | 代理配置 | register, list, delete |
| **metrics** | 指标监控 | show, export |
| **node** | 节点模板 | register, list, delete |

### 1.2 核心架构特性

| 特性 | 描述 |
|-----|------|
| **执行模式** | TUI交互式、CLI命令行、Headless无交互 |
| **输出格式** | JSON、表格、文本，支持彩色输出 |
| **错误处理** | 统一错误码、自定义错误类 |
| **PTY支持** | 跨平台伪终端支持（Windows/Linux/Mac） |
| **存储适配器** | 支持多种存储后端 |
| **DI容器** | 依赖注入服务容器 |

---

## 2. SDK/API 层提供的功能

### 2.1 资源管理 API（24+个）

#### 共享资源 API
| API 名称 | 功能描述 | 关键方法 |
|---------|---------|---------|
| **ToolRegistryAPI** | 工具注册和执行 | get, list, create, update, delete, search, executeToolCommand |
| **ScriptRegistryAPI** | 脚本管理 | register, get, list, search |
| **ProfileRegistryAPI** | LLM配置管理 | list, get, register, update, delete |
| **SkillRegistryAPI** | 技能加载管理 | load, list, get, filter |
| **EventResourceAPI** | 事件调度和查询 | dispatch, query, filter, getStats, ExecutionTimeline |
| **MetricsResourceAPI** | 指标查询（只读） | getWorkflowMetrics, getNodeMetrics, getAgentMetrics, getTopWorkflows, exportMetrics |
| **TaskResourceAPI** | 任务管理 | create, list, get, update, delete, query, getStats |
| **SearchAPI** | 跨资源搜索 | search(工作流、执行、任务、检查点、事件、代理循环) |
| **StorageDiagnosticsAPI** | 存储健康监控 | getAdapterHealth, getItemCounts, getStorageStatus |

#### 工作流资源 API
| API 名称 | 功能描述 | 关键方法 |
|---------|---------|---------|
| **WorkflowGraphQueryAPI** | ⭐ 工作流图结构查询 | getGraph, getGraphSummary, getNodes, getNodesByType, getEdges, getNodeNeighbors, getAnalysis（循环检测、拓扑排序） |
| **WorkflowExecutionRegistryAPI** | 执行追踪和管理 | get, list, create, update, delete, filter by status/date |
| **CheckpointResourceAPI** | 检查点管理 | create, list, get, delete, restore, analyze, CheckpointTransition |
| **FileCheckpointResourceAPI** | ⭐ 文件检查点管理 | list, get, delete, validate, 文件路径管理、校验和验证 |
| **VariableResourceAPI** | 变量管理 | get, list, create, update, delete |
| **MessageResourceAPI** | 消息存储查询 | list, get by execution/workflow, getStats |
| **TriggerResourceAPI** | ⭐ 触发器管理 | get, list, enable, disable, delete（状态管理） |
| **TriggerTemplateRegistryAPI** | 触发器模板 | register, get, list, search, delete |
| **NodeRegistryAPI** | 节点模板 | register, get, list, search |
| **UserInteractionResourceAPI** | ⭐ 用户交互追踪 | create, list, get, delete, filter by execution/type |

#### 代理资源 API
| API 名称 | 功能描述 | 关键方法 |
|---------|---------|---------|
| **AgentLoopIterationAPI** | ⭐ 迭代分析 | getDetail, getHistory, getDecisionAnalysis, getExecutionPathAnalysis, SystemMetrics, LLMMetrics |
| **AgentLoopCheckpointResourceAPI** | 代理检查点 | list, get, delete, query |
| **AgentLoopMessageResourceAPI** | 代理消息 | list, get, create, query |
| **AgentVariableResourceAPI** | 代理变量 | get, list, create, update, delete |
| **AgentUserInteractionResourceAPI** | ⭐ 代理交互追踪 | list, get, create, delete |
| **AgentTriggerResourceAPI** | 代理触发器 | get, list, enable, disable |
| **其他代理API** | 代理文件检查点、模板等 | ... |

### 2.2 命令执行 API

#### 工作流命令
- ExecuteWorkflowCommand
- ExecuteWorkflowStreamCommand
- PauseWorkflowCommand / ResumeWorkflowCommand
- CancelWorkflowCommand
- WorkflowCreateCheckpointCommand
- RestoreFromCheckpointCommand
- EnableTriggerCommand / DisableTriggerCommand

#### 代理命令
- RunAgentLoopCommand / RunAgentLoopStreamCommand
- CancelAgentLoopCommand
- PauseAgentLoopCommand / ResumeAgentLoopCommand
- 代理检查点命令
- 代理触发器命令

### 2.3 工具类

| 工具类 | 描述 |
|-------|------|
| **WorkflowBuilder** | 工作流定义的流畅API |
| **ExecutionBuilder** | 执行设置的流畅API |
| **NodeTemplateBuilder** | 节点模板构造 |
| **TriggerTemplateBuilder** | 触发器模板构造 |
| **AgentLoopConfigBuilder** | 代理配置 |

---

## 3. SDK-Kit 层提供的功能

### 3.1 高层 API 包装

| API | 功能 | 关键特性 |
|-----|------|---------|
| **WorkflowAPI** | ⭐ 工作流定义 | 流畅构建器、Result模式错误处理、模板验证 |
| **ExecutionAPI** | 简化执行 | 输入/输出处理、事件发射 |
| **QueryAPI** | ⭐ 高级查询 | 过滤、排序、分页、date range过滤 |
| **ResourceAPI** | ⭐ 资源管理 | 完整CRUD + 版本控制（create, read, update, delete, clone, listVersions, rollback） |
| **EventManager** | ⭐ 事件管理 | 事件订阅、历史追踪（10000事件）、事件查询 |
| **Analysis** | ⭐ 执行分析 | 执行对比、性能对比、中断恢复分析、进度跟踪 |
| **StarterRegistry** | ⭐ 启动器系统 | 预构建工作流模板（如GoalReviewStarter） |

### 3.2 Result 模式错误处理

```typescript
// 函数式错误处理，无异常
const result = await kit.resource().workflows().create(template);
result
  .andThen(id => kit.resource().workflows().read(id))
  .orElse(error => console.error(error))
```

### 3.3 预构建资源

- **GoalReviewStarter**: 目标驱动评审循环
- **executorTemplate**: 执行器代理模板
- **reviewerTemplate**: 评审器代理模板

---

## 4. CLI-App 缺失功能对比

### 4.1 SDK/API 提供但 CLI-App 缺失的功能

#### 高优先级（常用、核心功能）

| 功能 | 来源 | 描述 | CLI命令建议 |
|-----|-----|------|-----------|
| **工作流图查询** ⭐⭐⭐ | WorkflowGraphQueryAPI | 获取工作流的图结构、拓扑分析、节点邻接关系、循环检测 | `workflow graph show <id>` / `workflow graph analyze <id>` |
| **跨资源搜索** ⭐⭐⭐ | SearchAPI | 支持模糊搜索工作流、执行、任务、检查点、事件、代理 | `search <query> [--type workflow/execution/...]` |
| **代理迭代分析** ⭐⭐⭐ | AgentLoopIterationAPI | 获取迭代详情、决策分析、执行路径、性能指标 | `agent-loop iteration show <id>` / `agent-loop iteration analyze <id>` |
| **存储诊断** ⭐⭐ | StorageDiagnosticsAPI | 存储适配器健康检查、项目计数统计 | `storage diagnose` / `storage health` |
| **用户交互追踪** ⭐⭐ | UserInteractionResourceAPI | 追踪、查询、管理用户交互事件 | `execution interaction list <execution-id>` |
| **执行对比分析** ⭐⭐ | Analysis API (sdk-kit) | 比较两个或多个执行的性能差异 | `execution compare <id1> <id2> [--metrics]` |
| **版本控制管理** ⭐⭐ | ResourceAPI (sdk-kit) | 工作流版本管理、回滚 | `workflow version list/rollback <id>` |
| **文件检查点验证** | FileCheckpointResourceAPI | 检查点文件路径管理、校验和验证 | `checkpoint file validate <path>` |
| **触发器状态管理** | TriggerResourceAPI | 触发器的启用/禁用/删除（已有基础，可扩展） | 命令已有，可增强功能 |

#### 中优先级（实用功能）

| 功能 | 来源 | 描述 | CLI命令建议 |
|-----|-----|------|-----------|
| **高级查询API** ⭐ | QueryAPI (sdk-kit) | 执行、工作流、任务的高级过滤、排序、分页 | `execution list --filter status=completed --sort duration --limit 10` |
| **事件历史查询** | EventResourceAPI | 支持按类型、工作流、执行过滤事件历史 | `event query --workflow <id> --type execution_completed` |
| **代理交互追踪** | AgentUserInteractionResourceAPI | 代理级别的用户交互管理 | `agent-loop interaction list <agent-loop-id>` |
| **任务管理** | TaskResourceAPI | 任务的CRUD和统计 | `task create/list/update/delete` |
| **工作流流畅构建** | WorkflowBuilder (sdk-kit) | 代码方式定义工作流（JSON Schema验证支持） | 可通过`workflow create --builder`实现 |
| **进度实时跟踪** | Analysis API (sdk-kit) | 执行进度、ETA、时间线 | `execution progress <id>` |

#### 低优先级（特定场景）

| 功能 | 来源 | 描述 |
|-----|-----|------|
| **代理文件检查点** | AgentFileCheckpointResourceAPI | 代理级别检查点验证 |
| **代理触发器模板** | AgentTriggerTemplateRegistryAPI | 代理级触发器模板 |
| **代理Hook模板** | AgentHookTemplateRegistryAPI | 代理级Hook模板 |

---

### 4.2 SDK-Kit 特性但 CLI-App 缺失的功能

| 特性 | 描述 | CLI实现建议 |
|-----|------|-----------|
| **Result 模式错误处理** | 函数式错误处理替代异常 | CLI框架升级：统一ResultType返回 |
| **启动器系统** | 预构建工作流模板（GoalReviewStarter等） | `workflow starter list` / `workflow starter create <name>` |
| **版本控制和回滚** | 工作流版本管理、历史追踪、回滚 | `workflow version list/show/rollback <id>` |
| **执行对比和分析** | 执行性能对比、进度跟踪、中断恢复分析 | `execution compare <id1> <id2>` / `execution analyze <id>` |

---

## 5. 功能补充建议（按优先级）

### 5.1 第一阶段（高优先级，核心功能）

#### 1. **工作流图查询和可视化** 📊
```bash
# 新增命令
workflow graph show <workflow-id>          # 显示工作流图结构
workflow graph analyze <workflow-id>       # 分析图拓扑（循环检测、关键路径）
workflow graph nodes <workflow-id>         # 列出所有节点
workflow graph edges <workflow-id>         # 列出所有边/连接
```

**实现位置：** `cli-app/src/commands/workflow-graph/`
**依赖：** WorkflowGraphQueryAPI

#### 2. **跨资源搜索** 🔍
```bash
# 新增命令
search <query>                             # 模糊搜索所有资源
search <query> --type workflow             # 按类型过滤
search <query> --type execution --limit 20 # 搜索执行记录
```

**实现位置：** `cli-app/src/commands/search/`
**依赖：** SearchAPI

#### 3. **代理循环迭代分析** 🔬
```bash
# 新增命令
agent-loop iteration show <agent-loop-id> <iteration-index>
agent-loop iteration analyze <agent-loop-id>          # 分析所有迭代
agent-loop iteration decision <agent-loop-id> <index> # 查看决策详情
```

**实现位置：** `cli-app/src/commands/agent-loop/iteration/`
**依赖：** AgentLoopIterationAPI

#### 4. **存储诊断** 🏥
```bash
# 新增命令
storage diagnose                           # 完整诊断报告
storage health                             # 简化健康状态
storage stats                              # 存储统计信息
```

**实现位置：** `cli-app/src/commands/storage/`
**依赖：** StorageDiagnosticsAPI

### 5.2 第二阶段（中优先级，实用功能）

#### 5. **执行高级查询和过滤** 🔎
```bash
# 增强现有execution list命令
execution list --filter "status=completed,duration>1000"
execution list --sort duration --order desc --limit 10
execution list --date-range "2024-01-01 to 2024-01-31"
```

**实现位置：** 增强 `cli-app/src/commands/workflow-execution/`
**依赖：** QueryAPI, WorkflowExecutionRegistryAPI

#### 6. **执行对比分析** 📈
```bash
# 新增命令
execution compare <id1> <id2> [id3...]     # 对比多个执行
execution compare <id1> <id2> --metrics    # 显示性能指标
execution compare <id1> <id2> --timeline   # 显示时间线对比
```

**实现位置：** `cli-app/src/commands/workflow-execution/compare/`
**依赖：** Analysis API (需集成sdk-kit)

#### 7. **工作流版本管理** 📦
```bash
# 新增命令
workflow version list <workflow-id>        # 列出版本历史
workflow version show <workflow-id> <version>
workflow version rollback <workflow-id> <version>
workflow version diff <workflow-id> <v1> <v2>
```

**实现位置：** `cli-app/src/commands/workflow/version/`
**依赖：** ResourceAPI (sdk-kit)

#### 8. **用户交互追踪** 👤
```bash
# 新增命令
execution interaction list <execution-id>  # 列出执行中的交互
execution interaction show <execution-id> <interaction-id>
agent-loop interaction list <agent-loop-id>
```

**实现位置：** `cli-app/src/commands/execution/interaction/`
**依赖：** UserInteractionResourceAPI, AgentUserInteractionResourceAPI

#### 9. **实时进度跟踪** ⏱️
```bash
# 新增命令
execution progress <execution-id>          # 显示实时进度
execution progress <execution-id> --watch  # 持续监控
agent-loop progress <agent-loop-id>
```

**实现位置：** `cli-app/src/commands/execution/progress/`
**依赖：** Analysis API (sdk-kit)

### 5.3 第三阶段（低优先级，特定场景）

#### 10. **预构建启动器系统** 🚀
```bash
# 新增命令
workflow starter list                      # 列出可用启动器
workflow starter create <name> --starter GoalReviewStarter
workflow starter show <starter-id>
```

**实现位置：** `cli-app/src/commands/workflow/starter/`
**依赖：** StarterRegistry (sdk-kit)

#### 11. **事件历史高级查询** 📋
```bash
# 增强现有event命令
event list --filter "type=execution_completed"
event list --workflow <workflow-id> --type node_failed
event timeline <execution-id>               # 事件时间线
```

**实现位置：** 增强 `cli-app/src/commands/event/`
**依赖：** EventResourceAPI

---

## 6. 实现路线图

### Phase 1: 核心查询功能（第1-2周）
- [ ] 工作流图查询（完整的拓扑查询）
- [ ] 跨资源搜索
- [ ] 存储诊断

### Phase 2: 分析和追踪（第3-4周）
- [ ] 代理迭代分析
- [ ] 执行对比分析
- [ ] 用户交互追踪

### Phase 3: 版本和管理（第5-6周）
- [ ] 工作流版本管理
- [ ] 高级查询和过滤
- [ ] 实时进度跟踪

### Phase 4: 高级特性（第7-8周）
- [ ] 启动器系统集成
- [ ] 事件历史查询增强

---

## 7. 代码实现检查清单

### 适配器层 (`adapters/`)
- [ ] GraphQueryAdapter（工作流图查询）
- [ ] SearchAdapter（跨资源搜索）
- [ ] DiagnosticsAdapter（存储诊断）
- [ ] IterationAnalysisAdapter（迭代分析）
- [ ] ExecutionComparisonAdapter（执行对比）
- [ ] UserInteractionAdapter（用户交互）
- [ ] VersionManagementAdapter（版本管理）
- [ ] ProgressTrackingAdapter（进度跟踪）

### 命令层 (`commands/`)
- [ ] `workflow-graph/` - 工作流图命令
- [ ] `search/` - 搜索命令
- [ ] `storage/` - 存储诊断命令
- [ ] `agent-loop/iteration/` - 迭代分析命令
- [ ] `workflow-execution/compare/` - 执行对比命令
- [ ] `workflow/version/` - 版本管理命令
- [ ] `execution/interaction/` - 交互追踪命令
- [ ] `execution/progress/` - 进度跟踪命令
- [ ] `workflow/starter/` - 启动器命令

### 测试和文档
- [ ] 各模块单元测试
- [ ] 命令集成测试
- [ ] CLI使用文档
- [ ] 命令示例说明

---

## 8. SDK 集成要点

### 需要集成的新 API
```typescript
// 工作流图查询
import { WorkflowGraphQueryAPI } from '@wf-agent/sdk/api'

// 搜索
import { SearchAPI } from '@wf-agent/sdk/api'

// 存储诊断
import { StorageDiagnosticsAPI } from '@wf-agent/sdk/api'

// 代理迭代分析
import { AgentLoopIterationAPI } from '@wf-agent/sdk/api'

// 用户交互
import { UserInteractionResourceAPI, AgentUserInteractionResourceAPI } from '@wf-agent/sdk/api'

// SDK-Kit 集成（版本管理、执行对比、进度跟踪）
import { SDKKit } from '@wf-agent/sdk-kit'
```

### 现有适配器的扩展
- `workflow-adapter.ts` - 增加图查询方法
- `workflow-execution-adapter.ts` - 增加对比和进度跟踪
- `tool-adapter.ts` - 增加搜索支持

---

## 9. 总结表

| 功能 | 优先级 | 复杂度 | 预估周期 | 依赖SDK |
|-----|--------|--------|---------|--------|
| 工作流图查询 | 🔴 高 | 中 | 3-4天 | WorkflowGraphQueryAPI |
| 跨资源搜索 | 🔴 高 | 中 | 3-4天 | SearchAPI |
| 存储诊断 | 🔴 高 | 低 | 1-2天 | StorageDiagnosticsAPI |
| 代理迭代分析 | 🟠 中 | 中 | 3-4天 | AgentLoopIterationAPI |
| 执行高级查询 | 🟠 中 | 低 | 2-3天 | QueryAPI |
| 执行对比分析 | 🟠 中 | 高 | 4-5天 | Analysis API |
| 工作流版本管理 | 🟠 中 | 中 | 3-4天 | ResourceAPI |
| 用户交互追踪 | 🟠 中 | 低 | 2-3天 | UserInteractionResourceAPI |
| 实时进度跟踪 | 🟠 中 | 中 | 3-4天 | Analysis API |
| 启动器系统 | 🟡 低 | 中 | 3-4天 | StarterRegistry |

**总计预期工作量：** 约 6-8 周完整实现所有功能
