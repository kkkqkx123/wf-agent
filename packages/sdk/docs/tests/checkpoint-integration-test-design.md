# Checkpoint 集成测试补充方案

## 1. 概述

基于缺口分析，当前 Checkpoint 模块的集成测试存在以下缺失：

| 模块 | 单元测试 | 集成测试 | 缺口 |
|------|----------|----------|------|
| Core（coordinator + state-manager + restorer + metrics） | ✅ 4 文件 | ❌ | 完整生命周期未覆盖 |
| Hierarchy（restore-coordinator + child-resolver/restorer + recovery + error-handler） | ❌ | ❌ | 全部缺失 |
| CheckpointGraph | ❌ | ❌ | 全部缺失 |
| CheckpointVersionManager | ❌ | ❌ | 全部缺失 |

## 2. 测试架构

### 2.1 测试分层

- Core 集成测试：直接实例化 `MemoryCheckpointStorage` + 内联 Test Double（避免引入完整 SDK 启动）
- Hierarchy 集成测试：Mock 回调 + 内联 Test Double
- Graph/Version 测试：纯函数测试 + MemoryCheckpointStorage

### 2.2 通用 Fixture

```typescript
// Test entity / checkpoint 类型
interface TestEntity { id: string; state: Record<string, unknown> }
interface TestCheckpoint extends BaseCheckpoint<Record<string, { from: unknown; to: unknown }>, Record<string, unknown>> {}
```

## 3. 测试用例清单

### 3.1 Core 生命周期集成测试

**文件**: `shared/checkpoint/__tests__/checkpoint-lifecycle.int.test.ts`

| 编号 | 用例名称 | 验证点 |
|------|----------|--------|
| CP-INT-01 | Full checkpoint 创建 + 存储 + 恢复完整链路 | Coordinator.createCheckpoint → StateManager 可查询 → Coordinator.restoreFromCheckpoint 返回一致状态 |
| CP-INT-02 | Delta 链创建与恢复 | FULL → DELTA → DELTA 创建，restore 最终 checkpoint 返回全量状态 |
| CP-INT-03 | Metrics 采集集成 | 多次创建/恢复后 MetricsCollector 的 count/duration metrics 正确递增 |
| CP-INT-04 | Cleanup 策略执行 | 创建 N 个 checkpoint，执行 CountBasedCleanup，验证存储中最新的保留 |
| CP-INT-05 | Delta 链压缩 | 创建 FULL → D1 → D2 → D3，compactDeltaChain 合并，验证 D1 更新为合并后的 FULL |

### 3.2 Hierarchy 恢复集成测试

**文件**: `shared/checkpoint/__tests__/hierarchy-restore.int.test.ts`

| 编号 | 用例名称 | 验证点 |
|------|----------|--------|
| CP-INT-06 | ChildCheckpointResolver 查询最新 checkpoint | StorageBackedChildResolver.resolveLatestCheckpoint 返回正确的 metadata |
| CP-INT-07 | ChildCheckpointRestorer 并行恢复多个 child | restoreChildren 按 concurency 限制并行执行，所有 child 恢复成功 |
| CP-INT-08 | ExecutionRestoreCoordinator 层次化恢复 | restoreRoot 恢复 root entity 并重建 child 树 |
| CP-INT-09 | RestoreStrategyRegistry 注册与查找 | register/get/has/getAll 正确，不存在的 type 返回 undefined |

### 3.3 Recovery Transaction 集成测试

**文件**: `shared/checkpoint/__tests__/recovery-transaction.int.test.ts`

| 编号 | 用例名称 | 验证点 |
|------|----------|--------|
| CP-INT-10 | 正常提交生命周期 | begin → registerOperation → completeOperation(all) → commit → 所有 compensation NOT called, status = committed |
| CP-INT-11 | All-or-Nothing 回滚 | begin → registerOperation → failOperation → rollback → 所有 compensation 按注册顺序逆序调用 → status = rolled_back |
| CP-INT-12 | Best-Effort 部分回滚 | begin → 2 completed + 1 failed → rollback → 已完成的 compensation 调用, 失败的忽略 → status = rolled_back_with_errors |
| CP-INT-13 | ErrorHandler 三种策略 | strict → 抛出异常；warn → 返回 continue；silent → 返回 continue（日志检查） |

### 3.4 CheckpointGraph + Version 集成测试

**文件**: `shared/checkpoint/__tests__/checkpoint-graph.int.test.ts`

| 编号 | 用例名称 | 验证点 |
|------|----------|--------|
| CP-INT-14 | 依赖图构建与保护计算 | 构造 parent-child 依赖关系 → computeProtectedCheckpoints 排除有依赖的 checkpoint |
| CP-INT-15 | VersionManager 兼容性检查 + 迁移 | checkCompatibility 返回 BLOCKING/COMPATIBLE/WARN → migrateCheckpoint 调用注册的 handler |

## 4. 实施策略

- 与源码同目录的 `__tests__/` 中创建 `.int.test.ts` 文件，遵循模块内集成测试模式
- 使用 `MemoryCheckpointStorage` 作为存储后端，无需外部依赖
- 内联 Test Double 实现抽象方法，避免引入完整 Agent/Workflow 系统
