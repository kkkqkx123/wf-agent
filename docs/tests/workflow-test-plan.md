# Workflow 模块端到端测试设计

## 测试目标

通过 CLI 命令测试 SDK WorkflowRegistry 和 WorkflowRegistryAPI 的正确性。CLI 作为测试驱动,验证 SDK 核心功能的端到端行为。

## SDK 核心功能

### WorkflowRegistry (sdk/graph/services/workflow-registry.ts)
- 工作流注册、查询、删除
- 工作流版本管理
- 工作流关系管理
- 工作流引用检查

### WorkflowRegistryAPI (sdk/api/graph/resources/workflows/workflow-registry-api.ts)
- 统一的 CRUD 操作接口
- 工作流过滤和查询
- 版本化更新

### ConfigManager (apps/cli-app/src/config/config-manager.ts)
- 从文件加载工作流配置
- 从目录批量加载
- 参数模板替换

## SDK 实际类型定义

### WorkflowType
- `TRIGGERED_SUBWORKFLOW` - 触发子工作流
- `STANDALONE` - 独立工作流
- `DEPENDENT` - 依赖工作流

### NodeType (15 种)
- `START` - 开始节点
- `END` - 结束节点
- `VARIABLE` - 变量操作节点
- `FORK` - 分支节点
- `JOIN` - 连接节点
- `SUBGRAPH` - 子图节点
- `SCRIPT` - 脚本节点
- `LLM` - LLM 调用节点
- `ADD_TOOL` - 工具添加节点
- `USER_INTERACTION` - 用户交互节点
- `ROUTE` - 路由节点
- `CONTEXT_PROCESSOR` - 上下文处理器节点
- `LOOP_START` - 循环开始节点
- `LOOP_END` - 循环结束节点
- `AGENT_LOOP` - Agent 自循环节点
- `START_FROM_TRIGGER` - 触发开始节点
- `CONTINUE_FROM_TRIGGER` - 触发继续节点

### TriggerActionType (11 种)
- `start_dynamic_child` - 启动动态子线程
- `stop_thread` - 停止线程
- `pause_thread` - 暂停线程
- `resume_thread` - 恢复线程
- `skip_node` - 跳过节点
- `set_variable` - 设置变量
- `send_notification` - 发送通知
- `custom` - 自定义动作
- `apply_message_operation` - 应用消息操作
- `execute_triggered_subgraph` - 执行触发子工作流
- `execute_script` - 执行脚本

### EventType (60+ 种)
包括线程事件、节点事件、工具事件、检查点事件、子图事件、交互事件等

## 测试用例设计

### 1. WorkflowRegistry.register() - 工作流注册

#### 1.1 注册 STANDALONE 工作流
**测试目标**: 验证注册 STANDALONE 类型工作流
**SDK 验证点**:
- WorkflowRegistry.register() 正确执行
- 工作流类型为 STANDALONE
- 不包含 SUBGRAPH 节点和 EXECUTE_TRIGGERED_SUBGRAPH 触发器

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

---

#### 1.2 注册 TRIGGERED_SUBWORKFLOW 工作流
**测试目标**: 验证注册触发子工作流
**SDK 验证点**:
- 必须包含 START_FROM_TRIGGER 节点
- 必须包含 CONTINUE_FROM_TRIGGER 节点
- 不能包含 START、END、SUBGRAPH 节点

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

---

#### 1.3 注册 DEPENDENT 工作流
**测试目标**: 验证注册依赖工作流
**SDK 验证点**:
- 包含 SUBGRAPH 节点或 EXECUTE_TRIGGERED_SUBGRAPH 触发器

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

---

#### 1.4 注册工作流 - 包含触发器
**测试目标**: 验证触发器定义的正确性
**SDK 验证点**:
- TriggerCondition.eventType 有效
- TriggerAction.type 有效
- 参数类型正确

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

---

#### 1.5 注册工作流 - 参数替换
**测试目标**: 验证 ConfigManager 的参数替换功能
**SDK 验证点**:
- `{{variable}}` 占位符正确替换

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

**参数**: `{"workflow_id": "param-wf-001", "workflow_name": "Parameterized Workflow"}`

---

#### 1.6 注册工作流 - 验证失败
**测试目标**: 验证 WorkflowValidator 的验证逻辑
**SDK 验证点**:
- 缺少必需字段时抛出 ConfigurationValidationError
- 节点类型无效时抛出验证错误
- 边定义无效时抛出验证错误

**测试场景**:
- 缺少 id/name/type
- 节点类型不存在
- 节点 ID 重复
- 边引用不存在的节点
- START 节点不唯一
- END 节点不唯一
- TOML 格式错误

---

### 2. WorkflowRegistry.batchRegister() - 批量注册

#### 2.1 批量注册 - 全部成功
**测试目标**: 验证批量注册的正确性
**SDK 验证点**:
- 所有工作流正确注册
- 返回成功列表

---

#### 2.2 批量注册 - 部分失败
**测试目标**: 验证批量注册的错误处理
**SDK 验证点**:
- 有效工作流成功注册
- 无效工作流跳过
- 返回失败列表及原因

---

#### 2.3 批量注册 - 递归加载
**测试目标**: 验证 ConfigManager 的递归加载
**SDK 验证点**:
- 子目录文件被正确加载

---

#### 2.4 批量注册 - 文件过滤
**测试目标**: 验证文件模式过滤功能
**SDK 验证点**:
- 只有匹配文件被加载

---

### 3. WorkflowRegistry.get() / list() - 查询

#### 3.1 查询工作流 - 存在
**测试目标**: 验证 WorkflowRegistry.get() 的正确性
**SDK 验证点**:
- 返回完整的工作流定义
- 定义与注册时一致

---

#### 3.2 查询工作流 - 不存在
**测试目标**: 验证 WorkflowNotFoundError 错误
**SDK 验证点**:
- 抛出 WorkflowNotFoundError
- 错误信息包含工作流 ID

---

#### 3.3 列出所有工作流
**测试目标**: 验证 WorkflowRegistry.list() 的正确性
**SDK 验证点**:
- 返回所有已注册工作流的摘要
- 摘要信息完整

---

### 4. WorkflowRegistry.unregister() - 删除

#### 4.1 删除工作流 - 存在
**测试目标**: 验证 WorkflowRegistry.unregister() 的正确性
**SDK 验证点**:
- 工作流从注册表移除
- 无法再查询
- 触发删除事件

---

#### 4.2 删除工作流 - 不存在
**测试目标**: 验证删除不存在工作流的错误处理
**SDK 验证点**:
- 抛出 WorkflowNotFoundError

---

#### 4.3 删除工作流 - 有依赖
**测试目标**: 验证删除有依赖关系的工作流
**SDK 验证点**:
- 检查工作流引用关系
- 有依赖时拒绝删除或级联删除

---

### 5. WorkflowRegistryAPI - CRUD 操作

#### 5.1 API.create()
**测试目标**: 验证 API 层的创建操作
**SDK 验证点**:
- 调用 WorkflowRegistry.register()
- 返回创建结果

---

#### 5.2 API.getAll()
**测试目标**: 验证 API 层的查询操作
**SDK 验证点**:
- 调用 WorkflowRegistry.list()
- 返回工作流数组

---

#### 5.3 API.update() - 版本化更新
**测试目标**: 验证 WorkflowRegistryAPI 的版本化更新
**SDK 验证点**:
- 创建新版本工作流
- 保留或删除原版本
- 版本号正确递增

---

### 6. 工作流预处理

#### 6.1 工作流预处理 - SUBGRAPH 展开
**测试目标**: 验证 WorkflowProcessor 的 SUBGRAPH 节点展开
**SDK 验证点**:
- SUBGRAPH 节点正确展开为子工作流的节点和边
- 边正确生成

---

#### 6.2 工作流预处理 - 引用解析
**测试目标**: 验证工作流引用的解析
**SDK 验证点**:
- 子工作流引用正确解析
- 触发器引用正确解析

---

### 7. 工作流引用检查

#### 7.1 引用检查 - 有效引用
**测试目标**: 验证 checkWorkflowReferences() 的正确性
**SDK 验证点**:
- 有效引用通过检查
- 返回引用关系

---

#### 7.2 引用检查 - 无效引用
**测试目标**: 验证引用检查的错误处理
**SDK 验证点**:
- 引用不存在的工作流时报错
- 循环引用检测

---

### 8. 工作流关系管理

#### 8.1 父子关系
**测试目标**: 验证工作流父子关系的建立
**SDK 验证点**:
- 关系正确建立
- 可查询子工作流列表

---

#### 8.2 依赖关系
**测试目标**: 验证工作流依赖关系的管理
**SDK 验证点**:
- 依赖关系正确记录
- 可查询依赖列表

---

### 9. 错误处理

#### 9.1 ConfigurationValidationError
**测试目标**: 验证配置验证错误的处理
**SDK 验证点**:
- 错误包含详细的验证失败信息
- 错误上下文完整

---

#### 9.2 WorkflowNotFoundError
**测试目标**: 验证工作流不存在错误的处理
**SDK 验证点**:
- 错误包含工作流 ID
- 错误信息友好

---

#### 9.3 ExecutionError
**测试目标**: 验证执行错误的处理
**SDK 验证点**:
- 错误包含执行上下文
- 错误可追溯

---

## 测试数据模板

### 最小 STANDALONE 工作流
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

### 完整 STANDALONE 工作流
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

### 带触发器的工作流
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

### TRIGGERED_SUBWORKFLOW 工作流
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

### DEPENDENT 工作流
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

## 测试执行策略

### 测试隔离
- 每个测试用例使用独立的工作流 ID
- 测试后清理注册的工作流
- 不依赖测试执行顺序

### 测试层次
1. **单元测试**: 直接测试 SDK 类和方法
2. **集成测试**: 测试 SDK 组件间的交互
3. **端到端测试**: 通过 CLI 测试完整的 SDK 功能流程

### Mock 策略
- 单元测试: Mock 依赖组件
- 集成测试: 使用真实组件
- 端到端测试: 使用完整的 SDK 实例

## 测试覆盖目标

### 功能覆盖
- [x] 工作流注册 (STANDALONE, TRIGGERED_SUBWORKFLOW, DEPENDENT)
- [x] 工作流查询
- [x] 工作流删除
- [x] 批量操作
- [x] 参数替换
- [x] 配置验证
- [x] 预处理 (SUBGRAPH 展开)
- [x] 引用检查
- [x] 关系管理
- [x] 版本管理
- [x] 触发器定义

### 错误覆盖
- [x] ConfigurationValidationError
- [x] WorkflowNotFoundError
- [x] ExecutionError
- [x] 文件不存在
- [x] 格式错误
- [x] 验证失败

### 边界覆盖
- [x] 空注册表
- [x] 最小定义
- [x] 完整定义
- [x] 深层嵌套
- [x] 循环引用
- [x] 大规模批量

## 测试报告

测试完成后生成:
1. 测试通过率统计
2. SDK 功能覆盖报告
3. 错误处理覆盖报告
4. 性能指标(注册耗时、查询耗时等)
