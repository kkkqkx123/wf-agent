# ROUTE 节点条件增强方案分析

## 执行概述

针对 `route-node-condition-enhancement.md` 中提出的条件类型扩展，分析是否应该**扩展现有evaluator** 还是 **直接按照新方案执行**。

**结论：推荐混合方案 — 采用策略模式+分层架构，而非单纯扩展现有evaluator**

---

## 现有架构分析

### 当前设计（expression-only）

```
Condition (expression: string)
  ↓
ConditionEvaluator.evaluate()
  ├─ 表达式编译 (expressionCompiler)
  ├─ 依赖检查 (path validation)
  └─ 表达式执行 (expressionEvaluator)
```

**关键特性：**
- 只支持字符串表达式
- 内置DSL编译（lexer → parser → AST）
- 依赖路径预检查和验证
- 性能优化：缓存编译结果

### 相关文件关系

```
types/src/condition.ts          ← 仅定义 expression: string
  ↓
workflow/evaluation/condition-evaluator.ts    ← 处理所有条件逻辑
  ├─ expression-evaluator.ts   ← 表达式运行时
  ├─ expression-compiler.ts    ← 表达式编译
  └─ path-resolver.ts          ← 路径解析
  
execution/handlers/route-handler.ts  ← 使用条件评估器
  └─ evaluateRouteCondition() 内部调用
```

---

## 两种方案对比

### 方案 A：扩展现有Evaluator

**做法：** 在 `ConditionEvaluator` 中添加条件类型判别逻辑

```typescript
// condition.ts 中扩展
export type Condition = 
  | ExpressionCondition         // 现有
  | PredicateCondition          // 新增
  | ScriptCondition             // 新增
  | SchemaCondition             // 新增

// condition-evaluator.ts 中扩展
evaluate(condition: Condition, context: EvaluationContext): boolean {
  if (condition.type === 'expression') {
    // 现有逻辑
  } else if (condition.type === 'predicate') {
    // 谓词逻辑
  } else if (condition.type === 'script') {
    // 脚本逻辑
  } else if (condition.type === 'schema') {
    // Schema验证逻辑
  }
}
```

**优点：**
- ✅ 改动集中，单一入口点
- ✅ 保持 API 不变（evaluator.evaluate 仍是主接口）
- ✅ 向后兼容性简单（expression 为默认/空缺时的类型）

**缺点：**
- ❌ ConditionEvaluator 职责过重（单一职责原则违反）
- ❌ 逻辑复杂性指数增长：
  - 脚本条件需要沙箱隔离（新增安全层）
  - Schema条件需要ajv集成（新增库依赖）
  - 谓词条件的通用化逻辑（新增类型分发）
- ❌ 测试变得复杂：一个大evaluator的测试会覆盖多种场景
- ❌ 难以优化：不同条件类型的缓存策略不同
  - 脚本：需要缓存编译的Function
  - Schema：需要缓存ajv验证器
  - 表达式：已有编译缓存
- ❌ 难以扩展：添加第5种条件类型时，需要修改同一个大文件

---

### 方案 B：按计划执行 + 策略模式

**做法：** 为每种条件类型创建独立的评估器，通过统一接口分发

```
Condition (discriminated union: type + payload)
  ↓
UnifiedConditionEvaluator (分发器)
  ├─ ExpressionConditionEvaluator
  │  └─ expressionCompiler + expressionEvaluator
  ├─ PredicateConditionEvaluator
  │  └─ 单变量谓词检查
  ├─ ScriptConditionEvaluator
  │  └─ VM 沙箱 + 脚本执行
  └─ SchemaConditionEvaluator
     └─ ajv Schema 验证
```

**实施步骤：**

1. **扩展类型定义** (`packages/types/src/condition.ts`)
   ```typescript
   export type Condition = 
     | { type: 'expression'; expression: string; metadata?: Metadata }
     | { type: 'predicate'; predicateType: PredicateType; variable: string; metadata?: Metadata }
     | { type: 'script'; script: string; metadata?: Metadata }
     | { type: 'schema'; variable: string; schema: JSONSchema; metadata?: Metadata }
   ```

2. **创建专用评估器** (创建新文件 `sdk/workflow/evaluation/evaluators/`)
   ```
   evaluators/
   ├─ expression-condition-evaluator.ts   ← 现有逻辑移入
   ├─ predicate-condition-evaluator.ts    ← 新增
   ├─ script-condition-evaluator.ts       ← 新增（沙箱隔离）
   └─ schema-condition-evaluator.ts       ← 新增（ajv）
   ```

3. **统一分发器** (`condition-evaluator.ts` 重构为分发器)
   ```typescript
   export class ConditionEvaluator {
     private expressionEvaluator = new ExpressionConditionEvaluator()
     private predicateEvaluator = new PredicateConditionEvaluator()
     private scriptEvaluator = new ScriptConditionEvaluator()
     private schemaEvaluator = new SchemaConditionEvaluator()

     evaluate(condition: Condition, context: EvaluationContext): boolean {
       switch (condition.type) {
         case 'expression': return this.expressionEvaluator.evaluate(...)
         case 'predicate': return this.predicateEvaluator.evaluate(...)
         // ...
       }
     }
   }
   ```

4. **RouteHandler** 无需改动（接口兼容）

**优点：**
- ✅ **单一职责**：每个evaluator只处理一种条件
- ✅ **易于扩展**：新条件类型 = 新文件 + 分发器增加一个case
- ✅ **独立测试**：每个条件评估器有独立的测试用例文件
- ✅ **优化灵活**：
  - 脚本evaluator可自建Function缓存
  - Schema evaluator可管理ajv实例缓存
  - 表达式evaluator保持现有优化
- ✅ **安全隔离**：脚本沙箱逻辑完全隔离在ScriptConditionEvaluator
- ✅ **依赖管理清晰**：
  - 若不用脚本条件，ScriptConditionEvaluator可延迟加载或mock
  - ajv仅由SchemaConditionEvaluator导入

**缺点：**
- ⚠️ 初期代码量增加（但结构清晰）
- ⚠️ 需要修改Condition类型定义（但向后兼容可保证）

---

## 架构对比图

### 方案 A（扩展）
```
ConditionEvaluator (大)
├─ expression logic (复杂)
├─ predicate logic (新增)
├─ script logic (新增 + 安全问题)
└─ schema logic (新增 + 依赖问题)
   └─ 单一文件 → 快速膨胀
```

### 方案 B（策略分层）
```
UnifiedConditionEvaluator (分发 20行)
├─ ExpressionConditionEvaluator (现有逻辑)
├─ PredicateConditionEvaluator (独立)
├─ ScriptConditionEvaluator (独立 + 沙箱)
└─ SchemaConditionEvaluator (独立 + ajv)
   └─ 每个可独立测试、优化、发布
```

---

## 实施建议

### 推荐方案：方案 B（策略模式）

**原因：**
1. **项目阶段**：AGENTS.md 明确"No backward-compatible"，不需要过度保守
2. **长期收益**：架构清晰，便于后续维护和扩展
3. **团队协作**：多个评估器可并行开发和测试
4. **性能**：可为不同条件类型实施最优化策略

### 实施路径

**Phase 1：类型和分层**（优先级：高）
- [ ] 扩展 `Condition` 类型（discriminated union）
- [ ] 创建专用评估器接口：`ConditionEvaluator` interface
- [ ] 提取表达式评估逻辑到 `ExpressionConditionEvaluator`
- [ ] 保持现有 `ConditionEvaluator` 作为分发器（与route-handler兼容）

**Phase 2：新条件类型** （优先级：中）
- [ ] `PredicateConditionEvaluator`（最简单，风险最低）
- [ ] `SchemaConditionEvaluator`（依赖ajv）
- [ ] `ScriptConditionEvaluator`（涉及沙箱安全）

**Phase 3：优化和文档** （优先级：低）
- [ ] 缓存策略优化（per-evaluator）
- [ ] 性能基准测试
- [ ] 文档和示例

---

## 向后兼容性处理

```typescript
// 旧形式（保持兼容）
const condition: Condition = { expression: "x > 5" }

// 新形式
const condition: Condition = { type: 'expression', expression: "x > 5" }

// 可在类型定义中支持两种（带默认值）
export interface Condition {
  type?: 'expression' | 'predicate' | 'script' | 'schema'  // 默认 'expression'
  expression?: string  // type === 'expression' 时必需
  // ...其他字段
}
```

---

## 风险评估

### 方案 A（扩展）的风险
| 风险 | 影响 | 缓解 |
|------|------|------|
| ConditionEvaluator快速膨胀 | 维护困难 | 无法根本缓解 |
| 脚本沙箱安全边界不清 | 安全漏洞 | 在大evaluator中难以隔离 |
| ajv依赖耦合 | 部署体积 | 无法灵活处理 |

### 方案 B（分层）的风险
| 风险 | 影响 | 缓解 |
|------|------|------|
| 代码初期增加 | 开发周期 | 结构清晰，总量有限 |
| 类型定义复杂 | 易用性 | DSL或生成工具 |

**方案 B 风险可控，方案 A 的长期风险更大。**

---

## 优先级建议

1. **立即**：选择方案 B
2. **Phase 1**：实施类型+分发器（预计 2-3 工作日）
3. **Phase 2**：按优先级实施条件评估器
   - Predicate first（简单，快速获得收益）
   - Schema next（常见场景）
   - Script last（复杂，需要安全评审）

---

## 相关设计文档

- `route-node-condition-enhancement.md` - 原始需求计划
- 本文档 - 实施方案分析
- （待创建）`condition-evaluator-migration.md` - 迁移指南

