# CLI-App 功能补充快速参考指南

## 📋 核心补充功能速览

### 立即可实现（1-2天）

| 命令 | 描述 | 来源API | 实现难度 |
|-----|------|--------|---------|
| `storage diagnose` | 存储系统健康检查 | StorageDiagnosticsAPI | 🟢 简单 |
| `storage health` | 存储状态概览 | StorageDiagnosticsAPI | 🟢 简单 |
| `search <query>` | 跨资源全文搜索 | SearchAPI | 🟢 简单 |
| `execution list --filter ...` | 高级过滤执行 | QueryAPI | 🟢 简单 |

### 高价值功能（3-4天）

| 命令 | 描述 | 来源API | 实现难度 |
|-----|------|--------|---------|
| `workflow graph show <id>` | 显示工作流图结构 | WorkflowGraphQueryAPI | 🟡 中 |
| `workflow graph analyze <id>` | 拓扑和依赖分析 | WorkflowGraphQueryAPI | 🟡 中 |
| `execution compare <id1> <id2>` | 执行性能对比 | Analysis API (sdk-kit) | 🟡 中 |
| `execution progress <id>` | 实时进度跟踪 | Analysis API (sdk-kit) | 🟡 中 |

### 关键缺失（4-5天）

| 命令 | 描述 | 来源API | 实现难度 |
|-----|------|--------|---------|
| `agent-loop iteration show <id>` | 代理迭代详情 | AgentLoopIterationAPI | 🟡 中 |
| `workflow version list <id>` | 工作流版本历史 | ResourceAPI (sdk-kit) | 🟡 中 |
| `execution interaction list <id>` | 用户交互追踪 | UserInteractionResourceAPI | 🟡 中 |

---

## 🎯 优先级排序方案

### 方案 A: 快速见效（推荐前2周）
```
第1天:  storage diagnose (快速赢)
第2天:  search <query>
第3-4天: workflow graph show/analyze
第5-6天: execution list --filter
第7-8天: execution compare
```

### 方案 B: 完整功能（8周全完成）
```
第1-2周: 基础查询功能 (storage + search + graph)
第3周:   执行管理 (compare + progress + filter)
第4周:   代理分析 (iteration analysis)
第5周:   版本管理
第6-7周: 数据追踪 (interaction + events)
第8周:   启动器系统和优化
```

---

## 📊 新增命令完整列表

### 工作流命令组扩展

```bash
# 图查询命令 (新增)
workflow graph show <workflow-id>
workflow graph show <workflow-id> --format json|text|dot
workflow graph show <workflow-id> --include-metadata

workflow graph analyze <workflow-id>
workflow graph analyze <workflow-id> --check-cycles
workflow graph analyze <workflow-id> --find-critical-path

workflow graph nodes <workflow-id>
workflow graph nodes <workflow-id> --type LLM|CONDITION|FORK
workflow graph nodes <workflow-id> --depth 2

workflow graph edges <workflow-id>
workflow graph edges <workflow-id> --from <node-id>
workflow graph edges <workflow-id> --to <node-id>

# 版本控制命令 (新增)
workflow version list <workflow-id>
workflow version show <workflow-id> <version>
workflow version rollback <workflow-id> <version>
workflow version diff <workflow-id> <v1> <v2>
workflow version compare <workflow-id> <v1> <v2> --detailed
```

### 执行命令组扩展

```bash
# 高级查询命令 (增强)
execution list --filter "status=completed,duration>5000"
execution list --filter "status=failed" --limit 10
execution list --sort duration --order desc
execution list --date-range "2024-01-01 to 2024-01-31"
execution list --workflow <workflow-id> --limit 20

# 对比分析命令 (新增)
execution compare <id1> <id2>
execution compare <id1> <id2> --metrics
execution compare <id1> <id2> --timeline
execution compare <id1> <id2> <id3> <id4> --batch

# 进度跟踪命令 (新增)
execution progress <id>
execution progress <id> --watch
execution progress <id> --json
execution progress <id> --include-nodes

# 交互追踪命令 (新增)
execution interaction list <execution-id>
execution interaction show <execution-id> <interaction-id>
execution interaction list <execution-id> --type tool_approval
```

### 代理循环命令组扩展

```bash
# 迭代分析命令 (新增)
agent-loop iteration show <agent-loop-id>
agent-loop iteration show <agent-loop-id> <iteration-index>
agent-loop iteration analyze <agent-loop-id>
agent-loop iteration list <agent-loop-id> --limit 20

agent-loop iteration decision <agent-loop-id> <iteration-index>
agent-loop iteration decision <agent-loop-id> <iteration-index> --detailed

agent-loop iteration metrics <agent-loop-id>
agent-loop iteration metrics <agent-loop-id> <iteration-index>

# 交互追踪命令 (新增)
agent-loop interaction list <agent-loop-id>
agent-loop interaction show <agent-loop-id> <interaction-id>
```

### 搜索和诊断命令 (新增)

```bash
# 全局搜索
search <query>
search <query> --type workflow
search <query> --type execution
search <query> --type task
search <query> --type checkpoint
search <query> --type event
search <query> --type agent-loop
search <query> --limit 20

# 存储诊断
storage diagnose
storage diagnose --detailed
storage diagnose --export json

storage health
storage health --adapters
storage health --item-count

storage stats
storage stats --by-type
storage stats --export csv|json
```

### 启动器系统 (新增，低优先级)

```bash
# 启动器管理
workflow starter list
workflow starter show <starter-id>
workflow starter create <name> --starter GoalReviewStarter
workflow starter create <name> --starter <starter-id> --config config.json
```

---

## 🔧 实现清单

### Phase 1: 基础查询 (1-2周)

#### 命令
- [ ] `storage diagnose`
- [ ] `storage health`
- [ ] `search <query>`
- [ ] `search <query> --type`

#### 适配器
- [ ] StorageAdapter (new)
- [ ] SearchAdapter (new)

#### 测试
- [ ] 存储诊断单元测试
- [ ] 搜索功能集成测试

---

### Phase 2: 图和执行分析 (2-3周)

#### 命令
- [ ] `workflow graph show <id>`
- [ ] `workflow graph analyze <id>`
- [ ] `workflow graph nodes <id>`
- [ ] `workflow graph edges <id>`
- [ ] `execution list --filter`
- [ ] `execution list --sort`

#### 适配器
- [ ] WorkflowGraphAdapter (new)
- [ ] ExecutionQueryAdapter (enhance)

#### 测试
- [ ] 图查询单元测试
- [ ] 执行过滤集成测试

---

### Phase 3: 性能分析 (3-4周)

#### 命令
- [ ] `execution compare <id1> <id2>`
- [ ] `execution progress <id>`
- [ ] `agent-loop iteration show <id>`

#### 依赖
- [ ] SDK-Kit 集成到 cli-app
- [ ] Analysis API 导入

#### 适配器
- [ ] ExecutionComparisonAdapter (new)
- [ ] ProgressTrackingAdapter (new)
- [ ] IterationAnalysisAdapter (new)

#### 测试
- [ ] 对比分析单元测试
- [ ] 迭代分析集成测试

---

### Phase 4: 版本和追踪 (2-3周)

#### 命令
- [ ] `workflow version list <id>`
- [ ] `workflow version rollback <id> <version>`
- [ ] `execution interaction list <id>`
- [ ] `agent-loop interaction list <id>`

#### 适配器
- [ ] VersionManagementAdapter (new)
- [ ] UserInteractionAdapter (new)

#### 测试
- [ ] 版本管理单元测试
- [ ] 交互追踪集成测试

---

### Phase 5: 启动器系统 (1-2周)

#### 命令
- [ ] `workflow starter list`
- [ ] `workflow starter create <name>`

#### 适配器
- [ ] StarterAdapter (new)

#### 测试
- [ ] 启动器集成测试

---

## 💻 代码示例

### 工作流图查询实现

```typescript
// commands/workflow-graph/show.ts
import { WorkflowGraphQueryAPI } from '@wf-agent/sdk/api'
import { Command } from 'commander'

export const createWorkflowGraphShowCommand = (apiFactory) => {
  return new Command('show')
    .argument('<workflow-id>', 'Workflow ID')
    .option('--format <type>', 'Output format', 'text')
    .action(async (workflowId, options) => {
      const graphAPI = apiFactory.createWorkflowGraphQueryAPI()
      
      const graph = await graphAPI.getGraph(workflowId)
      const summary = await graphAPI.getGraphSummary(workflowId)
      
      if (options.format === 'json') {
        console.log(JSON.stringify({ graph, summary }, null, 2))
      } else {
        // 格式化输出
        console.log(`Workflow: ${workflowId}`)
        console.log(`Nodes: ${summary.nodeCount}`)
        console.log(`Edges: ${summary.edgeCount}`)
        // ... more output
      }
    })
}
```

### 执行对比实现

```typescript
// commands/workflow-execution/compare.ts
import { SDKKit } from '@wf-agent/sdk-kit'
import { Command } from 'commander'

export const createExecutionCompareCommand = (sdk) => {
  return new Command('compare')
    .argument('<id1>', 'First execution ID')
    .argument('<id2>', 'Second execution ID')
    .option('--metrics', 'Show detailed metrics')
    .action(async (id1, id2, options) => {
      const kit = new SDKKit(sdk)
      
      const comparison = await kit.analysis().compare(id1, id2)
      
      console.log('Execution Comparison:')
      console.log(`Duration: ${comparison.duration.delta}ms`)
      console.log(`Success Rate: ${comparison.success_rate}%`)
      
      if (options.metrics) {
        console.log('Detailed Metrics:')
        console.log(JSON.stringify(comparison.metrics, null, 2))
      }
    })
}
```

### 搜索实现

```typescript
// commands/search/index.ts
import { SearchAPI } from '@wf-agent/sdk/api'
import { Command } from 'commander'

export const createSearchCommand = (apiFactory) => {
  return new Command('search')
    .argument('<query>', 'Search query')
    .option('--type <type>', 'Filter by type')
    .option('--limit <number>', 'Result limit', '20')
    .action(async (query, options) => {
      const searchAPI = apiFactory.createSearchAPI()
      
      const results = await searchAPI.search(query, {
        type: options.type,
        limit: parseInt(options.limit)
      })
      
      // 按类型分组显示结果
      Object.entries(results).forEach(([type, items]) => {
        console.log(`\n${type} (${items.length}):`)
        items.forEach(item => {
          console.log(`  - ${item.name} (score: ${item.score})`)
        })
      })
    })
}
```

---

## 📈 预期影响

### 用户体验提升
- ✅ 更强大的查询和过滤能力
- ✅ 执行性能对比分析
- ✅ 工作流结构可视化
- ✅ 代理迭代详细分析
- ✅ 跨资源统一搜索

### 系统可观测性提升
- ✅ 存储系统诊断
- ✅ 事件历史追踪
- ✅ 执行进度实时跟踪
- ✅ 性能基线建立

### 开发和调试
- ✅ 工作流图拓扑检查
- ✅ 循环检测和路径分析
- ✅ 代理决策追踪
- ✅ 版本回滚能力

---

## 🔄 与SDK集成要点

### 需要添加的依赖

```json
{
  "dependencies": {
    "@wf-agent/sdk-kit": "workspace:*"
  }
}
```

### 必要的SDK API 导入

```typescript
// 新增 import
import {
  WorkflowGraphQueryAPI,
  SearchAPI,
  StorageDiagnosticsAPI,
  AgentLoopIterationAPI,
  UserInteractionResourceAPI,
  AgentUserInteractionResourceAPI,
} from '@wf-agent/sdk/api'

import { SDKKit } from '@wf-agent/sdk-kit'
```

### APIFactory 扩展

```typescript
// container.ts 中添加
export function setupAPIs(apiFactory: APIFactory) {
  // 新API 实例
  const graphAPI = apiFactory.createWorkflowGraphQueryAPI()
  const searchAPI = apiFactory.createSearchAPI()
  const diagnosticsAPI = apiFactory.createStorageDiagnosticsAPI()
  const iterationAPI = apiFactory.createAgentLoopIterationAPI()
  
  // 注册到容器
  container.set('graphAPI', graphAPI)
  container.set('searchAPI', searchAPI)
  container.set('diagnosticsAPI', diagnosticsAPI)
  container.set('iterationAPI', iterationAPI)
}
```

---

## 📝 文档和示例

### 新增帮助文本

```bash
# 工作流图命令帮助
wf-agent workflow graph --help
  工作流结构分析和可视化
  
  使用:
    wf-agent workflow graph show <workflow-id>
    wf-agent workflow graph analyze <workflow-id>
    
  示例:
    # 显示工作流图
    wf-agent workflow graph show my-workflow
    
    # 分析拓扑和依赖
    wf-agent workflow graph analyze my-workflow --check-cycles
    
    # 以DOT格式导出
    wf-agent workflow graph show my-workflow --format dot > graph.dot

# 搜索命令帮助
wf-agent search --help
  跨资源全文搜索
  
  使用:
    wf-agent search <query> [options]
    
  示例:
    wf-agent search "test"
    wf-agent search "user-*" --type workflow
    wf-agent search "failed" --type execution --limit 10
```

---

## ✅ 验收标准

每个新功能交付时应满足:

- [ ] 命令实现完成
- [ ] 与SDK/API正确集成
- [ ] 单元测试覆盖率 >80%
- [ ] 集成测试通过
- [ ] 帮助文本完善
- [ ] 使用示例完整
- [ ] 错误处理规范
- [ ] 性能合理（<5s响应）

---

## 🚀 开始行动

### 建议第一步

1. **评估 SDK-Kit 集成成本** (1天)
   - 查看 cli-app package.json
   - 确认依赖关系
   - 验证编译兼容性

2. **实现存储诊断** (1天)
   - 最简单的新功能
   - 可快速验证框架
   - 早期可见成果

3. **实现搜索功能** (2天)
   - 应用范围广
   - 用户价值高
   - 为下一步做准备

4. **实现工作流图查询** (3-4天)
   - 核心分析功能
   - 需要完整测试
   - 为启动后续功能

后续按优先级继续实现其他功能。
