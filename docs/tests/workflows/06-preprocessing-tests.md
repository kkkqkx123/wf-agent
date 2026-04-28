# Workflow 预处理测试用例

## 测试目标

验证工作流预处理功能，包括 SUBGRAPH 节点展开和引用解析。

## SDK 核心组件

- **WorkflowProcessor** (sdk/graph/services/workflow-processor.ts)
- **WorkflowRegistry** (sdk/graph/services/workflow-registry.ts)

---

## 6.1 工作流预处理 - SUBGRAPH 展开

**测试用例名称**: `workflow_preprocessing_subgraph_expansion`

**测试目标**: 验证 WorkflowProcessor 的 SUBGRAPH 节点展开

**测试步骤**:
1. 注册一个包含 SUBGRAPH 节点的工作流
2. 注册被引用的子工作流
3. 执行工作流预处理
4. 验证 SUBGRAPH 节点被正确展开

**测试配置**:
```toml
# parent-workflow.toml
[workflow]
id = "parent-wf"
name = "Parent Workflow"
type = "DEPENDENT"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "subgraph"
type = "SUBGRAPH"
config = { workflowId = "child-wf" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "subgraph"

[[edges]]
from = "subgraph"
to = "end"
```

```toml
# child-workflow.toml
[workflow]
id = "child-wf"
name = "Child Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "child-start"
type = "START"

[[nodes]]
id = "child-process"
type = "LLM"
config = { llmProfileId = "gpt-4o" }

[[nodes]]
id = "child-end"
type = "END"

[[edges]]
from = "child-start"
to = "child-process"

[[edges]]
from = "child-process"
to = "child-end"
```

**SDK 验证点**:
- SUBGRAPH 节点正确展开为子工作流的节点和边
- 边正确生成
- 节点 ID 正确处理(避免冲突)
- 预处理后的图结构正确

**验证点**:
- SUBGRAPH 节点被展开
- 子工作流的节点和边被正确插入
- 边连接正确
- 没有节点 ID 冲突

---

## 6.2 工作流预处理 - 引用解析

**测试用例名称**: `workflow_preprocessing_reference_resolution`

**测试目标**: 验证工作流引用的解析

**测试步骤**:
1. 注册包含触发器的工作流
2. 触发器引用其他工作流
3. 执行工作流预处理
4. 验证引用正确解析

**测试配置**:
```toml
[workflow]
id = "main-wf"
name = "Main Workflow"
type = "STANDALONE"
version = "1.0.0"

[[triggers]]
id = "trigger-001"
name = "Execute Subworkflow"
enabled = true

[triggers.condition]
eventType = "NODE_COMPLETED"

[triggers.action]
type = "execute_triggered_subgraph"

[triggers.action.parameters]
workflowId = "sub-wf-001"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "end"
```

**SDK 验证点**:
- 子工作流引用正确解析
- 触发器引用正确解析
- 引用的工作流存在
- 引用关系正确建立

**验证点**:
- 引用被正确解析
- 引用的工作流存在
- 没有循环引用
- 引用关系完整记录
