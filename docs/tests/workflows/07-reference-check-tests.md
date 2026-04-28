# Workflow 引用检查测试用例

## 测试目标

验证工作流引用检查功能，包括有效引用检查、无效引用检测和循环引用检测。

## SDK 核心组件

- **WorkflowRegistry** (sdk/graph/services/workflow-registry.ts)
- **WorkflowValidator**

---

## 7.1 引用检查 - 有效引用

**测试用例名称**: `workflow_reference_check_valid`

**测试目标**: 验证 checkWorkflowReferences() 的正确性

**测试步骤**:
1. 注册包含引用的工作流
2. 执行引用检查
3. 验证引用检查通过

**预期 stdout 内容**:
```
工作流引用检查通过
引用关系:
  - parent-wf -> child-wf (SUBGRAPH)
  - main-wf -> sub-wf-001 (TRIGGER)
```

**预期 stderr 内容**:
```
(空)
```

**SDK 验证点**:
- 有效引用通过检查
- 返回引用关系
- 引用类型正确识别
- 引用路径正确记录

**验证点**:
- 退出码为 0
- 所有引用有效
- 引用关系完整列出

---

## 7.2 引用检查 - 无效引用

**测试用例名称**: `workflow_reference_check_invalid`

**测试目标**: 验证引用检查的错误处理

**测试步骤**:
1. 注册包含无效引用的工作流
2. 执行引用检查
3. 验证错误处理

**测试配置**:
```toml
[workflow]
id = "invalid-ref-wf"
name = "Invalid Reference Workflow"
type = "DEPENDENT"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "subgraph"
type = "SUBGRAPH"
config = { workflowId = "nonexistent-wf" }

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

**预期 stdout 内容**:
```
(空)
```

**预期 stderr 内容**:
```
错误：工作流引用检查失败
详情：
  - invalid-ref-wf 引用了不存在的工作流: nonexistent-wf (SUBGRAPH)
```

**SDK 验证点**:
- 引用不存在的工作流时报错
- 错误信息包含引用详情
- 检查所有类型的引用

**验证点**:
- 退出码非 0
- stderr 包含详细的引用错误信息
- 所有无效引用都被列出

---

## 7.3 引用检查 - 循环引用

**测试用例名称**: `workflow_reference_check_circular`

**测试目标**: 验证循环引用检测

**测试步骤**:
1. 注册相互引用的工作流
2. 执行引用检查
3. 验证循环引用检测

**测试配置**:
```toml
# workflow-a.toml
[workflow]
id = "workflow-a"
name = "Workflow A"
type = "DEPENDENT"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "subgraph"
type = "SUBGRAPH"
config = { workflowId = "workflow-b" }

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
# workflow-b.toml
[workflow]
id = "workflow-b"
name = "Workflow B"
type = "DEPENDENT"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "subgraph"
type = "SUBGRAPH"
config = { workflowId = "workflow-a" }

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

**预期 stdout 内容**:
```
(空)
```

**预期 stderr 内容**:
```
错误：检测到循环引用
详情：
  - workflow-a -> workflow-b -> workflow-a
```

**SDK 验证点**:
- 循环引用检测正确
- 循环路径正确识别
- 错误信息清晰

**验证点**:
- 退出码非 0
- stderr 包含循环引用路径
- 所有循环引用都被检测到
