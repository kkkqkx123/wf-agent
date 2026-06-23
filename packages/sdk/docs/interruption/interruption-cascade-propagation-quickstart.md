# 中断级联传播 - 快速开始指南

## 🚀 5分钟快速上手

### 1. 基本用法

```typescript
import { InterruptionState } from "@wf-agent/sdk/core/utils/interruption";

// 创建父实例
const parent = new InterruptionState({ contextId: "parent-workflow" });

// 创建子实例
const child = new InterruptionState({ contextId: "child-subgraph" });

// 注册子实例(自动建立级联传播)
parent.registerChild(child);

// 现在,父实例的中断会自动传播到子实例!
parent.requestPause(); // child 也会自动暂停
parent.requestStop(); // child 也会自动停止
parent.resume(); // child 也会自动恢复
```

### 2. Workflow 集成

在创建子工作流时,中断级联会**自动建立**:

```typescript
// 在 WorkflowExecutionBuilder 中
const result = await builder.createChildExecution(parentWorkflow, {
  type: "SUBGRAPH",
  config: {
    subworkflowId: "sub-graph-1",
    nodeId: "agent-node-1",
  },
});

// ✅ 中断级联已自动建立,无需额外代码!
```

### 3. 继承规则

| 子执行类型              | 是否继承中断 | 说明              |
| ----------------------- | ------------ | ----------------- |
| SUBGRAPH                | ✅ 是        | 同步执行,始终继承 |
| FORK_BRANCH             | ✅ 是        | 同步执行,始终继承 |
| TRIGGERED (async=false) | ✅ 是        | 同步触发,继承     |
| TRIGGERED (async=true)  | ❌ 否        | 异步触发,独立     |

---

## 📖 API 参考

### InterruptionState

#### 核心方法

```typescript
class InterruptionState {
  // 请求暂停
  requestPause(): void;

  // 请求停止
  requestStop(): void;

  // 恢复执行(自动传播到子实例)
  resume(): void;

  // 注册子实例(建立级联传播)
  registerChild(childState: InterruptionState): void;

  // 注销子实例(清理)
  unregisterChild(childState: InterruptionState): void;

  // 订阅中断事件
  onInterrupted(callback: (type: "PAUSE" | "STOP" | "RESUME") => void): () => void;

  // 清理资源
  dispose(): void;
}
```

#### 查询方法

```typescript
// 检查是否已中止
isAborted(): boolean;

// 检查是否应该暂停
shouldPause(): boolean;

// 检查是否应该停止
shouldStop(): boolean;

// 获取中止原因
getAbortReason(): Error | undefined;

// 获取 AbortSignal
getAbortSignal(): AbortSignal;
```

### InterruptionPropagationProxy

```typescript
class InterruptionPropagationProxy {
  // 附加到父实例
  attachToParent(parentState: InterruptionState): void;

  // 注册子实例
  registerChild(childState: InterruptionState): void;

  // 注销子实例
  unregisterChild(childState: InterruptionState): void;

  // 清理资源
  dispose(): void;
}
```

---

## 🔧 高级用法

### 1. 自定义错误处理

```typescript
const state = new InterruptionState({
  contextId: "my-execution",
  createInterruptionError: info => {
    return new MyCustomInterruptedException(info.message, info.type);
  },
});
```

### 2. 监听中断事件

```typescript
const state = new InterruptionState({ contextId: "test" });

// 订阅中断事件
const unsubscribe = state.onInterrupted(type => {
  console.log(`Received ${type} event`);

  if (type === "PAUSE") {
    // 保存状态
    saveCheckpoint();
  } else if (type === "RESUME") {
    // 刷新信号引用
    refreshSignal(state.getAbortSignal());
  }
});

// 稍后取消订阅
unsubscribe();
```

### 3. 深层嵌套监控

系统会自动监控传播深度,超过10层时会记录警告日志:

```
[WARN] Deep interruption propagation detected
  depth: 12
  type: PAUSE
  parentContextId: root-workflow
  maxRecommendedDepth: 10
```

**建议**: 如果经常看到这个警告,考虑重构工作流结构。

---

## ⚠️ 注意事项

### 1. 内存管理

**必须清理**:

```typescript
// ✅ 正确做法
childEntity.cleanup(); // 调用 unregisterChild
parentInterruptionState.dispose(); // 清理所有资源

// ❌ 错误做法 - 可能导致内存泄漏
// 忘记调用 cleanup/dispose
```

### 2. Resume 自动传播

**重要**: `resume()` 现在会**自动传播**到所有子实例!

```typescript
// 之前的设计(已废弃)
parent.resume();
await coordinator.cascadeResumeToChildren(parent); // ❌ 不再需要

// 现在的实现
parent.resume(); // ✅ 自动传播,无需额外调用
```

### 3. 单向传播

中断只能从父→子传播,**不能**从子→父传播:

```typescript
const parent = new InterruptionState({ contextId: "parent" });
const child = new InterruptionState({ contextId: "child" });

parent.registerChild(child);

// 子实例暂停不会影响父实例
child.requestPause();
console.log(parent.shouldPause()); // false ✅
```

### 4. 并发安全

多次调用是安全的,只会触发一次实际的中断:

```typescript
parent.requestPause();
parent.requestPause(); // 不会重复传播
parent.requestPause(); // 不会重复传播
```

---

## 🐛 故障排查

### 问题1: 子实例没有收到中断

**可能原因**:

1. 忘记调用 `registerChild()`
2. 子实例的 `inheritsInterruption` 设置为 `false`

**解决方法**:

```typescript
// 确保调用了 registerChild
parentInterruptionState.registerChild(childInterruptionState);

// 检查 ChildExecutionReference
console.log(childRef.inheritsInterruption); // 应该是 true
```

### 问题2: 内存泄漏

**症状**: 长时间运行后内存持续增长

**解决方法**:

```typescript
// 确保在子实例完成时清理
childEntity.cleanup();

// 确保在父实例销毁时清理
parentInterruptionState.dispose();
```

### 问题3: 深层嵌套性能问题

**症状**: 日志中出现 "Deep interruption propagation detected"

**解决方法**:

1. 检查工作流结构,减少嵌套深度
2. 考虑将部分 SUBGRAPH 改为 TRIGGERED (异步)
3. 优化节点执行逻辑

---

## 📊 监控指标

### 关键指标

```typescript
// 传播成功率
successful_propagations / total_propagations > 99.9%

// 平均延迟
average_latency < 5ms

// P95/P99 延迟
p95_latency < 10ms
p99_latency < 20ms

// 最大深度
max_depth < 10 (warning threshold)
```

### 日志示例

```
[INFO] Interruption cascade established
  parentExecutionId: workflow-123
  childExecutionId: subgraph-456
  childType: SUBGRAPH

[INFO] Execution pause requested
  contextId: workflow-123
  nodeId: agent-node-1

[DEBUG] Propagating interruption to children
  type: PAUSE
  childCount: 3
  depth: 1

[WARN] Deep interruption propagation detected
  depth: 12
  type: PAUSE
  parentContextId: root-workflow
```

---

## 🎯 最佳实践

### 1. 始终清理资源

```typescript
class MyExecutionEntity {
  async cleanup(): Promise<void> {
    // 1. 从父实例注销
    const parentContext = this.getParentContext();
    if (parentContext) {
      const parentEntity = getParentEntity(parentContext.parentId);
      parentEntity?.getInterruptionState()?.unregisterChild(this.getInterruptionState());
    }

    // 2. 清理自身资源
    this.interruptionState?.dispose();

    // 3. 其他清理...
  }
}
```

### 2. 使用结构化日志

```typescript
logger.info("Interruption cascade established", {
  parentExecutionId: parent.id,
  childExecutionId: child.id,
  childType: type,
  inheritsInterruption: shouldInherit,
});
```

### 3. 测试边界情况

```typescript
it("should handle deep nesting correctly", () => {
  // 测试3层、5层、10层嵌套
  // 验证所有层级都正确传播
});

it("should clean up resources on dispose", () => {
  // 测试内存泄漏防护
});
```

---

## 📚 相关文档

- [完整架构设计](./interruption-cascade-propagation-design.md)
- [详细修改方案](./interruption-cascade-propagation-modification-plan.md)
- [实施总结报告](./interruption-cascade-propagation-implementation-summary.md)
- [API 文档](../../sdk/core/types/README.md)

---

## 💡 常见问题 (FAQ)

### Q: 如何禁用某个子实例的中断继承?

A: 在创建子执行时设置 `inheritsInterruption: false`:

```typescript
parent.registerChild({
  childType: "WORKFLOW",
  childId: child.id,
  createdAt: Date.now(),
  inheritsInterruption: false, // 不继承中断
});
```

### Q: RESUME 真的会自动传播吗?

A: 是的! 这是与 PAUSE/STOP 保持一致的设计。调用 `parent.resume()` 会自动传播到所有注册的子实例。

### Q: 可以动态添加/删除子实例吗?

A: 可以! 使用 `registerChild()` 和 `unregisterChild()` 方法:

```typescript
// 动态添加
parentInterruptionState.registerChild(newChildState);

// 动态删除
parentInterruptionState.unregisterChild(oldChildState);
```

### Q: 传播失败怎么办?

A: 系统会记录错误日志并继续传播其他子实例。如果需要严格模式,可以检查 `PropagationResult`:

```typescript
const result = proxy.propagateToInterruption("PAUSE");
if (!result.success) {
  console.error("Failed children:", result.failedChildren);
  // 处理失败...
}
```

---

**最后更新**: 2026-05-18  
**版本**: v1.0.0

🎉 **祝使用愉快!**
