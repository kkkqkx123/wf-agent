# Workflow API 测试用例

## 测试目标

验证 WorkflowRegistryAPI 的 CRUD 操作，包括创建、查询和版本化更新。

## SDK 核心组件

- **WorkflowRegistryAPI** (sdk/api/graph/resources/workflows/workflow-registry-api.ts)
- **WorkflowRegistry** (sdk/graph/services/workflow-registry.ts)

---

## 5.1 API.create()

**测试用例名称**: `workflow_api_create_success`

**测试目标**: 验证 API 层的创建操作

**测试步骤**:
1. 通过 API 创建工作流
2. 验证创建结果

**API 调用**:
```typescript
const result = await workflowRegistryAPI.create({
  workflow: {
    id: "api-wf-001",
    name: "API Workflow",
    type: "STANDALONE",
    version: "1.0.0",
    nodes: [
      { id: "start", type: "START" },
      { id: "end", type: "END" }
    ],
    edges: [
      { from: "start", to: "end" }
    ]
  }
});
```

**SDK 验证点**:
- 调用 WorkflowRegistry.register()
- 返回创建结果
- 正确处理 API 请求

**验证点**:
- 返回成功结果
- 工作流被注册
- 返回的工作流信息完整

---

## 5.2 API.getAll()

**测试用例名称**: `workflow_api_get_all_success`

**测试目标**: 验证 API 层的查询操作

**测试步骤**:
1. 预先注册多个工作流
2. 通过 API 查询所有工作流
3. 验证查询结果

**API 调用**:
```typescript
const result = await workflowRegistryAPI.getAll({
  filter: {
    type: "STANDALONE"
  }
});
```

**SDK 验证点**:
- 调用 WorkflowRegistry.list()
- 返回工作流数组
- 正确处理过滤条件

**验证点**:
- 返回符合条件的工作流列表
- 每个工作流信息完整
- 过滤条件正确应用

---

## 5.3 API.update() - 版本化更新

**测试用例名称**: `workflow_api_update_versioned_success`

**测试目标**: 验证 WorkflowRegistryAPI 的版本化更新

**测试步骤**:
1. 注册一个工作流
2. 通过 API 更新工作流
3. 验证版本化更新

**API 调用**:
```typescript
const result = await workflowRegistryAPI.update({
  workflowId: "wf-001",
  workflow: {
    id: "wf-001",
    name: "Updated Workflow",
    type: "STANDALONE",
    version: "2.0.0",
    nodes: [
      { id: "start", type: "START" },
      { id: "process", type: "LLM", config: { llmProfileId: "gpt-4o" } },
      { id: "end", type: "END" }
    ],
    edges: [
      { from: "start", to: "process" },
      { from: "process", to: "end" }
    ]
  },
  options: {
    keepPreviousVersion: true
  }
});
```

**SDK 验证点**:
- 创建新版本工作流
- 保留或删除原版本
- 版本号正确递增
- 版本历史记录完整

**验证点**:
- 返回更新后的工作流信息
- 版本号递增
- 原版本被保留(如果指定)
- 可以查询到不同版本的工作流
