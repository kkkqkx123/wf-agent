# CLI-App vs SDK/API vs SDK-Kit 功能对比矩阵

## 核心能力矩阵

```
功能域                    CLI-App         SDK/API         SDK-Kit
================================================================================
工作流管理
├─ 定义                   ✓ register       ✓ builder       ✓ fluent API
├─ 存储                   ✓ CRUD          ✓ registry      ✓ versioning
├─ 查询                   △ 基础           ✓ 完整          ✓ 高级查询
├─ 图结构分析             ✗               ✓ graph API     ✓ graph API
└─ 版本管理               ✗               ✗               ✓ rollback

执行管理
├─ 基础执行               ✓ run/pause/resume ✓ commands    ✓ execute API
├─ 性能对比               ✗               ✗               ✓ comparison
├─ 进度跟踪               ✗               △ events        ✓ analysis
├─ 历史查询               ✓ list          ✓ registry      ✓ query API
└─ 交互追踪               ✗               ✓ API           ✓ included

检查点管理
├─ 基础操作               ✓ CRUD          ✓ resource      ✓ API
├─ 文件检查点             ✗               ✓ file API      ✓ included
├─ 分析转换               ✗               ✓ analysis      ✓ analysis
└─ 验证和修复             ✗               △ validate      ✓ validate

代理管理
├─ 基础执行               ✓ agent run     ✓ commands      ✓ execute
├─ 迭代分析               ✗               ✓ iteration API ✓ detailed
├─ 决策追踪               ✗               ✓ decision      ✓ analysis
├─ 交互追踪               ✗               ✓ interaction   ✓ included
└─ 模板管理               ✓ agent-profile ✓ registry      ✓ templates

事件和日志
├─ 事件派发               ✗               ✓ dispatch      ✓ emit
├─ 事件查询               ✓ list/filter   ✓ query/filter  ✓ history
├─ 事件历史               ✗               △ basic         ✓ 10k buffer
└─ 时间线分析             ✗               ✓ timeline      ✓ analysis

搜索和发现
├─ 工作流搜索             ✓ list          ✓ search        ✓ query
├─ 执行搜索               ✓ list          ✓ registry      ✓ advanced
├─ 跨资源搜索             ✗               ✓ SearchAPI     ✓ multi-type
├─ 模糊匹配               ✗               ✓ fuzzy         ✓ fuzzy
└─ 评分排序               ✗               ✓ score-based   ✓ ranked

系统监控
├─ 指标查询               ✓ metrics cmd   ✓ MetricsAPI    ✓ included
├─ 存储诊断               ✗               ✓ diagnostics   ✓ included
├─ 性能分析               ✗               △ event stats   ✓ detailed
└─ 导出报告               ✓ export        ✓ prometheus    ✓ included

错误处理
├─ 异常捕获               ✓ try-catch     ✓ exceptions    ✓ Result<>
├─ 错误上下文             ✓ basic         ✓ detailed      ✓ chain
├─ 恢复策略               ✗               △ retry         ✓ recovery
└─ 错误链传播             ✗               ✓ cause         ✓ chain

配置管理
├─ JSON/TOML             ✓ register      ✓ parser        ✓ loader
├─ 环境覆盖               ✓ env vars      ✓ mapping       ✓ included
├─ 验证                   ✓ zod           ✓ validators    ✓ validation
└─ 预设模板               ✗               △ builders      ✓ starters
================================================================================
图例: ✓ 完整支持  △ 部分支持  ✗ 不支持
```

---

## 详细功能映射

### 1. 工作流图查询能力（只有SDK提供）

| 功能 | 描述 | 位置 |
|-----|------|------|
| getGraph() | 获取完整工作流图 | WorkflowGraphQueryAPI |
| getNodes() | 查询所有节点 | WorkflowGraphQueryAPI |
| getEdges() | 获取所有连接边 | WorkflowGraphQueryAPI |
| getNodeNeighbors() | 节点邻接关系 | WorkflowGraphQueryAPI |
| analyzeTopology() | 拓扑分析 | WorkflowGraphQueryAPI |
| detectCycles() | 循环检测 | WorkflowGraphQueryAPI |
| **CLI-App缺失** | **需要新增命令层** | **workflow graph/* |

---

### 2. 代理迭代分析能力（SDK特有，SDK-Kit改进）

```
SDK/API 提供                          SDK-Kit 改进
├─ getIterationDetails()             ├─ ExtendedIterationDetail
├─ getIterationHistory()             ├─ 历史管理
├─ getDecisionAnalysis()             ├─ 决策可视化
├─ getExecutionPathAnalysis()        ├─ 路径追踪
├─ getSystemMetrics()                └─ 性能聚合
└─ getLLMMetrics()

CLI-App 现状: agent run 只有基础执行，无分析功能
需要补充: agent-loop iteration {show/analyze/decision}
```

---

### 3. 搜索能力对比

```
CLI-App                 SDK/API (SearchAPI)      SDK-Kit (QueryAPI)
├─ workflow list       ├─ SearchAPI.search()    ├─ advanced filters
├─ execution list      ├─ 模糊匹配 fuzzy        ├─ sort/pagination
├─ 精确ID查询          ├─ 评分排序 score        ├─ date ranges
└─ 无跨资源搜索        ├─ 类型过滤 type         └─ 复合查询

跨资源搜索支持:
├─ workflows
├─ executions
├─ tasks
├─ checkpoints
├─ events
└─ agent-loops

CLI-App缺失: search <query> --type [workflow|execution|...]
```

---

### 4. 版本管理能力（只有SDK-Kit提供）

```
功能                    SDK-Kit ResourceAPI
├─ create()            ✓ 创建资源版本
├─ read()              ✓ 读取特定版本
├─ update()            ✓ 更新版本
├─ delete()            ✓ 删除版本
├─ clone()             ✓ 克隆版本
├─ listVersions()      ✓ 版本历史列表
└─ rollback()          ✓ 回滚到历史版本

CLI-App: 无版本控制
SDK: 无版本功能
SDK-Kit: 完整版本管理

需要CLI新增: workflow version {list|show|rollback|diff}
```

---

### 5. 执行对比分析能力（只有SDK-Kit）

```
执行对比功能           API 位置
├─ 并行对比          Analysis API
├─ 性能差异          comparison.ts
├─ 时间线对比        timeline analysis
├─ 中断恢复分析      recovery analysis
├─ 错误变化          error diff
└─ 吞吐量对比        metrics comparison

SDK-Kit 返回:
{
  executionIds: [],
  duration: { delta, percentage },
  success_rate: { comparison },
  errors: { added, removed },
  interruptions: { recovery_analysis }
}

CLI-App: 完全缺失
需要新增: execution compare <id1> <id2> [--metrics]
```

---

### 6. 实时进度跟踪能力（SDK-Kit特有）

```
进度跟踪功能           SDK-Kit Analysis API
├─ 实时百分比        ✓ currentProgress
├─ 节点完成状态      ✓ nodesCompleted
├─ ETA 时间估算      ✓ estimatedTime
├─ 当前活动          ✓ activeNodes
├─ 历史速率          ✓ throughputAnalysis
└─ 中断点恢复        ✓ interruptionAnalysis

CLI-App: 无进度跟踪
需要新增: execution progress <id> [--watch]
```

---

## 按功能域的补充建议

### 🎯 Query 能力（执行、工作流、任务查询）

| 功能 | CLI现状 | SDK提供 | 优先级 |
|-----|---------|--------|--------|
| 高级过滤 | `list` 仅ID | QueryAPI + 20+ filter | 🔴 高 |
| 排序 | 无 | QueryAPI sort | 🟠 中 |
| 分页 | 无 | QueryAPI limit/offset | 🟠 中 |
| Date Range | 无 | QueryAPI dateRange | 🟠 中 |
| 统计聚合 | 无 | QueryAPI.getStats() | 🟠 中 |

**命令示例:**
```bash
execution list --filter "status=completed" --sort "duration" --limit 10
execution list --date-range "2024-01-01 to 2024-01-31"
task list --status "pending" --limit 20 --offset 20
```

---

### 🔍 Graph 能力（工作流拓扑、可视化）

| 功能 | CLI现状 | SDK提供 | 优先级 |
|-----|---------|--------|--------|
| 图结构 | 无 | getGraph() | 🔴 高 |
| 节点分析 | 无 | getNodes() | 🔴 高 |
| 拓扑检查 | 无 | analyzeTopology() | 🔴 高 |
| 循环检测 | 无 | detectCycles() | 🟠 中 |
| 关键路径 | 无 | criticalPath() | 🟡 低 |

**命令示例:**
```bash
workflow graph show <id>                    # 显示图结构
workflow graph analyze <id>                 # 拓扑分析
workflow graph nodes <id> --type LLM       # 按类型筛选
workflow graph check-cycles <id>            # 循环检测
```

---

### 📊 Analysis 能力（对比、进度、性能）

| 功能 | CLI现状 | SDK-Kit提供 | 优先级 |
|-----|---------|------------|--------|
| 执行对比 | 无 | comparison() | 🟠 中 |
| 进度跟踪 | 无 | progress() | 🟠 中 |
| 中断恢复 | 无 | recovery() | 🟡 低 |
| 性能基线 | 无 | baseline() | 🟡 低 |

**命令示例:**
```bash
execution compare <id1> <id2>               # 对比两次执行
execution compare <id1> <id2> --timeline    # 时间线对比
execution progress <id>                     # 实时进度
execution progress <id> --watch             # 持续监控
```

---

## 三层架构功能补全树

```
CLI-App (用户界面层)
├── 已实现功能 (16个命令模块)
│   ├── workflow CRUD
│   ├── execution 基础
│   ├── checkpoint CRUD
│   ├── agent 基础执行
│   └── ...
└── 缺失功能 (需要补充)
    ├── 🔴 工作流图查询          ← WorkflowGraphQueryAPI
    ├── 🔴 跨资源搜索             ← SearchAPI
    ├── 🔴 存储诊断               ← StorageDiagnosticsAPI
    ├── 🟠 代理迭代分析          ← AgentLoopIterationAPI
    ├── 🟠 执行对比分析          ← Analysis API (sdk-kit)
    ├── 🟠 高级查询和过滤        ← QueryAPI (sdk-kit)
    ├── 🟠 版本管理              ← ResourceAPI (sdk-kit)
    ├── 🟠 用户交互追踪          ← UserInteractionResourceAPI
    ├── 🟠 实时进度跟踪          ← Analysis API (sdk-kit)
    └── 🟡 启动器系统            ← StarterRegistry (sdk-kit)

SDK/API 层 (功能引擎)
├── WorkflowGraphQueryAPI         → graph analysis
├── SearchAPI                     → cross-resource search
├── StorageDiagnosticsAPI         → storage health
├── AgentLoopIterationAPI         → iteration details
├── UserInteractionResourceAPI    → interaction tracking
├── MetricsResourceAPI            → metrics queries
├── EventResourceAPI              → event management
├── CheckpointResourceAPI         → checkpoint CRUD
├── TriggerResourceAPI            → trigger management
└── QueryAPI (sdk-kit)            → advanced queries

SDK-Kit 层 (高级抽象)
├── WorkflowAPI                   → fluent builders
├── ResourceAPI                   → versioning + CRUD
├── QueryAPI                      → advanced filtering
├── ExecutionAPI                  → simplified execution
├── Analysis                      → comparison + progress
├── EventManager                  → event history
├── StarterRegistry               → pre-built templates
└── Error Handling                → Result pattern
```

---

## 集成工作量评估

### 按复杂度

```
低复杂度 (1-2天):
├─ 存储诊断命令 (storage diagnose)
├─ 执行列表过滤 (execution list --filter)
├─ 事件查询增强 (event query --workflow)
└─ 用户交互列表 (execution interaction list)

中复杂度 (3-4天):
├─ 工作流图查询 (workflow graph show/analyze)
├─ 跨资源搜索 (search <query>)
├─ 代理迭代分析 (agent-loop iteration show)
├─ 版本管理 (workflow version list/rollback)
└─ 进度跟踪 (execution progress)

高复杂度 (4-5天):
├─ 执行对比分析 (execution compare)
├─ 启动器系统 (workflow starter create)
└─ 高级分析聚合

预期总工作量: 6-8 周，20+ 新命令
```

---

## 优先推荐实施顺序

```
第1周 (基础查询):
  1. 存储诊断 (简单快速赢)
  2. 工作流图查询 (核心功能)
  3. 跨资源搜索 (通用能力)

第2周 (执行管理):
  4. 执行高级过滤 (完善现有命令)
  5. 执行对比分析 (中等复杂)
  6. 用户交互追踪 (数据完整性)

第3周 (版本和分析):
  7. 工作流版本管理 (lifecycle管理)
  8. 代理迭代分析 (代理特有)
  9. 实时进度跟踪 (UX 改进)

第4周+ (高级特性):
  10. 启动器系统 (最后)
  11. 事件历史增强 (可选)
```

---

## 代码实现参考

### 现有适配器扩展位置

```typescript
// adapters/ 中需要新增或扩展
├── workflow-adapter.ts           // + graph query methods
├── workflow-execution-adapter.ts // + compare methods
├── search-adapter.ts             // 新增
├── storage-adapter.ts            // 新增
├── agent-iteration-adapter.ts    // 新增
└── progress-adapter.ts           // 新增

// commands/ 中需要新增
├── workflow-graph/               // 新增
├── search/                       // 新增
├── storage/                      // 新增
├── agent-loop/iteration/         // 新增
├── workflow-execution/compare/   // 新增
├── workflow/version/             // 新增
├── execution/interaction/        // 新增
└── execution/progress/           // 新增
```

### SDK 导入示例

```typescript
// 新的 API 导入
import {
  WorkflowGraphQueryAPI,
  SearchAPI,
  StorageDiagnosticsAPI,
  AgentLoopIterationAPI,
  UserInteractionResourceAPI,
} from '@wf-agent/sdk/api'

// SDK-Kit 导入（需要添加到 cli-app 依赖）
import { SDKKit } from '@wf-agent/sdk-kit'

// 使用示例
const graphAPI = apiFactory.createWorkflowGraphQueryAPI()
const graph = await graphAPI.getGraph(workflowId)
const analysis = await graphAPI.getAnalysis(workflowId)

const searchAPI = apiFactory.createSearchAPI()
const results = await searchAPI.search(query, { type: 'workflow' })

const kit = new SDKKit(sdk, { logging: { level: 'info' } })
const comparison = await kit.analysis().compare(executionId1, executionId2)
```
