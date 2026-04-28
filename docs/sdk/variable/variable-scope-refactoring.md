# 变量作用域重构方案

## 问题分析

### 原始问题
当前workflow变量系统中的thread、loop、subgraph变量存在作用域处理问题：

1. **初始化时机不当**：subgraph和loop作用域的变量在Thread创建时就试图初始化，但此时对应的作用域栈是空的
2. **静态声明 vs 动态作用域**：workflow中静态声明的变量无法适应运行时动态创建的嵌套作用域
3. **fork/join复杂性**：thread变量与fork/join操作的状态管理存在冲突

### 根本原因
- 所有变量（包括subgraph和loop作用域）都在`VariableManager.initializeFromWorkflow()`中统一初始化
- 但subgraph和loop作用域是运行时动态创建的栈结构，初始化时栈为空
- 导致这些变量无法正确分配到对应的作用域中

## 解决方案

### 核心原则
- **Global作用域**：在Thread创建时立即初始化（保持现有行为）
- **Thread/Loop/Subgraph作用域**：统一按需初始化

### 实现策略

#### 1. 修改初始化逻辑
**文件**: `sdk/core/execution/managers/variable-manager.ts`

**修改内容**:
- `initializeFromWorkflow()`方法中，只有global作用域的变量在初始化时直接赋值
- thread、subgraph、loop作用域的变量只做声明，不初始化值

```typescript
// 按作用域分配变量值
// 只有 global 作用域的变量在初始化时直接赋值
// thread、subgraph、loop 作用域的变量按需初始化
for (const variable of thread.variables) {
  switch (variable.scope) {
    case 'global':
      // global 作用域变量立即初始化
      thread.variableScopes.global[variable.name] = variable.value;
      break;
    case 'thread':
    case 'subgraph':
    case 'loop':
      // thread、subgraph、loop 作用域的变量按需初始化
      // 这里只做声明，不初始化值
      break;
  }
}
```

#### 2. 实现按需初始化机制
**文件**: `sdk/core/execution/managers/variable-manager.ts`

**新增方法**: `initializeVariableOnDemand()`

```typescript
/**
 * 按需初始化变量
 * @param thread Thread 实例
 * @param name 变量名称
 * @param scope 作用域
 * @param scopeObject 作用域对象
 * @returns 初始化的值，如果变量不存在则返回undefined
 */
private initializeVariableOnDemand(
  thread: Thread,
  name: string,
  scope: VariableScope,
  scopeObject: Record<string, any>
): any {
  const variableDef = thread.variables.find(v => v.name === name && v.scope === scope);
  
  if (!variableDef) {
    return undefined;
  }

  // 使用默认值初始化
  const initialValue = variableDef.value;
  scopeObject[name] = initialValue;
  
  return initialValue;
}
```

**修改方法**: `getVariable()`

在查找变量时，如果变量未初始化，则自动进行初始化：

```typescript
// 1. 循环作用域（最高优先级）
if (scopes.loop.length > 0) {
  const currentLoopScope = scopes.loop[scopes.loop.length - 1];
  if (currentLoopScope && name in currentLoopScope) {
    return currentLoopScope[name];
  }
  // 如果变量未初始化，尝试按需初始化
  if (currentLoopScope && !(name in currentLoopScope)) {
    const initialized = this.initializeVariableOnDemand(thread, name, 'loop', currentLoopScope);
    if (initialized !== undefined) {
      return initialized;
    }
  }
}
```

#### 3. 修改作用域管理逻辑
**文件**: `sdk/core/execution/managers/variable-manager.ts`

**修改方法**: `enterSubgraphScope()` 和 `enterLoopScope()`

在进入作用域时，自动初始化该作用域的所有变量：

```typescript
/**
 * 进入子图作用域
 * 自动初始化该作用域的变量
 * @param threadContext ThreadContext 实例
 */
enterSubgraphScope(threadContext: ThreadContext): void {
  const thread = threadContext.thread;
  const newScope: Record<string, any> = {};
  
  // 初始化该作用域的所有subgraph变量
  for (const variable of thread.variables) {
    if (variable.scope === 'subgraph') {
      newScope[variable.name] = variable.value;
    }
  }
  
  thread.variableScopes.subgraph.push(newScope);
}

/**
 * 进入循环作用域
 * 自动初始化该作用域的变量
 * @param threadContext ThreadContext 实例
 */
enterLoopScope(threadContext: ThreadContext): void {
  const thread = threadContext.thread;
  const newScope: Record<string, any> = {};
  
  // 初始化该作用域的所有loop变量
  for (const variable of thread.variables) {
    if (variable.scope === 'loop') {
      newScope[variable.name] = variable.value;
    }
  }
  
  thread.variableScopes.loop.push(newScope);
}
```

#### 4. 调整loop-start-handler
**文件**: `sdk/core/execution/handlers/node-handlers/loop-start-handler.ts`

**修改内容**: 在创建循环作用域时，初始化该作用域的变量

```typescript
// 创建新的循环作用域并初始化该作用域的变量
const newLoopScope: Record<string, any> = {};
for (const variable of thread.variables) {
  if (variable.scope === 'loop') {
    newLoopScope[variable.name] = variable.value;
  }
}
thread.variableScopes.loop.push(newLoopScope);
```

#### 5. Fork/Join场景处理
**文件**: `sdk/core/execution/thread-builder.ts`

**现有逻辑**: fork操作时，只复制thread作用域的变量，global作用域通过引用共享，subgraph和loop作用域清空

这个逻辑已经符合新设计：
- global变量通过引用共享（所有线程共享）
- thread变量深拷贝到子线程（每个线程独立）
- subgraph和loop变量清空（在子线程中按需初始化）

## 方案优势

### 1. 解决作用域初始化时机问题
- Global变量在Thread创建时立即初始化
- Thread/Loop/Subgraph变量在首次访问或进入作用域时初始化
- 避免了在空作用域栈中初始化变量的问题

### 2. 支持嵌套作用域
- 每个作用域实例维护独立的变量状态
- 支持多层嵌套的loop和subgraph
- 变量查找优先级：loop > subgraph > thread > global

### 3. 简化fork/join场景
- 只复制已初始化的变量状态
- 未初始化的变量保持声明状态，在子线程中按需初始化
- 减少了状态管理的复杂度

### 4. 保持向后兼容
- 现有API不变
- Global变量行为保持不变
- Thread变量行为基本保持不变（只是延迟初始化）

## 修改文件清单

1. `sdk/core/execution/managers/variable-manager.ts`
   - 修改`initializeFromWorkflow()`方法
   - 新增`initializeVariableOnDemand()`方法
   - 修改`getVariable()`方法
   - 修改`enterSubgraphScope()`方法
   - 修改`enterLoopScope()`方法

2. `sdk/core/execution/handlers/node-handlers/loop-start-handler.ts`
   - 修改循环作用域创建逻辑

## 测试建议

### 单元测试
1. 测试global变量在Thread创建时立即初始化
2. 测试thread变量在首次访问时按需初始化
3. 测试subgraph变量在进入作用域时初始化
4. 测试loop变量在进入作用域时初始化
5. 测试嵌套作用域的变量隔离
6. 测试变量查找优先级

### 集成测试
1. 测试包含loop的工作流
2. 测试包含subgraph的工作流
3. 测试嵌套loop的工作流
4. 测试嵌套subgraph的工作流
5. 测试fork/join场景的变量复制

### 边界测试
1. 测试变量未定义时的行为
2. 测试只读变量的行为
3. 测试变量类型验证
4. 测试作用域栈为空时的行为

## 注意事项

1. **变量访问性能**: 按需初始化可能会在首次访问时增加少量开销，但这是可接受的
2. **变量默认值**: 确保所有变量都有合理的默认值，避免undefined问题
3. **作用域清理**: 确保退出作用域时正确清理变量状态
4. **并发安全**: 在多线程场景下，global变量的共享需要注意并发安全

## 后续优化建议

1. **变量缓存**: 可以考虑缓存已初始化的变量，减少重复初始化
2. **延迟加载**: 对于大型工作流，可以考虑延迟加载变量定义
3. **变量监控**: 添加变量访问和修改的监控，便于调试
4. **变量验证**: 增强变量类型和值的验证逻辑