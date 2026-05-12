# Variable Scope Refactoring - Analysis & SDK Migration Guide

## 概述

本文档分析哪些节点配置需要显式变量映射改造,并提供详细的 SDK 改造指南。

---

## 📊 节点配置改造分析

### ✅ 需要改造的节点

#### 1. **SUBGRAPH Node** - 已完成 ✅

**原因**: 
- Subgraph 是嵌入的工作流,应该有清晰的接口契约
- 当前使用隐式 scope 继承,难以追踪数据流

**改造内容**:
```typescript
interface SubgraphNodeConfig {
  subgraphId: ID;
  async: boolean;
  
  // NEW: Explicit variable mapping
  variableInputs?: SubgraphVariableInput[];
  variableOutputs?: SubgraphVariableOutput[];
  
  messagePassing?: { ... }; // Existing
}
```

---

#### 2. **START Node** - 已完成 ✅

**原因**:
- START 是工作流的入口点(尤其是 subgraph 展开后的起点)
- 应该像函数签名一样声明输入输出
- 与 SUBGRAPH 配置保持一致

**改造内容**:
```typescript
interface StartNodeConfig {
  // NEW: Variable inputs/outputs
  variableInputs?: StartVariableInput[];
  variableOutputs?: StartVariableOutput[];
  
  // Existing: Message context
  messageInputs?: Array<{...}>;
  messageOutputs?: Array<{...}>;
}
```

**设计说明**:
- Subgraph 展开后,其 START 节点的 `variableInputs` 对应 SUBGRAPH 节点的 `variableInputs`
- 这形成了完整的数据流链路: Parent → SUBGRAPH node → Child START node

---

#### 3. **LOOP_START Node** - 已完成 ✅

**原因**:
- Loop 内部不应该自动访问父工作流变量
- 需要明确声明 loop body 可以访问哪些变量
- 提高代码可读性和可维护性

**改造内容**:
```typescript
interface LoopStartNodeConfig {
  loopId: string;
  
  // NEW: Explicit variable inputs
  variableInputs?: LoopVariableInput[];
  
  dataSource?: DataSource;
  maxIterations: number;
}
```

**示例**:
```toml
[[nodes]]
id = "loop_start"
type = "LOOP_START"

[nodes.config]
loopId = "process-items"
maxIterations = 100

# 显式声明 loop 可以访问的父工作流变量
[[nodes.config.variableInputs]]
externalName = "config"
internalName = "loopConfig"
required = true

[[nodes.config.variableInputs]]
externalName = "threshold"
internalName = "maxValue"
defaultValue = 100

[nodes.config.dataSource]
iterable = "{{items}}"
variableName = "currentItem"
```

---

### ❌ 不需要改造的节点

#### 4. **FORK/JOIN Nodes** - 不需要改造

**原因分析**:

1. **Fork 不是独立的工作流**
   - Fork 分支仍在同一个 WorkflowExecution 中执行
   - 所有分支共享同一个 VariableManager 实例
   - 不存在跨工作流的边界

2. **并行执行的语义不同**
   ```typescript
   // Fork 分支 A 和 B 同时执行,但共享状态
   FORK {
     paths: [A, B]
   }
   
   // A 和 B 都可以读写相同的 execution scope 变量
   // 这是设计意图,不是问题
   ```

3. **如果需要隔离,应该使用 Subgraph**
   ```typescript
   // 错误做法: 用 Fork 实现隔离
   FORK {
     paths: [isolated_branch_1, isolated_branch_2]
   }
   
   // 正确做法: 用 Subgraph 实现隔离
   SUBGRAPH {
     subgraphId: "isolated-workflow-1"
     variableInputs: [...]  // 显式映射
   }
   ```

4. **Join 只是同步点,不涉及变量传递**
   - Join 等待分支完成
   - 不改变变量作用域
   - 不需要输入输出映射

**结论**: FORK/JOIN 保持现状,它们的设计目标是并行执行而非隔离。

---

#### 5. **TRIGGER_SUBWORKFLOW Nodes** - 已有显式映射 ✅

**现状**:
- `StartFromTriggerNodeConfig` 已经有 `messageInputs`
- `ContinueFromTriggerNodeConfig` 已经有 `variableCallback`

**分析**:
- Trigger Subworkflow 是完全独立的异步执行
- 已经通过 `inputMapping` 实现显式变量映射(见 `triggered-subworkflow-handler.ts`)
- 与我们的设计方案一致

**建议**: 保持不变,已经符合显式映射原则。

---

#### 6. **其他节点 (VARIABLE, LLM, SCRIPT 等)** - 不需要改造

**原因**:
- 这些是执行节点,不是控制流节点
- 它们操作的是当前工作流的变量
- 不涉及跨工作流/跨作用域的变量传递

---

## 🎯 总结表格

| 节点类型 | 是否改造 | 原因 | 优先级 |
|---------|---------|------|--------|
| SUBGRAPH | ✅ 已改造 | 跨工作流边界,需要清晰接口 | P0 |
| START | ✅ 已改造 | 工作流入口,函数签名语义 | P0 |
| LOOP_START | ✅ 已改造 | 循环隔离,避免隐式依赖 | P0 |
| FORK/JOIN | ❌ 不改造 | 同工作流内并行,共享状态是预期行为 | - |
| TRIGGER_SUBWORKFLOW | ✅ 已有 | 已有显式映射机制 | - |
| 其他节点 | ❌ 不改造 | 不涉及跨作用域 | - |

---

## 📝 SDK 改造指南

### Phase 1: VariableManager 重构

**文件**: `sdk/workflow/state-managers/variable-manager.ts`

#### 1.1 简化数据结构

```typescript
export class VariableManager implements StateManager<VariableManagerSnapshot> {
  /** Global scope - shared across all executions */
  private global: Map<string, VariableEntry> = new Map();
  
  /** Execution scope - isolated per execution */
  private execution: Map<string, VariableEntry> = new Map();
  
  // ❌ REMOVE: scopeStacks
  // private scopeStacks: ScopeStacks = { subgraph: [], loop: [] };
  
  // Optional cache
  private cache: Map<string, { value: unknown; timestamp: number }> | null = null;
  private cacheEnabled: boolean = false;
  private cacheTTL: number = 1000;
  
  constructor(options?: { enableCache?: boolean; cacheTTL?: number }) {
    if (options?.enableCache) {
      this.cacheEnabled = true;
      this.cache = new Map();
      this.cacheTTL = options.cacheTTL || 1000;
    }
  }
}
```

#### 1.2 更新 getVariable()

```typescript
/**
 * Get variable value by name
 * Priority: execution > global
 * 
 * NOTE: No more scope priority (loop/subgraph removed)
 */
getVariable(name: string): unknown {
  // Check cache first
  if (this.cacheEnabled && this.cache) {
    const cached = this.cache.get(name);
    if (cached && Date.now() - cached.timestamp < this.cacheTTL) {
      return cached.value;
    }
  }
  
  // Check execution scope first (higher priority)
  if (this.execution.has(name)) {
    const value = this.execution.get(name)!.value;
    
    // Update cache
    if (this.cacheEnabled && this.cache) {
      this.cache.set(name, { value, timestamp: Date.now() });
    }
    
    return value;
  }
  
  // Check global scope
  if (this.global.has(name)) {
    const value = this.global.get(name)!.value;
    
    // Update cache
    if (this.cacheEnabled && this.cache) {
      this.cache.set(name, { value, timestamp: Date.now() });
    }
    
    return value;
  }
  
  logger.debug("Variable not found", { name });
  return undefined;
}
```

#### 1.3 更新 setVariable()

```typescript
/**
 * Set variable value
 * Variables are stored in execution scope by default
 * Use explicit API to set global variables
 */
setVariable(name: string, value: unknown, freeze?: boolean): void {
  let entry = this.execution.get(name);
  
  // If not in execution scope, check global
  if (!entry) {
    entry = this.global.get(name);
  }
  
  if (!entry) {
    const availableVars = [
      ...Array.from(this.execution.keys()),
      ...Array.from(this.global.keys())
    ];
    throw new RuntimeValidationError(
      `Variable '${name}' is not defined. Available variables: ${availableVars.length > 0 ? availableVars.join(', ') : '(none)'}`,
      {
        operation: "setVariable",
        field: "variableName",
        value: name,
        context: { availableVariables: availableVars },
      }
    );
  }

  if (entry.definition.readonly) {
    throw new RuntimeValidationError(
      `Variable '${name}' is readonly and cannot be modified`,
      {
        operation: "setVariable",
        field: "variableName",
        value: name,
      }
    );
  }

  const shouldFreeze = freeze ?? entry.definition.freeze ?? false;

  logger.debug("Setting variable", { 
    name, 
    scope: entry.definition.scope, 
    freeze: shouldFreeze 
  });

  if (shouldFreeze && typeof value === 'object' && value !== null && !Object.isFrozen(value)) {
    Object.freeze(value);
    logger.debug("Froze object value for variable", { name });
  }

  // Update the value
  entry.value = value;

  // Invalidate cache
  if (this.cacheEnabled && this.cache) {
    this.cache.delete(name);
  }
}
```

#### 1.4 移除 scope 相关方法

```typescript
// ❌ REMOVE these methods entirely:
// - enterSubgraphScope()
// - exitSubgraphScope()
// - enterLoopScope()
// - exitLoopScope()

// ✅ KEEP these methods:
registerVariable(definition: VariableDefinition): void { ... }
hasVariable(name: string): boolean { ... }
getVariableDefinition(name: string): VariableDefinition | undefined { ... }
getAllVariableDefinitions(): VariableDefinition[] { ... }
getAllVariables(): Record<string, unknown> { ... }
deleteVariable(name: string): boolean { ... }
createSnapshot(): VariableManagerSnapshot { ... }
restoreFromSnapshot(snapshot: VariableManagerSnapshot): void { ... }
copyFrom(source: VariableManager): void { ... }
cleanup(): void { ... }
```

#### 1.5 更新 Snapshot 接口

```typescript
export interface VariableManagerSnapshot {
  global: Map<string, VariableEntry>;
  execution: Map<string, VariableEntry>;
}

createSnapshot(): VariableManagerSnapshot {
  return {
    global: new Map(this.global),
    execution: new Map(this.execution),
  };
}

restoreFromSnapshot(snapshot: VariableManagerSnapshot): void {
  this.global = new Map(snapshot.global);
  this.execution = new Map(snapshot.execution);
  
  // Clear cache after restore
  if (this.cacheEnabled && this.cache) {
    this.cache.clear();
  }
}
```

#### 1.6 更新 copyFrom() (用于 Fork)

```typescript
/**
 * Copy from another VariableManager (for fork scenarios)
 * 
 * Note: Global scope variables are shared by reference
 * Execution scope variables are deep copied
 */
copyFrom(source: VariableManager): void {
  // Share global variables by reference
  this.global = source.global;
  
  // Deep copy execution variables
  this.execution = new Map();
  for (const [name, entry] of source.execution) {
    this.execution.set(name, {
      definition: { ...entry.definition },
      value: entry.value,
    });
  }
  
  // Clear cache
  if (this.cacheEnabled && this.cache) {
    this.cache.clear();
  }
}
```

---

### Phase 2: Subgraph Handler 改造

**文件**: `sdk/workflow/execution/handlers/subgraph-handler.ts`

#### 2.1 更新 enterSubgraph()

```typescript
export async function enterSubgraph(
  executionEntity: WorkflowExecutionEntity,
  workflowId: string,
  parentWorkflowId: string,
  input: Record<string, unknown>,
  subgraphNode: Node,
): Promise<void> {
  const config = subgraphNode.config as SubgraphNodeConfig;
  const parentManager = executionEntity.variableStateManager;
  
  // Create child execution entity
  const childEntity = await createSubgraphContext(...);
  const childManager = childEntity.variableStateManager;
  
  // ✅ NEW: Explicit variable mapping
  if (config.variableInputs && config.variableInputs.length > 0) {
    for (const mapping of config.variableInputs) {
      const parentValue = parentManager.getVariable(mapping.externalName);
      
      if (parentValue === undefined) {
        if (mapping.required) {
          throw new RuntimeValidationError(
            `Required input '${mapping.externalName}' not found in parent workflow`,
            { operation: "enterSubgraph", field: "variableInputs" }
          );
        }
        if (mapping.defaultValue !== undefined) {
          childManager.setVariable(mapping.internalName, mapping.defaultValue);
        }
      } else {
        childManager.setVariable(mapping.internalName, parentValue);
      }
    }
  }
  
  // ❌ REMOVE: Automatic scope inheritance
  // parentManager.enterSubgraphScope();
  
  // Register child execution
  executionEntity.registerChildExecution(childEntity.id);
  
  // Enter subgraph context (for tracking, not for variable scope)
  executionEntity.enterSubgraph(workflowId, parentWorkflowId, input);
}
```

#### 2.2 更新 exitSubgraph()

```typescript
export async function exitSubgraph(
  executionEntity: WorkflowExecutionEntity,
  subgraphNode: Node,
): Promise<void> {
  const config = subgraphNode.config as SubgraphNodeConfig;
  const childEntity = getChildExecution(...); // Get child execution entity
  const childManager = childEntity.variableStateManager;
  const parentManager = executionEntity.variableStateManager;
  
  // ✅ NEW: Explicit variable output mapping
  if (config.variableOutputs && config.variableOutputs.length > 0) {
    for (const mapping of config.variableOutputs) {
      const childValue = childManager.getVariable(mapping.internalName);
      
      if (childValue !== undefined) {
        parentManager.setVariable(mapping.externalName, childValue);
      }
    }
  }
  
  // ❌ REMOVE: Automatic scope cleanup
  // parentManager.exitSubgraphScope();
  
  // Exit subgraph context
  executionEntity.exitSubgraph();
}
```

---

### Phase 3: Loop Handler 改造

**文件**: `sdk/workflow/execution/handlers/node-handlers/loop-start-handler.ts`

#### 3.1 更新 handle()

```typescript
export async function handle(
  _globalContext: GlobalContext,
  executionEntity: WorkflowExecutionEntity,
  node: Node,
  _context?: unknown,
): Promise<unknown> {
  const config = node.config as LoopStartNodeConfig;
  const manager = executionEntity.variableStateManager;
  
  // ✅ NEW: Map parent variables to loop scope
  if (config.variableInputs && config.variableInputs.length > 0) {
    for (const mapping of config.variableInputs) {
      const parentValue = manager.getVariable(mapping.externalName);
      
      if (parentValue === undefined) {
        if (mapping.required) {
          throw new RuntimeValidationError(
            `Required input '${mapping.externalName}' not found for loop`,
            { operation: "handle", field: "variableInputs" }
          );
        }
        if (mapping.defaultValue !== undefined) {
          manager.setVariable(mapping.internalName, mapping.defaultValue);
        }
      } else {
        manager.setVariable(mapping.internalName, parentValue);
      }
    }
  }
  
  // Rest of loop initialization logic...
  // ...
}
```

---

### Phase 4: Workflow Builder 验证

**文件**: `sdk/api/workflow/builders/workflow-builder.ts`

#### 4.1 添加验证逻辑

```typescript
build(): WorkflowTemplate {
  // Validate subgraph variable mappings
  for (const node of this.nodes.values()) {
    if (node.type === NodeType.SUBGRAPH) {
      const config = node.config as SubgraphNodeConfig;
      
      // Check that variableInputs reference valid parent variables
      if (config.variableInputs) {
        for (const input of config.variableInputs) {
          const parentVar = this.variables.find(v => v.name === input.externalName);
          if (!parentVar && input.required && input.defaultValue === undefined) {
            throw new ValidationError(
              `Subgraph '${node.id}' requires variable '${input.externalName}' which is not defined in parent workflow`
            );
          }
        }
      }
    }
    
    if (node.type === NodeType.START) {
      const config = node.config as StartNodeConfig;
      
      // Validate START node variable declarations
      if (config.variableInputs) {
        // Ensure no duplicate internal names
        const internalNames = new Set(config.variableInputs.map(i => i.internalName));
        if (internalNames.size !== config.variableInputs.length) {
          throw new ValidationError(
            `START node has duplicate internal variable names`
          );
        }
      }
    }
  }
  
  // Rest of build logic...
}
```

---

### Phase 5: 测试用例更新

需要更新的测试文件:
1. `sdk/__tests__/workflow/state-managers/variable-manager.test.ts`
2. `sdk/__tests__/workflow/execution/handlers/subgraph-handler.test.ts`
3. `sdk/__tests__/workflow/execution/handlers/loop-start-handler.test.ts`
4. 所有涉及 subgraph/loop 的集成测试

---

## 🚀 迁移检查清单

- [ ] VariableManager 移除 scope stacks
- [ ] VariableManager 简化为 global + execution 两层
- [ ] Subgraph handler 实现显式映射
- [ ] Loop handler 实现显式映射
- [ ] Workflow builder 添加验证
- [ ] 更新所有测试用例
- [ ] 更新文档和示例
- [ ] 运行完整测试套件

---

## ⚠️ Breaking Changes

1. **删除的方法**:
   - `VariableManager.enterSubgraphScope()`
   - `VariableManager.exitSubgraphScope()`
   - `VariableManager.enterLoopScope()`
   - `VariableManager.exitLoopScope()`

2. **新增的配置字段**:
   - `SubgraphNodeConfig.variableInputs`
   - `SubgraphNodeConfig.variableOutputs`
   - `StartNodeConfig.variableInputs`
   - `StartNodeConfig.variableOutputs`
   - `LoopStartNodeConfig.variableInputs`

3. **行为变化**:
   - Subgraph 不再自动继承父工作流变量
   - Loop 不再自动继承父工作流变量
   - 必须显式声明所有跨作用域的变量传递

---

## 📚 示例对比

### Before (Implicit)

```toml
# Parent workflow
[[variables]]
name = "apiKey"
type = "string"
value = "secret123"

[[nodes]]
id = "subgraph"
type = "SUBGRAPH"
config = { subgraphId = "child-wf" }

# Child workflow (can implicitly access apiKey)
[[nodes]]
id = "start"
type = "START"

[[nodes]]
id = "use_api"
type = "SCRIPT"
config = { code = "console.log(apiKey)" }  # Works but unclear!
```

### After (Explicit)

```toml
# Parent workflow
[[variables]]
name = "apiKey"
type = "string"
value = "secret123"

[[nodes]]
id = "subgraph"
type = "SUBGRAPH"

[nodes.config]
subgraphId = "child-wf"

[[nodes.config.variableInputs]]
externalName = "apiKey"
internalName = "api_key"
required = true

# Child workflow (must declare inputs)
[[nodes]]
id = "start"
type = "START"

[[nodes.config.variableInputs]]
externalName = "apiKey"
internalName = "api_key"
required = true

[[nodes]]
id = "use_api"
type = "SCRIPT"
config = { code = "console.log(api_key)" }  # Clear and explicit!
```

---

## 🎯 结论

本次改造将显著提升工作流系统的:
1. **可维护性**: 清晰的接口契约,易于理解和重构
2. **安全性**: 防止敏感数据意外泄漏
3. **可测试性**: 可以独立测试子工作流
4. **性能**: 简化 VariableManager,减少不必要的 scope 检查

虽然需要一定的迁移成本,但长期收益远大于短期投入。
