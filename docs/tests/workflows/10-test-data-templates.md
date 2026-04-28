# Workflow 测试数据模板

## 测试目标

提供各种类型的工作流配置模板，用于测试用例。

---

## 最小 STANDALONE 工作流

```toml
[workflow]
id = "minimal-wf"
name = "Minimal Workflow"
type = "STANDALONE"
version = "1.0.0"

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

---

## 完整 STANDALONE 工作流

```toml
[workflow]
id = "complete-wf"
name = "Complete Workflow"
type = "STANDALONE"
description = "Complete workflow with all node types"
version = "1.0.0"

[workflow.metadata]
author = "Test"
tags = ["test", "complete"]

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "variable"
type = "VARIABLE"
config = { operations = [] }

[[nodes]]
id = "llm"
type = "LLM"
config = { llmProfileId = "gpt-4o" }

[[nodes]]
id = "script"
type = "SCRIPT"
config = { scriptName = "test-script" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "variable"

[[edges]]
from = "variable"
to = "llm"

[[edges]]
from = "llm"
to = "script"

[[edges]]
from = "script"
to = "end"
```

---

## 带触发器的工作流

```toml
[workflow]
id = "trigger-wf"
name = "Workflow with Triggers"
type = "STANDALONE"
version = "1.0.0"

[[triggers]]
id = "on-completion"
name = "On Thread Completed"
enabled = true

[triggers.condition]
eventType = "THREAD_COMPLETED"

[triggers.action]
type = "execute_script"

[triggers.action.parameters]
scriptName = "cleanup"
parameters = { threadId = "${thread.id}" }

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

---

## TRIGGERED_SUBWORKFLOW 工作流

```toml
[workflow]
id = "triggered-subwf"
name = "Triggered Subworkflow"
type = "TRIGGERED_SUBWORKFLOW"
version = "1.0.0"

[[nodes]]
id = "start_from_trigger"
type = "START_FROM_TRIGGER"

[[nodes]]
id = "process"
type = "LLM"
config = { llmProfileId = "gpt-4o" }

[[nodes]]
id = "continue_from_trigger"
type = "CONTINUE_FROM_TRIGGER"

[[edges]]
from = "start_from_trigger"
to = "process"

[[edges]]
from = "process"
to = "continue_from_trigger"
```

---

## DEPENDENT 工作流

```toml
[workflow]
id = "dependent-wf"
name = "Dependent Workflow"
type = "DEPENDENT"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "subgraph"
type = "SUBGRAPH"
config = { workflowId = "sub-workflow-id" }

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

---

## 参数化工作流

```toml
[workflow]
id = "{{workflow_id}}"
name = "{{workflow_name}}"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "process"
type = "LLM"
config = { llmProfileId = "{{llm_profile_id}}" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "process"

[[edges]]
from = "process"
to = "end"
```

**参数**:
```json
{
  "workflow_id": "param-wf-001",
  "workflow_name": "Parameterized Workflow",
  "llm_profile_id": "gpt-4o"
}
```

---

## 无效工作流 - 缺少必需字段

```toml
[workflow]
# 缺少 id
# 缺少 name
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"
```

---

## 无效工作流 - 节点类型不存在

```toml
[workflow]
id = "invalid-node-type-wf"
name = "Invalid Node Type Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "INVALID_TYPE"
```

---

## 无效工作流 - 节点 ID 重复

```toml
[workflow]
id = "duplicate-node-wf"
name = "Duplicate Node Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "start"
type = "END"
```

---

## 无效工作流 - 边引用不存在的节点

```toml
[workflow]
id = "invalid-edge-wf"
name = "Invalid Edge Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[edges]]
from = "start"
to = "nonexistent"
```

---

## 无效工作流 - 多个 START 节点

```toml
[workflow]
id = "multiple-start-wf"
name = "Multiple Start Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start1"
type = "START"

[[nodes]]
id = "start2"
type = "START"
```

---

## 无效工作流 - 多个 END 节点

```toml
[workflow]
id = "multiple-end-wf"
name = "Multiple End Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "end1"
type = "END"

[[nodes]]
id = "end2"
type = "END"
```

---

## 循环引用工作流 A

```toml
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

---

## 循环引用工作流 B

```toml
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

---

## 父工作流（包含 SUBGRAPH）

```toml
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

---

## 子工作流（被引用）

```toml
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
