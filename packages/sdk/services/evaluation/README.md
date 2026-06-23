# Evaluation Service Module

条件评估服务模块提供了基于 DSL 的表达式解析、编译和执行能力，支持复杂的条件评估场景。

## 架构

```
services/evaluation/
├── dsl/                     # 表达式解析层
│   ├── condition-lexer.ts  # 词法分析
│   ├── condition-parser.ts # 语法分析（Chevrotain）
│   ├── condition-cst-to-ast.ts # CST 转 AST
│   ├── tokens.ts           # Token 定义
│   ├── types.ts            # AST 类型
│   └── index.ts
├── compilers/               # 编译层
│   ├── expression-compiler.ts   # 表达式编译
│   ├── predicate-compiler.ts    # 谓词编译
│   ├── schema-compiler.ts       # JSON Schema 编译
│   ├── script-compiler.ts       # 脚本编译
│   └── index.ts
├── executors/               # 执行层
│   ├── expression-condition-executor.ts
│   ├── predicate-executor.ts
│   ├── schema-executor.ts
│   ├── script-executor.ts
│   └── index.ts
├── shared/                  # 通用工具
│   ├── path-resolver.ts    # 路径解析
│   ├── security-validator.ts # 安全验证
│   └── index.ts
├── condition-evaluator.ts  # 统一入口
├── cache-manager.ts        # 缓存管理
├── base-executor.ts        # 基础类
├── types/                  # 类型定义
└── index.ts
```

## 分层设计

### 1. DSL 层（解析）
**职责**：将字符串表达式解析成抽象语法树 (AST)

实现：
- **词法分析** (Lexer) - 字符串 → Token 流
- **语法分析** (Parser) - Token 流 → 具体语法树 (CST)
- **AST 转换** - CST → 抽象语法树

**特点**：
- 基于 Chevrotain 词法分析库
- 完整的语义验证
- 清晰的错误消息

**支持的表达式**：
```
// 字面量
42, "hello", true, [1, 2, 3]

// 变量和成员访问
x, obj.property, arr[0], obj.method()

// 运算符
+ - * / % ** == != < > <= >= && || !

// 三元表达式
x > 0 ? "positive" : "non-positive"

// 函数调用
Math.max(a, b), Math.sqrt(x)
```

### 2. 编译层（优化）
**职责**：将 AST 编译成可执行的代码或优化形式

实现：
- **ExpressionCompiler** - JavaScript 表达式编译
- **PredicateCompiler** - 布尔谓词编译
- **SchemaCompiler** - JSON Schema 验证编译
- **ScriptCompiler** - 脚本编译

**特点**：
- 提取依赖项 (dependencies)
- 生成优化的执行函数
- 缓存编译结果

### 3. 执行层（运行）
**职责**：在给定上下文中执行编译后的代码

实现：
- **ExpressionConditionExecutor** - 执行表达式
- **PredicateExecutor** - 执行谓词
- **SchemaExecutor** - 验证数据
- **ScriptExecutor** - 执行脚本

**特点**：
- 隔离执行环境
- 超时保护
- 详细的错误信息

### 4. 统一 API（ConditionEvaluator）
**职责**：提供统一的条件评估接口

支持的条件类型：
```typescript
type ConditionType = 'expression' | 'predicate' | 'schema' | 'script'

await conditionEvaluator.evaluate({
  type: 'expression',           // 条件类型
  expression: 'x > 10',         // 表达式内容
}, context)
```

## 依赖关系

```
ConditionEvaluator（统一入口）
    ↓ (路由到对应执行器)
├── ExpressionConditionExecutor
│   ├── ExpressionCompiler (DSL → 代码)
│   └── shared/security-validator
├── PredicateExecutor
│   ├── PredicateCompiler
│   └── shared/path-resolver
├── SchemaExecutor
│   ├── SchemaCompiler
│   └── Zod schema validation
└── ScriptExecutor
    └── ScriptCompiler

CacheManager（缓存层）
    ↓ (可选加速)
所有执行器
```

## 使用示例

### 简单表达式评估

```typescript
import { conditionEvaluator } from '@sdk/services/evaluation'

const result = await conditionEvaluator.evaluate(
  { type: 'expression', expression: 'x > 10' },
  { x: 20 }
)
console.log(result) // true
```

### 谓词评估

```typescript
const result = await conditionEvaluator.evaluate(
  {
    type: 'predicate',
    predicate: {
      field: 'status',
      operator: 'equals',
      value: 'active'
    }
  },
  { status: 'active' }
)
// true
```

### JSON Schema 验证

```typescript
const result = await conditionEvaluator.evaluate(
  {
    type: 'schema',
    schema: {
      type: 'object',
      properties: {
        email: { type: 'string', format: 'email' }
      }
    }
  },
  { email: 'user@example.com' }
)
// true
```

### 缓存优化

```typescript
import { cacheManager } from '@sdk/services/evaluation'

// 预编译并缓存
const compiled = expressionCompiler.compile('x * 2')
cacheManager.setCachedResult('double_x', undefined, ['x'], { x: 5 })

// 后续评估速度更快
const result = await conditionEvaluator.evaluate(
  { type: 'expression', expression: 'x * 2' },
  { x: 5 },
  'double_x'  // 使用缓存键
)
```

## 安全特性

1. **路径验证** - 防止访问不允许的属性
   ```typescript
   validatePath('user.email', context)  // ✓ OK
   validatePath('__proto__', context)   // ✗ Blocked
   ```

2. **数组索引验证** - 防止数组越界
   ```typescript
   validateArrayIndex(arr, 5)
   ```

3. **类型检查** - 运行时类型验证
   ```typescript
   validateValueType(value, 'number')
   ```

4. **超时保护** - 防止无限循环（外层实现）

## 性能优化

1. **编译缓存** - 减少重复编译
2. **依赖追踪** - 识别表达式的依赖变量
3. **增量更新** - 仅当依赖变化时重新评估

## 扩展点

### 添加自定义执行器

```typescript
import { BaseExecutor } from '@sdk/services/evaluation'

export class CustomExecutor extends BaseExecutor {
  async execute(condition: any, context: EvaluationContext): Promise<unknown> {
    // 实现自定义逻辑
    return true
  }
}
```

### 添加自定义编译器

```typescript
import type { ICompiler } from '@sdk/services/evaluation'

export class CustomCompiler implements ICompiler {
  compile(input: any): any {
    // 实现编译逻辑
    return { /* 编译结果 */ }
  }
}
```

## 设计原则

1. **分离关注** - DSL 解析、编译、执行各自独立
2. **可扩展** - 容易添加新的条件类型和执行器
3. **高性能** - 编译缓存和依赖追踪
4. **类型安全** - 完整的 TypeScript 支持
5. **安全第一** - 内置多层安全验证
