# Workflow 关系管理测试用例

## 测试目标

验证工作流关系管理功能，包括父子关系和依赖关系的建立与查询。

## SDK 核心组件

- **WorkflowRegistry** (sdk/graph/services/workflow-registry.ts)
- **WorkflowReferenceManager**

---

## 8.1 父子关系

**测试用例名称**: `workflow_relationship_parent_child`

**测试目标**: 验证工作流父子关系的建立

**测试步骤**:
1. 注册包含 SUBGRAPH 节点的工作流
2. 查询工作流关系
3. 验证父子关系正确建立

**预期 stdout 内容**:
```
工作流关系
----------

工作流 ID: parent-wf
类型: DEPENDENT

子工作流:
  - child-wf (SUBGRAPH)

工作流 ID: child-wf
类型: STANDALONE

父工作流:
  - parent-wf
```

**SDK 验证点**:
- 关系正确建立
- 可查询子工作流列表
- 可查询父工作流列表
- 关系类型正确识别

**验证点**:
- 退出码为 0
- 父子关系正确显示
- 关系类型正确

---

## 8.2 依赖关系

**测试用例名称**: `workflow_relationship_dependencies`

**测试目标**: 验证工作流依赖关系的管理

**测试步骤**:
1. 注册多个相互依赖的工作流
2. 查询工作流依赖
3. 验证依赖关系正确记录

**预期 stdout 内容**:
```
工作流依赖关系
--------------

工作流 ID: main-wf
类型: STANDALONE

依赖的工作流:
  - sub-wf-001 (TRIGGER: execute_triggered_subgraph)

工作流 ID: sub-wf-001
类型: STANDALONE

被以下工作流依赖:
  - main-wf
```

**SDK 验证点**:
- 依赖关系正确记录
- 可查询依赖列表
- 可查询被依赖列表
- 依赖类型正确识别

**验证点**:
- 退出码为 0
- 依赖关系正确显示
- 依赖类型正确
