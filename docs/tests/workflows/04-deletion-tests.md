# Workflow 删除测试用例

## 测试目标

验证 WorkflowRegistry.unregister() 的正确性，包括单个删除、级联删除和依赖检查。

## SDK 核心组件

- **WorkflowRegistry** (sdk/graph/services/workflow-registry.ts)
- **WorkflowRegistryAPI** (sdk/api/graph/resources/workflows/workflow-registry-api.ts)

---

## 4.1 删除工作流 - 存在

**测试用例名称**: `workflow_delete_existing_success`

**测试目标**: 验证 WorkflowRegistry.unregister() 的正确性

**测试步骤**:
1. 预先注册一个工作流
2. 执行 CLI 命令: `modular-agent workflow delete wf-001 --force`
3. 验证删除成功
4. 尝试查询已删除的工作流

**预期 stdout 内容**:
```
工作流已删除: wf-001
```

**预期 stderr 内容**:
```
(空)
```

**SDK 验证点**:
- 工作流从注册表移除
- 无法再查询
- 触发删除事件
- 依赖关系被清理

**验证点**:
- 退出码为 0
- stdout 包含 "工作流已删除"
- 工作流不再可以通过 list 命令查询到
- 尝试查询已删除的工作流返回错误

---

## 4.2 删除工作流 - 不存在

**测试用例名称**: `workflow_delete_not_found`

**测试目标**: 验证删除不存在工作流的错误处理

**测试步骤**:
1. 执行 CLI 命令: `modular-agent workflow delete nonexistent-id --force`
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
- 不触发删除事件

**验证点**:
- 退出码非 0
- stderr 包含明确的错误信息
- 错误信息包含工作流 ID

---

## 4.3 删除工作流 - 有依赖

**测试用例名称**: `workflow_delete_with_dependencies`

**测试目标**: 验证删除有依赖关系的工作流

**测试步骤**:
1. 注册一个包含 SUBGRAPH 节点的工作流(依赖另一个工作流)
2. 尝试删除被依赖的工作流
3. 验证依赖检查

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

**预期 stdout 内容**:
```
(空)
```

**预期 stderr 内容**:
```
错误：无法删除工作流 child-wf
原因：工作流被以下工作流引用:
  - parent-wf
使用 --cascade 选项级联删除依赖的工作流
```

**SDK 验证点**:
- 检查工作流引用关系
- 有依赖时拒绝删除
- 提供级联删除选项
- 列出所有依赖工作流

**验证点**:
- 退出码非 0
- stderr 包含依赖关系信息
- 被依赖的工作流未被删除

---

## 4.4 删除工作流 - 级联删除

**测试用例名称**: `workflow_delete_cascade_success`

**测试目标**: 验证级联删除功能

**测试步骤**:
1. 注册一个包含 SUBGRAPH 节点的工作流
2. 执行 CLI 命令: `modular-agent workflow delete child-wf --cascade`
3. 验证级联删除成功

**预期 stdout 内容**:
```
工作流已删除: child-wf
级联删除以下工作流:
  - parent-wf
```

**预期 stderr 内容**:
```
(空)
```

**SDK 验证点**:
- 正确识别所有依赖工作流
- 按正确顺序删除(先删除依赖者)
- 所有相关工作流被删除

**验证点**:
- 退出码为 0
- 被删除的工作流和所有依赖工作流都被删除
- stdout 显示级联删除的工作流列表
