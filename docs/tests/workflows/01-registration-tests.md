# Workflow 注册测试用例

## 测试目标

验证 WorkflowRegistry.register() 的正确性，包括各种工作流类型的注册、参数替换和验证逻辑。

## SDK 核心组件

- **WorkflowRegistry** (sdk/graph/services/workflow-registry.ts)
- **ConfigManager** (apps/cli-app/src/config/config-manager.ts)
- **WorkflowValidator**

---

## 1.1 注册 STANDALONE 工作流

**测试用例名称**: `workflow_register_standalone_success`

**测试目标**: 验证注册 STANDALONE 类型工作流

**测试步骤**:
1. 准备 STANDALONE 工作流配置文件
2. 执行 CLI 命令: `modular-agent workflow register <file-path>`
3. 验证注册成功

**测试配置**:
```toml
[workflow]
id = "standalone-wf-001"
name = "Standalone Workflow"
type = "STANDALONE"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "llm"
type = "LLM"
config = { llmProfileId = "gpt-4o" }

[[nodes]]
id = "end"
type = "END"

[[edges]]
from = "start"
to = "llm"

[[edges]]
from = "llm"
to = "end"
```

**预期 stdout 内容**:
```
正在注册工作流: /path/to/standalone-wf-001.toml
工作流已注册: standalone-wf-001
```

**预期 stderr 内容**:
```
(空)
```

**SDK 验证点**:
- WorkflowRegistry.register() 正确执行
- 工作流类型为 STANDALONE
- 不包含 SUBGRAPH 节点和 EXECUTE_TRIGGERED_SUBGRAPH 触发器
- 工作流状态为 active

**验证点**:
- 退出码为 0
- stdout 包含 "工作流已注册"
- 工作流可以通过 list 命令查询到
- 工作流类型正确

---

## 1.2 注册 TRIGGERED_SUBWORKFLOW 工作流

**测试用例名称**: `workflow_register_triggered_subworkflow_success`

**测试目标**: 验证注册触发子工作流

**测试步骤**:
1. 准备 TRIGGERED_SUBWORKFLOW 工作流配置文件
2. 执行 CLI 命令: `modular-agent workflow register <file-path>`
3. 验证注册成功

**测试配置**:
```toml
[workflow]
id = "triggered-wf-001"
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

**预期 stdout 内容**:
```
正在注册工作流: /path/to/triggered-wf-001.toml
工作流已注册: triggered-wf-001
```

**预期 stderr 内容**:
```
(空)
```

**SDK 验证点**:
- 必须包含 START_FROM_TRIGGER 节点
- 必须包含 CONTINUE_FROM_TRIGGER 节点
- 不能包含 START、END、SUBGRAPH 节点
- 工作流类型为 TRIGGERED_SUBWORKFLOW

**验证点**:
- 退出码为 0
- 工作流注册成功
- 包含必需的触发器节点

---

## 1.3 注册 DEPENDENT 工作流

**测试用例名称**: `workflow_register_dependent_success`

**测试目标**: 验证注册依赖工作流

**测试步骤**:
1. 准备 DEPENDENT 工作流配置文件(包含 SUBGRAPH 节点)
2. 执行 CLI 命令: `modular-agent workflow register <file-path>`
3. 验证注册成功

**测试配置**:
```toml
[workflow]
id = "dependent-wf-001"
name = "Dependent Workflow"
type = "DEPENDENT"
version = "1.0.0"

[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "subgraph"
type = "SUBGRAPH"
config = { workflowId = "sub-wf-001" }

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
正在注册工作流: /path/to/dependent-wf-001.toml
工作流已注册: dependent-wf-001
```

**预期 stderr 内容**:
```
(空)
```

**SDK 验证点**:
- 包含 SUBGRAPH 节点或 EXECUTE_TRIGGERED_SUBGRAPH 触发器
- 工作流类型自动推断为 DEPENDENT
- 建立工作流依赖关系

**验证点**:
- 退出码为 0
- 工作流注册成功
- 依赖关系正确建立

---

## 1.4 注册工作流 - 包含触发器

**测试用例名称**: `workflow_register_with_trigger_success`

**测试目标**: 验证触发器定义的正确性

**测试步骤**:
1. 准备包含触发器的工作流配置文件
2. 执行 CLI 命令: `modular-agent workflow register <file-path>`
3. 验证触发器正确注册

**测试配置**:
```toml
[workflow]
id = "workflow-with-trigger"
name = "Workflow with Trigger"
type = "STANDALONE"
version = "1.0.0"

[[triggers]]
id = "trigger-001"
name = "On Node Completed"
enabled = true

[triggers.condition]
eventType = "NODE_COMPLETED"

[triggers.action]
type = "set_variable"

[triggers.action.parameters]
threadId = "current"
variables = { status = "completed" }

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

**预期 stdout 内容**:
```
正在注册工作流: /path/to/workflow-with-trigger.toml
工作流已注册: workflow-with-trigger
触发器已注册: trigger-001
```

**预期 stderr 内容**:
```
(空)
```

**SDK 验证点**:
- TriggerCondition.eventType 有效
- TriggerAction.type 有效
- 参数类型正确
- 触发器正确关联到工作流

**验证点**:
- 退出码为 0
- 触发器正确注册
- 触发器配置完整

---

## 1.5 注册工作流 - 参数替换

**测试用例名称**: `workflow_register_with_parameter_replacement_success`

**测试目标**: 验证 ConfigManager 的参数替换功能

**测试步骤**:
1. 准备包含参数占位符的工作流配置文件
2. 准备运行时参数 JSON
3. 执行 CLI 命令: `modular-agent workflow register <file-path> --params '{"workflow_id": "param-wf-001", "workflow_name": "Parameterized Workflow"}'`
4. 验证参数替换成功

**测试配置**:
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
id = "end"
type = "END"

[[edges]]
from = "start"
to = "end"
```

**预期 stdout 内容**:
```
正在注册工作流: /path/to/param-workflow.toml
工作流已注册: param-wf-001
```

**预期 stderr 内容**:
```
(空)
```

**SDK 验证点**:
- `{{variable}}` 占位符正确替换
- ConfigManager 正确处理参数
- 替换后的配置验证通过

**验证点**:
- 退出码为 0
- 工作流 ID 为 param-wf-001
- 工作流名称为 "Parameterized Workflow"
- 参数占位符已被替换

---

## 1.6 注册工作流 - 验证失败

**测试用例名称**: `workflow_register_validation_failure`

**测试目标**: 验证 WorkflowValidator 的验证逻辑

**测试步骤**:
1. 准备包含各种验证错误的配置文件
2. 执行 CLI 命令: `modular-agent workflow register <file-path>`
3. 验证错误处理正确

### 场景 1: 缺少必需字段

```toml
[workflow]
# 缺少 id
# 缺少 name
type = "STANDALONE"
version = "1.0.0"
```

**预期 stderr 内容**:
```
错误：工作流配置验证失败
详情：缺少必需字段: id, name
```

### 场景 2: 节点类型不存在

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

**预期 stderr 内容**:
```
错误：工作流配置验证失败
详情：无效的节点类型: INVALID_TYPE
```

### 场景 3: 节点 ID 重复

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

**预期 stderr 内容**:
```
错误：工作流配置验证失败
详情：节点 ID 重复: start
```

### 场景 4: 边引用不存在的节点

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

**预期 stderr 内容**:
```
错误：工作流配置验证失败
详情：边引用了不存在的节点: nonexistent
```

### 场景 5: START 节点不唯一

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

**预期 stderr 内容**:
```
错误：工作流配置验证失败
详情：工作流必须包含且仅包含一个 START 节点
```

### 场景 6: END 节点不唯一

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

**预期 stderr 内容**:
```
错误：工作流配置验证失败
详情：工作流必须包含且仅包含一个 END 节点
```

### 场景 7: TOML 格式错误

```toml
[workflow
# 缺少闭合括号
id = "invalid-toml-wf"
```

**预期 stderr 内容**:
```
错误：TOML 解析失败
详情：语法错误
```

**SDK 验证点**:
- 缺少必需字段时抛出 ConfigurationValidationError
- 节点类型无效时抛出验证错误
- 边定义无效时抛出验证错误
- 错误信息清晰明确

**验证点**:
- 退出码非 0
- stderr 包含详细的验证错误信息
- 没有工作流被注册
