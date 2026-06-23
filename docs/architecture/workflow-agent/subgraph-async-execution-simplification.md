# SUBGRAPH 异步执行简化决策

## 决策日期
2026-05-16

## 决策内容
**SUBGRAPH 节点仅支持同步执行，移除异步执行能力。**

如需异步/并行执行，应使用 FORK 节点。

---

## 背景

在 Phase 1 实施过程中，我们实现了 SUBGRAPH 的方案 C（独立执行实体模型）。初始设计包含了同步和异步两种执行模式，但在代码审查中发现：

1. **SUBGRAPH 的典型使用场景是嵌入工作流内部**，作为工作流的一个步骤
2. **需要异步/并行执行的场景通常使用 FORK 节点**
3. **保留两种执行模式会增加代码复杂度和用户认知负担**

---

## 决策理由

### 1. 职责清晰
- **SUBGRAPH**：专注于"嵌入子工作流"的职责，同步等待完成
- **FORK**：专注于"并行/异步执行"的职责，创建多个分支

### 2. 避免混淆
用户不会困惑何时使用 `SUBGRAPH(async=true)` vs `FORK`，因为：
- SUBGRAPH 永远是同步的
- 需要并行时自然选择 FORK

### 3. 代码简化
移除了约 60 行代码：
- `executeAsync()` 函数（包含 TODO 注释）
- 条件分支逻辑（if/else 判断 async）
- 相关配置选项（`timeout` 等）

### 4. 性能考虑
真正的并行需求应该用 FORK，而不是通过 SUBGRAPH 的异步模式模拟。

---

## 影响范围

### 代码修改

#### 1. `sdk/workflow/execution/handlers/node-handlers/subgraph-handler.ts`
- ✅ 删除 `executeAsync()` 函数
- ✅ 简化执行逻辑，移除 if/else 分支
- ✅ 更新文档注释，明确仅支持同步执行
- ✅ 强制设置 `async: false`

**代码行数变化**：-60 行

#### 2. `docs/architecture/workflow-agent/subgraph-embedgraph-architecture-analysis.md`
- ✅ 更新架构对比表："可同步/异步" → "仅同步执行（异步应使用 FORK）"
- ✅ 简化执行流程示例，移除异步分支
- ✅ 更新 `SubgraphNodeConfig` 类型定义，标注 `async?: false`
- ✅ 更新功能对比表和验收标准
- ✅ 更新决策理由说明

---

## 替代方案

如果用户需要异步/并行执行子工作流，应该使用：

### 方案 1：FORK 节点（推荐）
```toml
[[nodes]]
id = "parallel-task"
type = "FORK"

[[nodes.fork.branches]]
id = "branch-1"
# ... branch configuration

[[nodes.fork.branches]]
id = "branch-2"
# ... branch configuration
```

### 方案 2：TRIGGERED_SUBWORKFLOW（未来）
如果需要完全独立的异步任务（不阻塞父工作流），可以使用 Triggered Subworkflow 模式。

---

## 向后兼容性

由于这是新功能的 Phase 1 实现，尚未正式发布，因此：
- ✅ **无向后兼容性问题**
- ✅ 不需要迁移指南
- ✅ 可以直接采用新设计

---

## 测试影响

需要验证：
1. SUBGRAPH 节点在同步模式下正常工作
2. 变量导入/导出正确执行
3. 消息上下文传递正常
4. 没有遗留的异步执行相关代码路径

---

## 相关文档

- [SUBGRAPH 与 EmbedGraph 架构设计分析](./subgraph-embedgraph-architecture-analysis.md)
- [变量传递架构重构 - 最终决策与规划](./variable-passing-refactoring-tasks.md)

---

## 批准人
- 架构决策由 AI 助手提出
- 基于用户的明确反馈："考虑到subgraph一般是嵌入工作流内部的，需要异步执行的一般在fork中使用，考虑是否应该仅保留一种实现"

---

## 后续行动

1. ✅ 已完成代码简化
2. ✅ 已更新架构文档
3. ⏸️ 待编写单元测试验证同步执行
4. ⏸️ 待运行现有测试确保无回归
