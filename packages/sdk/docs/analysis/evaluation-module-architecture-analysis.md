# Evaluation 模块架构分析报告

**分析日期**: 2026-06-20  
**分析对象**: `sdk/workflow/evaluation/` 模块  
**分析问题**: 该模块作为 workflow 子模块是否合理？是否需要改造为独立的 services 模块？

---

## 1. 当前架构现状

### 1.1 模块位置与结构

```
sdk/
├── workflow/
│   └── evaluation/              # 当前位置：workflow 的子模块
│       ├── compilers/           # 编译器层
│       ├── dsl/                 # 领域特定语言
│       ├── executors/           # 执行器层
│       ├── shared/              # 共享工具
│       ├── types/               # 类型定义
│       ├── condition-evaluator.ts       # 主入口
│       ├── cache-manager.ts             # 缓存管理
│       └── base-executor.ts             # 基础执行器
├── services/                    # 现有服务目录
│   ├── auto-approval/           # 自动批准服务
│   ├── command-safety/          # 命令安全检查
│   ├── executors/               # 执行器服务
│   ├── sandbox/                 # 沙箱运行时
│   ├── terminal/                # 终端服务
│   ├── tools/                   # 工具服务
│   └── ...
```

### 1.2 evaluation 模块的职责

**核心职责**：条件/表达式的编译和执行

1. **DSL（Domain Specific Language）**
   - 条件表达式的词法分析（tokenizer）
   - 语法解析（parser）
   - 抽象语法树生成（CST → AST）
   - 类型定义（15+ 表达式类型）

2. **编译器层（Compilers）**
   - `expression-compiler.ts` - 表达式编译
   - `predicate-compiler.ts` - 谓词编译
   - `schema-compiler.ts` - 模式编译
   - `script-compiler.ts` - 脚本编译
   - 支持 4 种条件类型编译

3. **执行器层（Executors）**
   - `base-executor.ts` - 基础执行器
   - `expression-condition-executor.ts` - 表达式执行
   - `predicate-executor.ts` - 谓词执行
   - `schema-executor.ts` - 模式执行
   - `script-executor.ts` - 脚本执行

4. **共享工具（Shared）**
   - `path-resolver.ts` - 路径解析
   - `security-validator.ts` - 安全验证
   - 支持 `variables/input/output` 访问

5. **缓存管理（Cache Manager）**
   - LRU 缓存策略（编译缓存 1000 条，执行缓存 5000 条）
   - 依赖变化检测
   - 缓存统计
   - 深度相等性检查

6. **统一入口（Condition Evaluator）**
   - 条件类型分发
   - 缓存管理集成
   - 错误处理和日志记录

### 1.3 模块依赖关系

**使用该模块的位置** (19 个文件):

```
核心使用场景：
├── workflow/execution/handlers/
│   ├── route-handler.ts          # 路由节点条件评估
│   ├── loop-end-handler.ts       # 循环结束条件
│   └── variable-operation-handlers.ts  # 变量操作
├── workflow/validation/
│   ├── route-validator.ts        # 路由节点验证
│   └── loop-end-validator.ts     # 循环结束验证
├── workflow/entities/
│   └── workflow-execution-entity.ts   # 执行实体（DependencyManager）
├── workflow/builder/
│   └── workflow-navigator.ts     # 工作流导航器
├── core/
│   ├── validation/
│   │   ├── trigger-validator.ts
│   │   └── hook-validator.ts
│   ├── hooks/
│   │   └── executor.ts
│   └── triggers/
│       └── matcher.ts
```

**依赖关系**：
- `@wf-agent/types` - 类型定义
- `@wf-agent/common-utils` - 日志记录
- `chevrotain` - 解析器库
- `zod` - 数据验证
- `lru-cache` - 缓存实现

### 1.4 现有 services 模块分析

| 服务 | 职责 | 独立性 | 复用度 |
|------|------|--------|--------|
| **terminal** | 终端会话管理 | 高 | 高 |
| **sandbox** | 代码沙箱执行 | 高 | 中 |
| **tools** | 工具执行框架 | 高 | 高 |
| **executors** | 执行器（CLI/远程） | 高 | 高 |
| **auto-approval** | 自动批准检查 | 中 | 中 |
| **command-safety** | 命令安全检查 | 中 | 中 |
| **skill-loader** | 技能加载器 | 高 | 中 |

**设计特点**：
- 职责清晰，功能独立
- 很少相互依赖，主要被上层使用
- 包含自己的 types、error、logger
- 有完整的初始化和配置机制

---

## 2. 架构对比分析

### 2.1 与 services 模块的相似性

| 特性 | evaluation | terminal | sandbox |
|-----|-----------|----------|---------|
| **职责单一** | ✓ | ✓ | ✓ |
| **可复用** | ✓ | ✓ | ✓ |
| **完整处理流程** | ✓ | ✓ | ✓ |
| **缓存管理** | ✓ | ✓ | 部分 |
| **错误处理** | ✓ | ✓ | ✓ |
| **日志系统** | ✓ | ✓ | ✓ |
| **单例导出** | ✓ | ✓ | ✓ |

### 2.2 与 services 模块的差异性

| 维度 | evaluation | services 模块 |
|------|-----------|--------------|
| **当前位置** | workflow 子模块 | 顶级 services 目录 |
| **逻辑关系** | 与 workflow 紧密耦合 | 与各模块松耦合 |
| **主要使用者** | workflow（70%）、core（30%） | 多个独立模块 |
| **数据结构** | DSL/AST（领域特定） | 通用数据结构 |
| **配置方式** | 简单单例 | 有配置对象 |
| **初始化** | 被动使用 | 主动初始化 |
| **概念绑定** | 与"条件评估"紧密 | 与"某种能力"相关 |

### 2.3 职责边界分析

**evaluation 模块的核心价值**：
```
输入: 条件定义（多种格式）
  ↓
[编译层] → 编译为可执行单元 + 依赖分析
  ↓
[执行层] → 在给定上下文中执行
  ↓
[缓存层] → 结果缓存 + 依赖变化检测
  ↓
输出: 布尔值 / 执行结果
```

**当前位置的问题**：
1. ✗ 放在 workflow 下隐含了"仅用于 workflow"的假设
2. ✗ 导入路径复杂（`../../evaluation`）
3. ✗ 不符合 SDK 的"services 是通用能力"的设计理念

**作为 services 的优势**：
1. ✓ 突出其作为"条件评估服务"的通用价值
2. ✓ 与 tools、terminal、sandbox 并列
3. ✓ 导入路径简化（`@wf-agent/sdk/services/evaluation`）
4. ✓ 未来便于独立版本控制或发布

---

## 3. 依赖树梳理

### 3.1 当前导入关系

**顶层直接导入**：
```typescript
import { conditionEvaluator } from "../../evaluation/index.js"
import { validateExpression } from "../../evaluation/index.js"
import { DependencyManager } from "../evaluation/index.js"
```

**深层导入**：
```typescript
import { dslParseWithErrors } from "../../evaluation/index.js"
import { expressionEvaluator, setArrayItemByKey } from "../../../evaluation/index.js"
```

**使用场景按层级**：
```
workflow (70% 使用)
├── execution/handlers/node-handlers/*
├── validation/node-validation/*
├── entities/workflow-execution-entity
└── builder/workflow-navigator

core (30% 使用)
├── validation/*
├── hooks/executor
└── triggers/matcher
```

### 3.2 搬迁后的导入变化

**当前**（workflow 下）：
```typescript
// 从 workflow 内
import { conditionEvaluator } from "../../evaluation/index.js"

// 从 core 
import { conditionEvaluator } from "../../workflow/evaluation/index.js"
```

**搬迁后**（services 下）：
```typescript
// 都使用统一路径
import { conditionEvaluator } from "@wf-agent/sdk/services/evaluation"
import { evaluationService } from "@wf-agent/sdk/services"
```

---

## 4. 设计方案评估

### 方案 A：保持现状（评分: 40/100）

**优点**：
- 无需迁移成本
- 与 workflow 概念绑定较强

**缺点**：
- ✗ 隐藏了模块的通用价值
- ✗ 导入路径复杂，跨越多层目录
- ✗ 与 SDK 设计理念不符（services 是通用能力）
- ✗ 难以在其他 SDK 使用场景中发现
- ✗ 未来扩展受限（如非 workflow 场景的条件评估）
- ✗ 文档引用困难（在哪个模块下查找？）

### 方案 B：移动到 services 目录（评分: 85/100）

**改造内容**：
1. 移动 `sdk/workflow/evaluation/` → `sdk/services/evaluation/`
2. 更新 package.json 导出
3. 更新所有导入语句（19 处）
4. 更新文档和 README

**优点**：
- ✓ 突出模块的通用价值
- ✓ 与其他 services 模块并列，地位清晰
- ✓ 导入路径统一简化
- ✓ 符合 SDK 整体架构理念
- ✓ 便于文档组织和发现
- ✓ 支持未来的独立扩展
- ✓ 提高代码复用性和认知清晰度

**缺点**：
- 需要更新 19 处导入语句
- 需要调整模块初始化路径
- 一次性迁移成本（低）

**影响范围**：
```
需要更新的文件: 19 个
最大层级变化: 相对路径从 ../../ 变为统一的 @wf-agent/sdk/services
破坏性: 低（只改变导入路径，逻辑不变）
兼容性: 可通过 re-export 保持短期兼容
```

### 方案 C：分离为独立包（评分: 60/100）

**创建**：`packages/condition-evaluator/`

**优点**：
- ✓ 最高的独立性
- ✓ 可单独发布和版本控制
- ✓ 清晰的依赖边界

**缺点**：
- ✗ 增加仓库复杂性
- ✗ 内部 SDK 工具不需要这个级别的独立性
- ✗ 跨包导入增加配置复杂性
- ✗ 对 SDK 内部使用者来说反而增加复杂性

---

## 5. 迁移成本分析

### 5.1 需要更新的文件清单

| 文件 | 迁移成本 | 优先级 |
|------|--------|--------|
| workflow/execution/handlers/route-handler.ts | 低 | P1 |
| workflow/execution/handlers/loop-end-handler.ts | 低 | P1 |
| workflow/execution/handlers/variable-handler.ts | 低 | P1 |
| workflow/execution/handlers/variable-operation-handlers.ts | 低 | P1 |
| workflow/validation/node-validation/route-validator.ts | 低 | P1 |
| workflow/validation/node-validation/loop-end-validator.ts | 低 | P1 |
| workflow/entities/workflow-execution-entity.ts | 低 | P1 |
| workflow/builder/workflow-navigator.ts | 低 | P1 |
| workflow/execution/coordinators/workflow-execution-coordinator.ts | 低 | P1 |
| core/validation/trigger-validator.ts | 低 | P2 |
| core/validation/hook-validator.ts | 低 | P2 |
| core/hooks/executor.ts | 低 | P2 |
| core/triggers/matcher.ts | 低 | P2 |
| 单元测试文件（11 个） | 低 | P2 |

**总体成本估算**：
- 代码迁移: 1-2 小时
- 测试验证: 1-2 小时
- 文档更新: 30 分钟
- **总计: 2.5-4.5 小时**

### 5.2 迁移步骤

```
步骤 1: 创建新目录和移动文件
  sdk/services/evaluation/ ← sdk/workflow/evaluation/

步骤 2: 更新导出
  sdk/services/index.ts (添加 evaluation 导出)
  sdk/index.ts (验证导出)

步骤 3: 更新导入语句
  19 个文件的相对路径 → 统一路径
  
步骤 4: 验证和测试
  pnpm build
  pnpm test

步骤 5: 文档更新
  README.md 中添加 evaluation service
  导入示例文档
```

---

## 6. 推荐结论

### 建议: **采用方案 B（移动到 services 目录）**

**理由总结**：

1. **架构理念** - evaluation 模块的设计完全符合 services 的定义：
   - 通用的条件评估能力
   - 独立的编译→执行→缓存流程
   - 可被多个模块复用

2. **符合 SDK 设计**：
   ```
   services/ = 通用能力层
   ├── terminal/     # 终端能力
   ├── sandbox/      # 沙箱能力
   ├── tools/        # 工具能力
   ├── evaluation/   # ← 条件评估能力 (推荐位置)
   └── ...
   ```

3. **长期收益**：
   - 提高代码可发现性
   - 简化导入路径
   - 支持未来扩展
   - 提高代码复用度

4. **迁移低风险**：
   - 只改变导入路径，无逻辑改动
   - 代码本身保持完全不变
   - 迁移成本仅 3-4 小时
   - 不影响现有功能

5. **未来可达性**：
   - 便于新开发者发现条件评估服务
   - 便于与其他外部 SDK 集成
   - 便于编写 SDK 使用文档

### 实施优先级

| 阶段 | 任务 | 预期时间 |
|------|------|--------|
| **第一阶段** | 代码迁移 + 导入更新 | 1-2 小时 |
| **第二阶段** | 测试验证 + bug fix | 1-2 小时 |
| **第三阶段** | 文档更新 + 发布说明 | 30 分钟 |
| **验收** | 构建和端到端测试 | 1 小时 |

---

## 7. 参考架构

### 迁移后的目录结构

```
sdk/
├── services/
│   ├── evaluation/                  # ← 新位置
│   │   ├── compilers/
│   │   ├── dsl/
│   │   ├── executors/
│   │   ├── shared/
│   │   ├── types/
│   │   ├── condition-evaluator.ts
│   │   ├── cache-manager.ts
│   │   ├── index.ts
│   │   └── __tests__/
│   ├── terminal/
│   ├── sandbox/
│   ├── tools/
│   └── index.ts                    # 包含 evaluation 导出
├── workflow/
│   ├── execution/
│   ├── validation/
│   ├── entities/
│   ├── builder/
│   └── index.ts                    # evaluation 的导入改为 re-export
└── index.ts                        # 最终统一导出
```

### 导入示例

**SDK 内部使用**（推荐）：
```typescript
import { conditionEvaluator } from "@wf-agent/sdk/services/evaluation"
import type { Condition, EvaluationContext } from "@wf-agent/types"
```

**应用层使用**（through public API）：
```typescript
import { evaluationService } from "@wf-agent/sdk"
```

---

## 附录 A：完整文件清单

### 需要移动的文件

```
sdk/workflow/evaluation/
├── __tests__/
│   ├── cache-manager.test.ts
│   ├── condition-evaluator.test.ts
│   ├── expression-condition-executor.test.ts
│   ├── path-resolver.test.ts
│   ├── predicate-executor.test.ts
│   └── security-validator.test.ts
├── compilers/
│   ├── expression-compiler.ts
│   ├── index.ts
│   ├── predicate-compiler.ts
│   ├── schema-compiler.ts
│   └── script-compiler.ts
├── dsl/
│   ├── __tests__/
│   │   └── dsl.test.ts
│   ├── condition-cst-to-ast.ts
│   ├── condition-lexer.ts
│   ├── condition-parser.ts
│   ├── index.ts
│   ├── tokens.ts
│   └── types.ts
├── executors/
│   ├── expression-condition-executor.ts
│   ├── index.ts
│   ├── predicate-executor.ts
│   ├── schema-executor.ts
│   └── script-executor.ts
├── shared/
│   ├── index.ts
│   ├── path-resolver.ts
│   └── security-validator.ts
├── types/
│   ├── compiler.ts
│   ├── executor.ts
│   └── index.ts
├── base-executor.ts
├── cache-manager.ts
├── condition-evaluator.ts
└── index.ts
```

---

## 附录 B：迁移检查清单

- [ ] 创建 `sdk/services/evaluation/` 目录
- [ ] 复制所有文件
- [ ] 删除 `sdk/workflow/evaluation/` 目录
- [ ] 更新 `sdk/services/index.ts` (添加导出)
- [ ] 更新 `sdk/package.json` (verification)
- [ ] 更新 19 个文件的导入语句
- [ ] 更新 11 个测试文件的导入语句
- [ ] 运行 `pnpm build`
- [ ] 运行 `pnpm test`
- [ ] 运行类型检查 `pnpm typecheck`
- [ ] 更新 README.md
- [ ] 创建迁移说明文档
- [ ] 代码审查
- [ ] 合并到 main

