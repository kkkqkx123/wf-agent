# Workflow 查询测试用例

## 测试目标

验证 WorkflowRegistry.get() 和 WorkflowRegistry.list() 的正确性，包括单个工作流查询和批量列表查询。

## SDK 核心组件

- **WorkflowRegistry** (sdk/graph/services/workflow-registry.ts)
- **WorkflowRegistryAPI** (sdk/api/graph/resources/workflows/workflow-registry-api.ts)

---

## 3.1 查询工作流 - 存在

**测试用例名称**: `workflow_get_existing_success`

**测试目标**: 验证 WorkflowRegistry.get() 的正确性

**测试步骤**:
1. 预先注册一个工作流
2. 执行 CLI 命令: `modular-agent workflow show wf-001`
3. 验证详情输出正确

**预期 stdout 内容**:
```
工作流详情
----------

ID: wf-001
名称: Test Workflow
类型: STANDALONE
版本: 1.0.0
状态: active
描述: Test workflow description
创建时间: 2024-01-01T12:00:00.000Z
更新时间: 2024-01-01T12:00:00.000Z

节点 (3):
  [1] ID: start
      类型: START

  [2] ID: process
      类型: LLM
      配置: { "llmProfileId": "gpt-4o" }

  [3] ID: end
      类型: END

边 (2):
  [1] start -> process
  [2] process -> end

触发器: (无)
```

**预期 stderr 内容**:
```
(空)
```

**SDK 验证点**:
- 返回完整的工作流定义
- 定义与注册时一致
- 所有配置信息完整

**验证点**:
- 退出码为 0
- stdout 包含工作流的完整信息
- 输出格式清晰易读

---

## 3.2 查询工作流 - 不存在

**测试用例名称**: `workflow_get_not_found`

**测试目标**: 验证 WorkflowNotFoundError 错误

**测试步骤**:
1. 执行 CLI 命令: `modular-agent workflow show nonexistent-id`
2. 验证错误处理正确

**预期 stdout 内容**:
```
(空或包含错误摘要)
```

**预期 stderr 内容**:
```
错误：工作流不存在: nonexistent-id
```

**SDK 验证点**:
- 抛出 WorkflowNotFoundError
- 错误信息包含工作流 ID
- 错误类型正确

**验证点**:
- 退出码非 0
- stderr 包含明确的错误信息
- 错误信息包含工作流 ID

---

## 3.3 列出所有工作流

**测试用例名称**: `workflow_list_all_success`

**测试目标**: 验证 WorkflowRegistry.list() 的正确性

**测试步骤**:
1. 预先注册多个工作流
2. 执行 CLI 命令: `modular-agent workflow list`
3. 验证列表输出正确

**预期 stdout 内容**:
```
ID                    名称                    类型                      状态
---------------------------------------------------------------------------------
standalone-wf-001     Standalone Workflow     STANDALONE               active
triggered-wf-001      Triggered Subworkflow   TRIGGERED_SUBWORKFLOW    active
dependent-wf-001      Dependent Workflow      DEPENDENT                active
```

**预期 stderr 内容**:
```
(空)
```

**SDK 验证点**:
- 返回所有已注册工作流的摘要
- 摘要信息完整
- 按注册顺序或 ID 排序

**验证点**:
- 退出码为 0
- stdout 包含所有已注册的工作流
- 每个工作流显示 ID、名称、类型、状态
- 列表格式正确

---

## 3.4 列出所有工作流 - 详细模式

**测试用例名称**: `workflow_list_all_verbose`

**测试目标**: 以详细模式列出所有工作流

**测试步骤**:
1. 预先注册多个工作流
2. 执行 CLI 命令: `modular-agent workflow list --verbose`
3. 验证详细输出包含更多信息

**预期 stdout 内容**:
```
ID: standalone-wf-001
名称: Standalone Workflow
类型: STANDALONE
状态: active
版本: 1.0.0
创建时间: 2024-01-01T12:00:00.000Z
更新时间: 2024-01-01T12:00:00.000Z
节点数: 3
边数: 2
触发器数: 0

ID: triggered-wf-001
名称: Triggered Subworkflow
类型: TRIGGERED_SUBWORKFLOW
状态: active
版本: 1.0.0
创建时间: 2024-01-01T12:00:00.000Z
更新时间: 2024-01-01T12:00:00.000Z
节点数: 3
边数: 2
触发器数: 0
```

**预期 stderr 内容**:
```
(空)
```

**验证点**:
- 退出码为 0
- 输出包含更详细的信息(版本、时间、节点数等)
- 每个工作流显示完整的元数据

---

## 3.5 列出所有工作流 - 无工作流

**测试用例名称**: `workflow_list_all_empty`

**测试目标**: 当没有工作流时列出工作流

**测试步骤**:
1. 确保没有已注册的工作流
2. 执行 CLI 命令: `modular-agent workflow list`
3. 验证空列表的处理

**预期 stdout 内容**:
```
没有找到工作流
```

**预期 stderr 内容**:
```
(空)
```

**SDK 验证点**:
- 正确处理空注册表
- 返回空列表
- 不抛出错误

**验证点**:
- 退出码为 0
- stdout 显示友好提示信息
- 不抛出错误
