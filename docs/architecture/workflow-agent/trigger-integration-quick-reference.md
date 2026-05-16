# TriggerCoordinator 集成 - 快速参考指南

## 🚀 快速开始

如果你已经阅读了详细文档，这里是实施的关键步骤摘要。

---

## 📋 核心修改清单

### 1. ExecutionEventEmitter (3处修改)

**文件**: `sdk/core/registry/event-emitter.ts`

```typescript
// ✅ 添加导入
import type { TriggerCoordinator } from "../../workflow/execution/coordinators/trigger-coordinator.js";

// ✅ 添加字段
private triggerCoordinator?: TriggerCoordinator;

// ✅ 更新构造函数
constructor(executionId: string, triggerCoordinator?: TriggerCoordinator) {
  this.executionId = executionId;
  this.triggerCoordinator = triggerCoordinator;
}

// ✅ 在 emit() 末尾添加
if (this.triggerCoordinator) {
  try {
    await this.triggerCoordinator.handleEvent(event);
  } catch (error) {
    logger.warn('Trigger execution failed', { error });
  }
}
```

---

### 2. EventRegistry (2处修改)

**文件**: `sdk/core/registry/event-registry.ts`

```typescript
// ✅ 添加导入
import type { IdBasedServiceFactory } from "../di/service-factory-types.js";

// ✅ 添加字段和方法
private triggerCoordinatorFactory?: IdBasedServiceFactory<TriggerCoordinator>;

setTriggerCoordinatorFactory(factory: IdBasedServiceFactory<TriggerCoordinator>): void {
  this.triggerCoordinatorFactory = factory;
}

// ✅ 更新 getEmitter()
getEmitter(executionId: string): ExecutionEventEmitter {
  if (!this.emitters.has(executionId)) {
    let triggerCoordinator: TriggerCoordinator | undefined;
    if (this.triggerCoordinatorFactory) {
      triggerCoordinator = this.triggerCoordinatorFactory.create(executionId);
    }
    
    this.emitters.set(
      executionId, 
      new ExecutionEventEmitter(executionId, triggerCoordinator)
    );
  }
  return this.emitters.get(executionId)!;
}
```

---

### 3. DI Container Config (1处修改)

**文件**: `sdk/core/di/container-config.ts`

```typescript
// ✅ 更新 EventRegistry 绑定
container
  .bind(Identifiers.EventRegistry)
  .toDynamicValue((c: IContainer): EventRegistry => {
    const eventRegistry = new EventRegistry();
    const triggerCoordinatorFactory = c.get(Identifiers.TriggerCoordinator);
    eventRegistry.setTriggerCoordinatorFactory(
      triggerCoordinatorFactory as IdBasedServiceFactory<TriggerCoordinator>
    );
    return eventRegistry;
  })
  .inSingletonScope();
```

---

## 🧪 测试要点

### 单元测试

```typescript
// 测试 1: 触发器被调用
it('should call trigger coordinator when emitting event', async () => {
  const mockTC = { handleEvent: vi.fn().mockResolvedValue(undefined) };
  const emitter = new ExecutionEventEmitter('exec-1', mockTC);
  
  await emitter.emit(testEvent);
  
  expect(mockTC.handleEvent).toHaveBeenCalledWith(testEvent);
});

// 测试 2: 触发器失败不影响主流程
it('should continue even if trigger fails', async () => {
  const mockTC = { handleEvent: vi.fn().mockRejectedValue(new Error('fail')) };
  const emitter = new ExecutionEventEmitter('exec-1', mockTC);
  
  await expect(emitter.emit(testEvent)).resolves.not.toThrow();
});

// 测试 3: 没有触发器时正常工作
it('should work without trigger coordinator', async () => {
  const emitter = new ExecutionEventEmitter('exec-1');
  await expect(emitter.emit(testEvent)).resolves.not.toThrow();
});
```

### 集成测试

创建一个带触发器的工作流，验证：
1. 触发器在事件发出时被执行
2. 触发动作正确执行（如停止工作流）
3. 日志输出清晰

---

## ⚠️ 常见陷阱

### ❌ 陷阱 1: 忘记错误处理

```typescript
// 错误示例
await this.triggerCoordinator.handleEvent(event); // 可能抛出异常

// 正确示例
try {
  await this.triggerCoordinator.handleEvent(event);
} catch (error) {
  logger.warn('Trigger execution failed', { error });
  // 不中断主流程
}
```

### ❌ 陷阱 2: 循环依赖

确保 DI 容器配置顺序正确：
1. 先绑定 TriggerCoordinator
2. 再绑定 EventRegistry（依赖 TriggerCoordinator）

### ❌ 陷阱 3: 忘记清理触发器状态

在 workflow execution 结束时调用：
```typescript
triggerCoordinator.clear();
```

---

## 🔍 调试技巧

### 检查触发器是否注册

```typescript
const triggers = triggerCoordinator.getAll();
console.log('Registered triggers:', triggers.length);
```

### 跟踪事件流向

```typescript
// 在 ExecutionEventEmitter.emit() 中添加日志
logger.debug('Emitting event', { eventType: event.type });
logger.debug('Calling trigger coordinator', { 
  hasCoordinator: !!this.triggerCoordinator 
});
```

### 验证触发器执行

```typescript
// 在 TriggerCoordinator.handleEvent() 中添加日志
logger.info('Handling event', { 
  eventType: event.type,
  enabledTriggers: enabledTriggers.length 
});
```

---

## 📊 性能优化

### 快速路径优化

```typescript
async emit<T extends BaseEvent>(event: T): Promise<void> {
  // 如果没有触发器，直接返回
  if (!this.triggerCoordinator) {
    return;
  }
  
  // 检查是否有匹配的触发器
  const hasMatching = this.triggerCoordinator.hasMatchingTriggers(event.type);
  if (!hasMatching) {
    return;
  }
  
  // 执行触发器
  await this.triggerCoordinator.handleEvent(event);
}
```

### 缓存匹配结果

```typescript
// 在 TriggerCoordinator 中缓存
private triggerCache = new Map<EventType, boolean>();

hasMatchingTriggers(eventType: EventType): boolean {
  if (this.triggerCache.has(eventType)) {
    return this.triggerCache.get(eventType)!;
  }
  
  const hasMatch = /* 检查逻辑 */;
  this.triggerCache.set(eventType, hasMatch);
  return hasMatch;
}
```

---

## 🎯 验收清单

实施完成后，确认以下项目：

- [ ] ExecutionEventEmitter 接受可选的 TriggerCoordinator
- [ ] emit() 方法在触发器存在时调用 handleEvent()
- [ ] 触发器异常不会中断事件发射
- [ ] EventRegistry 可以设置 TriggerCoordinator 工厂
- [ ] 每个 ExecutionEventEmitter 关联一个 TriggerCoordinator
- [ ] DI 容器正确配置依赖关系
- [ ] 所有单元测试通过
- [ ] 集成测试通过
- [ ] 手动测试验证功能正常
- [ ] 日志输出清晰可追踪
- [ ] 没有性能退化
- [ ] 文档已更新

---

## 📚 相关文档

- [详细实现分析](./trigger-coordinator-implementation-analysis.md)
- [实施任务清单](./trigger-coordinator-implementation-tasks.md)
- [清理总结](./workflow-execution-coordinator-cleanup-summary.md)

---

## 🆘 遇到问题？

### Q: 触发器没有被执行？

**检查**:
1. TriggerCoordinator 是否正确注入到 ExecutionEventEmitter？
2. 触发器是否已注册？(`triggerCoordinator.getAll()`)
3. 事件类型是否匹配？
4. 触发器是否启用？

### Q: 触发器执行失败？

**检查**:
1. 查看日志中的错误信息
2. 验证触发动作的依赖是否可用
3. 检查触发器配置是否正确

### Q: 性能问题？

**检查**:
1. 是否有太多触发器？
2. 是否每次事件都检查所有触发器？
3. 考虑添加快速路径优化

---

**最后更新**: 2026-05-16  
**版本**: 1.0
