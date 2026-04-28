# Workflow 错误处理测试用例

## 测试目标

验证各种错误类型的处理，包括配置验证错误、工作流不存在错误和执行错误。

## SDK 核心组件

- **WorkflowRegistry** (sdk/graph/services/workflow-registry.ts)
- **WorkflowValidator**
- **错误类型**: ConfigurationValidationError, WorkflowNotFoundError, ExecutionError

---

## 9.1 ConfigurationValidationError

**测试用例名称**: `workflow_error_configuration_validation`

**测试目标**: 验证配置验证错误的处理

**测试步骤**:
1. 准备配置错误的 TOML 文件
2. 尝试注册工作流
3. 验证 ConfigurationValidationError 处理

**预期 stderr 内容**:
```
错误：工作流配置验证失败
错误类型: ConfigurationValidationError
详情：
  - 字段: name, 错误: 必需字段缺失
  - 节点: process, 错误: 无效的节点类型: INVALID_TYPE

上下文:
  文件: /path/to/invalid-workflow.toml
  工作流 ID: (未定义)
```

**SDK 验证点**:
- 错误包含详细的验证失败信息
- 错误上下文完整
- 错误类型正确
- 错误可追溯

**验证点**:
- 退出码非 0
- stderr 包含详细的验证错误信息
- 错误类型明确
- 错误上下文完整

---

## 9.2 WorkflowNotFoundError

**测试用例名称**: `workflow_error_workflow_not_found`

**测试目标**: 验证工作流不存在错误的处理

**测试步骤**:
1. 尝试查询不存在的工作流
2. 验证 WorkflowNotFoundError 处理

**预期 stderr 内容**:
```
错误：工作流不存在
错误类型: WorkflowNotFoundError
详情：
  工作流 ID: nonexistent-wf
  建议检查:
    - 工作流 ID 是否正确
    - 工作流是否已注册
    - 使用 list 命令查看所有已注册的工作流
```

**SDK 验证点**:
- 错误包含工作流 ID
- 错误信息友好
- 错误类型正确
- 提供有用的建议

**验证点**:
- 退出码非 0
- stderr 包含明确的工作流 ID
- 错误信息友好且有用

---

## 9.3 ExecutionError

**测试用例名称**: `workflow_error_execution`

**测试目标**: 验证执行错误的处理

**测试步骤**:
1. 执行工作流
2. 模拟执行错误
3. 验证 ExecutionError 处理

**预期 stderr 内容**:
```
错误：工作流执行失败
错误类型: ExecutionError
详情：
  执行上下文:
    工作流 ID: wf-001
    线程 ID: thread-001
    当前节点: process
    节点类型: LLM

  错误原因: LLM API 调用失败

  建议检查:
    - LLM 配置是否正确
    - API 密钥是否有效
    - 网络连接是否正常
```

**SDK 验证点**:
- 错误包含执行上下文
- 错误可追溯
- 错误类型正确
- 提供有用的调试信息

**验证点**:
- 退出码非 0
- stderr 包含完整的执行上下文
- 错误原因明确
- 调试信息有用
