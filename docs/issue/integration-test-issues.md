# 集成测试问题汇总

本文档记录在编写 SDK 工作流集成测试过程中发现的所有问题。

---

## 问题清单

### ISSUE-001: 存储适配器初始化缺失

**严重程度：** 🔴 高

**状态：** 待修复

**发现时间：** 2026-06-11

**问题描述：**

SDK bootstrap 过程中未初始化存储适配器，导致所有 Memory 存储适配器操作失败。

**错误信息：**
```
Storage not initialized. Call initialize() first.
```

**影响范围：**
- `sdk.workflows.create()` - 工作流注册失败
- `sdk.workflows.update()` - 工作流更新失败
- 所有依赖持久化的操作

**临时解决方案：**
测试中使用 `describe.skip` 跳过相关测试。

**根本解决方案：**
详见 [storage-initialization-issue.md](./storage-initialization-issue.md)

---

### ISSUE-002: 事件缺少 executionId

**严重程度：** 🟡 中

**状态：** 待调查

**发现时间：** 2026-06-11

**问题描述：**

某些事件在发射时缺少 `executionId` 字段，导致事件发射失败。

**错误信息：**
```
Event must have executionId
```

**影响范围：**
- 工作流执行完成后的某些事件
- 可能影响事件监听器

**临时解决方案：**
测试中跳过涉及事件发射的测试。

**需要调查：**
- 哪些事件缺少 executionId
- 事件构建逻辑是否正确
- 是否所有事件类型都需要 executionId

---

### ISSUE-003: WorkflowBuilder 重复节点 ID 静默覆盖

**严重程度：** 🟡 中

**状态：** 设计决策

**发现时间：** 2026-06-11

**问题描述：**

`WorkflowBuilder.addNode()` 使用 `Map` 存储节点，当添加重复 ID 的节点时，会静默覆盖前一个节点，而不是抛出错误。

**代码位置：**
```typescript
// sdk/api/workflow/builders/workflow-builder.ts
addNode(id: string, type: StaticNodeType, config: StaticNode['config']): this {
  const node = nodeBuilder.build();
  this.nodes.set(id, node);  // Map.set() 会覆盖
  return this;
}
```

**影响：**
- 用户可能无意中覆盖节点
- 难以调试重复 ID 问题

**当前行为：**
- `builder.build()` 会调用 `validate()` 检查重复 ID
- 但在 `addNode()` 时不会立即报错

**建议：**
- 选项 A：在 `addNode()` 时检查重复并抛出错误（更早发现问题）
- 选项 B：保持当前行为，依赖 `build()` 时的验证

---

### ISSUE-004: E2E 测试同样受存储初始化问题影响

**严重程度：** 🔴 高

**状态：** 待修复

**发现时间：** 2026-06-11

**问题描述：**

E2E 测试使用相同的 SDK 初始化方式，同样受 ISSUE-001 影响。

**验证命令：**
```bash
cd sdk && npx vitest run --config vitest.config.e2e.mjs "__tests__/e2e/workflow/"
```

**结果：**
```
expected false to be true
```

**说明：**
E2E 测试和集成测试共享相同的问题根源。

---

## 测试覆盖情况

### 已通过的测试

| 测试文件 | 测试用例 | 状态 |
|----------|----------|------|
| workflow-execution.int.test.ts | WF-INT-01: SDK API 可用性 | ✅ 4 通过 |
| workflow-scenarios.int.test.ts | WF-INT-07: 工作流验证 | ✅ 4 通过 |

### 已跳过的测试

| 测试文件 | 测试用例 | 原因 |
|----------|----------|------|
| workflow-execution.int.test.ts | WF-INT-02: 线性工作流 | ISSUE-001 |
| workflow-execution.int.test.ts | WF-INT-03: 多步工作流 | ISSUE-001 |
| workflow-execution.int.test.ts | WF-INT-04: 执行元数据 | ISSUE-001, ISSUE-002 |
| workflow-execution.int.test.ts | WF-INT-05: 变量节点 | ISSUE-001, ISSUE-002 |
| workflow-scenarios.int.test.ts | WF-INT-06: 条件路由 | ISSUE-001 |
| workflow-scenarios.int.test.ts | WF-INT-08: 错误处理 | ISSUE-001, ISSUE-002 |
| workflow-scenarios.int.test.ts | WF-INT-09: 输入数据 | ISSUE-001, ISSUE-002 |

**总计：** 8 通过，14 跳过

---

## 修复优先级

| 优先级 | 问题 | 影响 |
|--------|------|------|
| P0 | ISSUE-001 | 阻塞所有工作流测试 |
| P1 | ISSUE-004 | E2E 测试同样受影响 |
| P2 | ISSUE-002 | 事件系统问题 |
| P3 | ISSUE-003 | 开发体验问题 |

---

## 下一步行动

1. **修复 ISSUE-001**（P0）
   - 修改 SDK bootstrap 初始化存储适配器
   - 验证所有测试通过

2. **调查 ISSUE-002**（P2）
   - 定位缺少 executionId 的事件
   - 确定修复方案

3. **决定 ISSUE-003**（P3）
   - 确定是否需要修改 WorkflowBuilder 行为
   - 更新相关文档

---

## 相关文档

- [存储初始化问题详细分析](./storage-initialization-issue.md)
- [工作流测试计划](../tests/workflow-test-plan.md)
