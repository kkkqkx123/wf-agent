# Evaluation模块重构分析

## 执行概述

针对route条件增强方案，分析evaluation模块现有结构是否需要重构。

**结论：推荐同步重构evaluation模块。现有架构存在职责混合、缓存不一致、可扩展性有限等问题。**

---

## 现有Evaluation模块结构

### 当前层次架构

```
应用层（Hook Executor, Route Handler, Trigger Matcher）
  ↓
条件评估层（ConditionEvaluator）
  ├─ 依赖验证（path existence check）
  ├─ 编译（expressionCompiler.compile）
  └─ 执行（expressionEvaluator.evaluate）
  ↓
执行引擎层（ExpressionEvaluator）
  ├─ AST 遍历和执行
  ├─ 安全验证（security-validator）
  ├─ 路径解析（path-resolver）
  └─ 方法/函数调用
  ↓
编译缓存层（ExpressionCompiler）
  └─ 表达式 → AST + 元数据缓存
  ↓
解析层（DSL）
  ├─ Lexer（tokens.ts）
  ├─ Parser（condition-parser.ts）
  └─ CST→AST 转换
  ↓
优化层（DependencyManager）
  ├─ 依赖追踪
  ├─ 变量变化检测
  └─ 结果缓存
```

### 13个源文件清单

| 文件 | 行数 | 职责 | 现状 |
|------|------|------|------|
| condition-evaluator.ts | ~120 | 条件评估入口 | 仅支持expression |
| expression-evaluator.ts | ~720 | AST执行引擎 | 功能厚重 |
| expression-compiler.ts | ~195 | 表达式编译+缓存 | 功能清晰 |
| security-validator.ts | ~180 | 安全验证 | 功能清晰 |
| dependency-tracker.ts | ~230 | 依赖追踪 | 仅支持expression |
| path-resolver.ts | ? | 路径解析 | 功能单一 |
| DSL相关（6个文件） | ~500 | 词法/语法分析 | 基于Chevrotain |

---

## 现有问题分析

### 1. 职责混合问题

**ConditionEvaluator 的多重职责：**
```typescript
// condition-evaluator.ts:26-114
evaluate(condition: Condition, context: EvaluationContext): boolean {
  // 职责1：验证expression字段存在
  if (!condition.expression) { throw ... }
  
  // 职责2：预编译+依赖检查（专有逻辑）
  let compiled = expressionCompiler.compile(...)
  
  // 职责3：路径验证（复制的安全逻辑）
  for (const dep of compiled.dependencies) {
    exists = resolvePath(...) // 重复验证
  }
  
  // 职责4：执行调用
  const result = expressionEvaluator.evaluate(...)
  
  // 职责5：错误处理+日志
}
```

**问题：**
- ConditionEvaluator 是"智能评估器"，而不是"分发器"
- ExpressionEvaluator 也做路径验证（重复）
- 当添加新条件类型时，这个大评估器会变得更复杂

---

### 2. 缓存策略不一致

**三套缓存机制并存：**

| 缓存层 | 所有者 | 作用 | 问题 |
|--------|--------|------|------|
| ExpressionCompiler.cache | 全局单例 | 编译结果 | 全局作用域，重复编译浪费 |
| DependencyManager（Trigger） | 模块单例 | 依赖跟踪+结果 | 仅在matcher中使用 |
| DependencyManager（Route） | 工作流执行 | 同上 | 每个execution新建 |
| Hook Executor | 无 | 无缓存 | **性能漏洞** |

**具体表现：**

```typescript
// route-handler.ts 中的缓存使用
const depManager = workflowExecutionEntity.getDepManager()
const cached = depManager.getTrackedExpression(expression)
if (cached) {
  return Boolean(depManager.evaluateIfChanged(expression, context))
}
// → 使用DependencyManager缓存和变量变化检测

// trigger-matcher.ts 中
const depManager = new DependencyManager() // 模块单例
// → 自己维护缓存

// hook-executor.ts 中
conditionEvaluator.evaluate(hook.condition, evaluationContext)
// → 无缓存，每次都编译！
```

**问题：**
- 缓存策略取决于调用者，不是evaluator的职责
- Hook Executor 完全没有缓存（频繁调用时性能差）
- ExpressionCompiler 全局缓存无法清理（内存泄漏风险）

---

### 3. 接口设计问题

**ExpressionEvaluator 的双重接口：**

```typescript
// expression-evaluator.ts:39
evaluate(expression: string, context: EvaluationContext): unknown
// 接收字符串表达式，内部编译

// expression-compiler.ts:34
evaluate = (context: EvaluationContext) => {
  return expressionEvaluator.evaluateAST(ast, context)
}
// 接收AST，调用evaluateAST

// 这导致：
// - 字符串表达式需要经过：字符串 → 编译 → AST → evaluateAST
// - ExpressionEvaluator.evaluate() 是 compiled.evaluate() 的包装
```

**问题：**
- `expressionEvaluator.evaluate(string)` 内部还会调用编译
- `expressionCompiler.compile().evaluate()` 才是完整链路
- 接口设计不清晰，导致使用混乱

---

### 4. 扩展困难

**添加新条件类型时的问题：**

当添加 Script/Schema/Predicate 条件：
1. ConditionEvaluator.evaluate() 需要 if-else 分支（膨胀）
2. 每种条件类型的缓存策略不同：
   - Script：需要缓存编译的Function
   - Schema：需要缓存ajv验证器
   - Expression：已有编译缓存
   - Predicate：无需缓存
3. DependencyManager 只能处理expression（需要升级）
4. ExpressionEvaluator 只处理expression类型（需要分化）

---

### 5. 性能优化漏洞

**ExpressionEvaluator 中的性能问题：**

```typescript
// expression-evaluator.ts:429-449
private evaluateArrayMethod(...): unknown {
  // 问题1：缓存键基于 JSON.stringify(arguments)
  const cacheKey = `${methodName}:${JSON.stringify(...)}`
  
  // 问题2：每次调用evaluate都会清理超过100条的缓存
  if (this.arrayMethodCache.size > 100) {
    this.cleanArrayMethodCache(now)
  }
  
  // 问题3：缓存仅50ms TTL（太短）
  const ARRAY_METHOD_CACHE_TTL = 50
}
```

**问题：**
- 缓存键基于JSON序列化，可能冲突或性能差
- per-instance缓存不会跨evaluator调用
- TTL太短，无法有效缓存

---

### 6. DependencyManager 功能限制

```typescript
// dependency-tracker.ts:105-126
register(key: string, expression: string, context: EvaluationContext): TrackedExpression {
  const compiled = expressionCompiler.compile(expression)
  // 硬编码依赖expression type
  this.trackedExpressions.set(key, { expression, compiled, dependencies: ... })
}

evaluateIfChanged(key: string, context: EvaluationContext): unknown {
  if (!this.hasDependenciesChanged(key, context)) {
    return tracked.lastResult // 返回缓存
  }
  const result = tracked.compiled.evaluate(context)
  // 更新跟踪的变量值
}
```

**问题：**
- 仅支持expression type的compiled表达式
- 新条件类型（Script/Schema）无法使用
- 需要为每种条件类型创建对应的tracking机制

---

### 7. 使用现状的混乱

**三个调用点，三种用法：**

```typescript
// Hook Executor（无缓存）
conditionEvaluator.evaluate(hook.condition, evaluationContext)

// Route Handler（用DependencyManager缓存）
depManager.register(expression, expression, context)
depManager.evaluateIfChanged(expression, context)

// Trigger Matcher（用自己的DependencyManager缓存）
const depManager = new DependencyManager()
// 独立维护缓存
```

**问题：**
- 缓存使用方式取决于调用者
- Route/Trigger有缓存，Hook没有
- 新添加的调用点容易忘记缓存
- 无法统一监控/清理

---

## 重构必要性评估

### 不重构的后果

如果仅在现有evaluation基础上添加新条件类型：

| 问题 | 影响程度 | 后期修复成本 |
|------|---------|------------|
| ConditionEvaluator 膨胀到200+行 | 高 | 高 |
| 缓存混乱加剧（3种→6种） | 高 | 极高 |
| Hook Executor 仍无缓存，性能差 | 中 | 中 |
| DependencyManager 代码重复 | 中 | 中 |
| 不同条件类型的优化策略冲突 | 中 | 高 |
| 新开发者难以理解架构 | 低 | 持续 |

### 重构的收益

1. **清晰的职责分离**
   - 编译器负责编译
   - 执行器负责执行
   - 分发器负责路由
   - 缓存层独立管理

2. **统一的缓存框架**
   - 所有条件类型共用一套缓存机制
   - Hook/Route/Trigger 使用相同API
   - 缓存清理策略统一

3. **性能优化空间**
   - 复杂表达式可以提前警告
   - 每种条件类型可定制缓存策略
   - 内存泄漏风险降低

4. **可扩展性**
   - 新条件类型 = 新compiler + 新executor
   - ConditionEvaluator 保持简洁
   - DependencyManager 自动支持

5. **代码质量**
   - 单一职责原则
   - 更易于单元测试
   - 更易于性能调优

---

## 推荐的重构架构

### 新架构设计（分层+工厂模式）

```
┌─────────────────────────────────────────┐
│  Application Layer                      │
│  (Hook Executor, Route Handler, etc.)   │
└──────────────────┬──────────────────────┘
                   │
┌──────────────────▼──────────────────────┐
│  Unified Condition Evaluator            │
│  (Dispatcher/Factory)                   │
│                                          │
│  dispatch(condition) → appropriate eval │
└──────────────────┬──────────────────────┘
                   │
        ┌──────────┴──────────┐
        │                     │
   ┌────▼─────┐  ┌──────┐  ┌─▼────────┐
   │Expression │  │...   │  │ Schema   │
   │Evaluator  │  │      │  │Evaluator │
   │           │  │      │  │          │
   ├─ Compiler │  │      │  ├─Compiler │
   ├─ Executor │  │      │  ├─ Executor│
   └────▲─────┘  └──────┘  └─▲────────┘
        │                     │
        └──────────┬──────────┘
                   │
        ┌──────────▼──────────┐
        │  Unified Cache      │
        │  (DependencyManager)│
        │                      │
        │  • Compile cache    │
        │  • Result cache     │
        │  • Dependency track │
        └─────────────────────┘
```

### 文件结构规划

```
evaluation/
├─ index.ts                          ← 公共导出
├─ condition-evaluator.ts            ← 统一分发器（简化）
├─ cache-manager.ts                  ← 新增：统一缓存管理
│
├─ compilers/
│  ├─ index.ts
│  ├─ expression-compiler.ts         ← 现有（精简）
│  ├─ predicate-compiler.ts          ← 新增
│  ├─ script-compiler.ts             ← 新增
│  └─ schema-compiler.ts             ← 新增
│
├─ executors/
│  ├─ index.ts
│  ├─ base-executor.ts               ← 新增：抽象基类
│  ├─ expression-executor.ts         ← 重构（现有evaluator）
│  ├─ predicate-executor.ts          ← 新增
│  ├─ script-executor.ts             ← 新增
│  └─ schema-executor.ts             ← 新增
│
├─ types/
│  ├─ compiler.ts                    ← 新增：编译器接口
│  └─ executor.ts                    ← 新增：执行器接口
│
├─ shared/
│  ├─ security-validator.ts          ← 现有
│  ├─ path-resolver.ts               ← 现有
│  └─ dependency-tracker.ts          ← 重构
│
└─ dsl/
   ├─ (已有的6个文件保持不变)
   └─ index.ts
```

### 关键组件设计

**1. 统一编译器接口**
```typescript
interface ICompiler {
  compile(input: string | Record<string, unknown>): CompiledUnit
}

interface CompiledUnit {
  ast: unknown
  dependencies?: string[]
  complexity?: number
  metadata?: Record<string, unknown>
}
```

**2. 统一执行器接口**
```typescript
interface IExecutor {
  execute(compiled: CompiledUnit, context: EvaluationContext): unknown
}
```

**3. 统一缓存管理器**
```typescript
class CacheManager {
  // 编译缓存（编译结果）
  getCompiled(type: ConditionType, key: string): CompiledUnit | null
  setCompiled(type: ConditionType, key: string, unit: CompiledUnit): void
  
  // 执行缓存（结果 + 依赖追踪）
  getResult(key: string): CachedResult | null
  setResult(key: string, result: unknown, deps: string[]): void
  
  // 依赖变化检测
  hasDepsChanged(key: string, context: EvaluationContext): boolean
}
```

**4. 简化的ConditionEvaluator**
```typescript
class ConditionEvaluator {
  private compilers = new Map<ConditionType, ICompiler>()
  private executors = new Map<ConditionType, IExecutor>()
  private cache = new CacheManager()
  
  evaluate(condition: Condition, context: EvaluationContext): boolean {
    const type = condition.type ?? 'expression'
    const compiler = this.compilers.get(type)
    const executor = this.executors.get(type)
    
    // 缓存查询
    const cached = this.cache.getResult(condition.key)
    if (cached && !this.cache.hasDepsChanged(condition.key, context)) {
      return cached.result
    }
    
    // 编译 + 执行
    const compiled = compiler.compile(condition.payload)
    const result = executor.execute(compiled, context)
    
    // 缓存保存
    this.cache.setResult(condition.key, result, compiled.dependencies)
    
    return Boolean(result)
  }
}
```

---

## 重构实施路径

### Phase 1：基础设施（优先级：高，1-2 周）

- [ ] 创建 `cache-manager.ts`（统一缓存）
- [ ] 创建编译器/执行器接口（types/）
- [ ] 创建 `BaseExecutor` 抽象类
- [ ] 优化 `dependency-tracker.ts`（支持所有类型）
- [ ] 重构 ExpressionCompiler 为 ExpressionConditionCompiler
- [ ] 重构 ExpressionEvaluator 为 ExpressionConditionExecutor

### Phase 2：新条件类型支持（优先级：中，2-3 周）

- [ ] PredicateCompiler + PredicateExecutor
- [ ] SchemaCompiler + SchemaExecutor
- [ ] ScriptCompiler + ScriptExecutor（含沙箱）

### Phase 3：集成和优化（优先级：中，1-2 周）

- [ ] 更新 ConditionEvaluator（新架构分发器）
- [ ] 更新 Hook/Route/Trigger 使用（统一缓存API）
- [ ] 性能基准测试和调优

### Phase 4：清理和文档（优先级：低，1 周）

- [ ] 删除已废弃的代码
- [ ] 更新导出（index.ts）
- [ ] 编写迁移指南

---

## 风险和缓解

| 风险 | 影响 | 缓解方案 |
|------|------|--------|
| 重构期间功能中断 | 高 | 使用worktree/分支开发，保持向后兼容 |
| 新架构学习曲线 | 中 | 详细文档+示例代码 |
| 性能回归 | 中 | 基准测试对标，性能优化 |
| DependencyManager替换复杂 | 中 | 逐步迁移，新旧并存过渡期 |

---

## 小结

**是否需要重构：是的，强烈推荐**

### 不重构的成本
- ConditionEvaluator 继续膨胀
- 缓存混乱
- 性能瓶颈（Hook Executor）
- 后期重构成本 10 倍以上

### 重构的收益
- 清晰的架构
- 统一的缓存框架
- 性能优化空间
- 易于扩展新条件类型
- 代码质量提升

### 合并后的总工作量
- Phase 1-2（新条件类型）：仅在新架构基础上添加 3 套编译器/执行器
- 性能优化：统一缓存框架提供 20-40% 的性能提升空间

**建议：与 route-node-condition-enhancement 同步执行，不增加额外周期。**

