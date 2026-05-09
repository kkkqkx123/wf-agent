# getContainer() 移除迁移指南

## 📋 概述

`getContainer()` 函数已被删除，所有调用者需要更新为使用 `GlobalContext`。

## 🔄 迁移模式

### 模式 1: 类构造函数注入（推荐）

**之前:**
```typescript
import { getContainer } from "../../../core/di/index.js";

export class MyService {
  someMethod() {
    const container = getContainer();
    const service = container.get(Identifiers.SomeService);
  }
}
```

**之后:**
```typescript
import type { GlobalContext } from "../../../core/global-context.js";

export class MyService {
  constructor(private readonly globalContext: GlobalContext) {}
  
  someMethod() {
    const service = this.globalContext.container.get(Identifiers.SomeService);
    // 或者使用便捷属性
    const eventManager = this.globalContext.eventRegistry;
  }
}
```

**DI 配置更新:**
```typescript
container.bind(Identifiers.MyService).toDynamicValue((c: IContainer) => {
  const globalContext = c.get(Identifiers.GlobalContext) as GlobalContext;
  return new MyService(globalContext);
});
```

### 模式 2: 函数式调用

**之前:**
```typescript
import { getContainer } from "../../../core/di/index.js";

export function myFunction() {
  const container = getContainer();
  const service = container.get(Identifiers.SomeService);
}
```

**之后 (通过参数传递):**
```typescript
import type { GlobalContext } from "../../../core/global-context.js";

export function myFunction(globalContext: GlobalContext) {
  const service = globalContext.container.get(Identifiers.SomeService);
}
```

### 模式 3: SDK 实例访问

**在应用层代码中:**
```typescript
const sdk = createSDK(options);
await sdk.waitForReady();

// 获取容器
const container = sdk.getGlobalContext().container;

// 或直接访问服务
const eventRegistry = sdk.getGlobalContext().eventRegistry;
```

## ✅ 已完成的迁移

### 1. WorkflowLifecycleCoordinator
- ✅ 添加 `globalContext` 构造函数参数
- ✅ 替换所有 `getContainer()` 调用
- ✅ 更新 DI 配置

### 2. 核心导出
- ✅ 从 `container-manager.ts` 删除 `getContainer()` 函数
- ✅ 从 `index.ts` 移除导出

## 📝 待迁移文件列表

以下文件仍需要迁移（按优先级排序）：

### 高优先级（核心协调器）
- [ ] `workflow-state-transitor.ts` - 类似 WorkflowLifecycleCoordinator
- [ ] `script-handler.ts` - 节点处理器
- [ ] `agent-loop-handler.ts` - Agent 循环处理器

### 中优先级（构建器和工厂）
- [ ] `workflow-graph-builder.ts` - 工作流图构建器
- [ ] `workflow-registry.ts` - 工作流注册表
- [ ] `workflow-execution-builder.ts` - 执行构建器
- [ ] `agent-loop-factory.ts` - Agent 工厂

### 低优先级（工具处理器）
- [ ] `call-agent/handler.ts`
- [ ] `execute-workflow/handler.ts`
- [ ] `query-workflow-status/handler.ts`
- [ ] `cancel-workflow/handler.ts`
- [ ] `execute-triggered-subgraph-handler.ts`

### 其他
- [ ] `checkpoint-restoration.ts` - 动态导入场景
- [ ] `execute-workflow-stream-command.ts` - 动态导入场景
- [ ] `workflow-handlers.test.ts` - 测试文件

## 🔧 迁移步骤

对于每个文件：

1. **移除导入**:
   ```typescript
   - import { getContainer } from ".../di/index.js";
   + import type { GlobalContext } from ".../global-context.js";
   ```

2. **添加构造函数参数** (如果是类):
   ```typescript
   constructor(
     // ... existing params
     private readonly globalContext: GlobalContext,
   ) {}
   ```

3. **替换调用**:
   ```typescript
   - const container = getContainer();
   - const service = container.get(Identifiers.SomeService);
   + const service = this.globalContext.container.get(Identifiers.SomeService);
   ```

4. **更新 DI 配置** (在 container-config.ts 中):
   ```typescript
   const globalContext = c.get(Identifiers.GlobalContext) as GlobalContext;
   return new MyClass(...otherDeps, globalContext);
   ```

## ⚠️ 注意事项

1. **GlobalContext 已经绑定到容器**: 在 `sdk-instance.ts` 中已绑定，可以直接从容器获取

2. **便捷属性**: GlobalContext 提供了一些便捷属性，优先使用：
   - `globalContext.eventRegistry`
   - `globalContext.workflowRegistry`
   - `globalContext.toolRegistry`
   - 等等...

3. **测试文件**: 测试可能需要 mock GlobalContext 或传入测试容器

## 🎯 完成标准

- [ ] 所有 `import { getContainer }` 已移除
- [ ] 所有 `getContainer()` 调用已替换
- [ ] 所有 DI 配置已更新
- [ ] 构建成功 (`pnpm build`)
- [ ] 测试通过 (`pnpm test`)
