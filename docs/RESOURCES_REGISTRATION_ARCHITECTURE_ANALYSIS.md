# Resources 注册架构分析

## 当前架构概览

### 目录结构

```
packages/sdk/resources/
├── predefined/
│   ├── tools/
│   │   ├── registration.ts          (registerPredefinedTools)
│   │   ├── registry.ts
│   │   └── ...
│   ├── trigger/
│   │   ├── registration.ts          (registerPredefinedTriggers)
│   │   ├── registry.ts
│   │   └── ...
│   ├── workflow/
│   │   ├── registration.ts          (registerPredefinedWorkflows)
│   │   ├── registry.ts
│   │   └── ...
│   ├── prompts/
│   │   └── ...
│   ├── registration.ts              (registerPredefinedContent - 协调器)
│   ├── index.ts
│   └── presets-types.ts
├── custom/
│   ├── registration.ts              (registerCustomResources)
│   ├── loader.ts
│   ├── types.ts
│   └── index.ts
├── registration/                    (统一注册接口)
│   ├── orchestrator.ts              (registerAllResources)
│   ├── types.ts
│   └── index.ts
├── dynamic/
│   └── ...
└── index.ts                         (总导出)
```

### 注册流程

```
registerAllResources (registration/orchestrator.ts)
  ├─ 动态导入 registerPredefinedContent (predefined/registration.ts)
  │  └─ 协调三个子模块的注册
  │     ├─ registerPredefinedWorkflows (predefined/workflow/registration.ts)
  │     ├─ registerPredefinedTriggers (predefined/trigger/registration.ts)
  │     └─ registerPredefinedTools (predefined/tools/registration.ts)
  │
  └─ 动态导入 registerCustomResources (custom/registration.ts)
     └─ 直接实现三种资源的注册
        ├─ registerCustomTools
        ├─ registerCustomTriggers
        └─ registerCustomPrompts
```

## 存在的问题

### 1. 注册逻辑分散

**问题表现**：
- 注册逻辑分散在多个位置：
  - `predefined/registration.ts` - 协调层
  - `predefined/tools/registration.ts` - 具体实现
  - `predefined/trigger/registration.ts` - 具体实现
  - `predefined/workflow/registration.ts` - 具体实现
  - `custom/registration.ts` - 完整实现
  - `registration/orchestrator.ts` - 顶层协调

**影响**：
- 新增资源类型时需要在多个地方添加代码
- 维护者需要理解多层的注册流程
- 注册逻辑与资源创建逻辑混淆

### 2. 动态导入导致的问题

**代码示例** (`registration/orchestrator.ts` L69-71)：
```typescript
const { registerPredefinedContent } = await import(
  "../predefined/registration.js"
);
```

**问题**：
- 削弱类型检查能力（动态导入的返回类型需要类型断言）
- 降低模块依赖的可追溯性
- 使用 IDE 的"查找引用"功能时难以定位
- 增加打包工具优化的难度
- 运行时才能发现导入错误

### 3. 模块间的设计不一致

**Predefined 设计**：
```
predefined/registration.ts (协调)
  └─ tools/registration.ts (实现)
  └─ trigger/registration.ts (实现)
  └─ workflow/registration.ts (实现)
```

**Custom 设计**：
```
custom/registration.ts (直接实现全部逻辑)
```

**问题**：两种资源的注册结构完全不同，导致：
- 学习成本高
- 维护难度增加
- 新增应用层资源时不知道该遵循哪种模式

### 4. 关注点混淆

**当前**：资源模块（predefined/custom）同时承担：
- 资源定义/创建
- 资源注册

**应该是**：
- 资源模块只负责资源定义/创建
- 注册逻辑独立在 registration 模块

### 5. 内部模块管理复杂

在 `predefined/registration.ts` 中需要：
- 管理多个子模块的导入
- 协调注册顺序
- 聚合结果

这增加了中间层的复杂度。

## 推荐方案

### 方案 A：完全集中化（推荐）

将所有注册逻辑统一移至 `registration/` 目录。

**目标结构**：
```
registration/
├── orchestrator.ts              # 统一入口
├── predefined-registration.ts   # 预定义资源注册
├── custom-registration.ts       # 自定义资源注册
├── application-registration.ts  # 应用层资源（预留）
├── types.ts
└── index.ts
```

**优点**：
- ✅ 注册逻辑完全集中
- ✅ 清晰的分层：创建 vs 注册分离
- ✅ 易于新增资源类型
- ✅ 所有注册都遵循统一模式
- ✅ 文件结构简单，易于导航

**缺点**：
- ❌ registration 目录文件数增加
- ❌ 需要较大的重构工作

### 方案 B：分层集中化

在 registration 目录内按资源来源分层。

**目标结构**：
```
registration/
├── orchestrator.ts
├── predefined/
│   ├── index.ts
│   ├── tools-registration.ts
│   ├── triggers-registration.ts
│   └── workflows-registration.ts
├── custom/
│   ├── index.ts
│   ├── tools-registration.ts
│   ├── triggers-registration.ts
│   └── prompts-registration.ts
├── types.ts
└── index.ts
```

**优点**：
- ✅ 注册逻辑集中在 registration 目录
- ✅ 按资源来源分类，逻辑清晰
- ✅ 易于扩展
- ✅ 结构便于理解

**缺点**：
- ❌ 目录结构较深
- ❌ 文件数量较多

## 迁移路径建议

### 第一阶段：准备工作

1. **保持当前的公共 API**：
   - `predefined/index.ts` 继续导出 `registerPredefinedContent`
   - `custom/index.ts` 继续导出 `registerCustomResources`
   - `registration/index.ts` 继续导出 `registerAllResources`
   
   这样确保已有的代码（如 `sdk-instance.ts`）继续工作

2. **评估调用点**：
   - `sdk-instance.ts` - 直接导入 `registerPredefinedContent`
   - 需要检查是否有其他模块直接调用这些注册函数

### 第二阶段：实施迁移（选择方案 A）

1. **创建新的注册模块**：
   ```typescript
   // registration/predefined-registration.ts
   export function registerPredefinedContent(...) { ... }
   
   // registration/custom-registration.ts
   export function registerCustomResources(...) { ... }
   ```

2. **更新 orchestrator.ts**：
   ```typescript
   // 从动态导入改为静态导入
   import { registerPredefinedContent } from "./predefined-registration.js";
   import { registerCustomResources } from "./custom-registration.js";
   ```

3. **更新 predefined/registration.ts**（保持为兼容层）：
   ```typescript
   // 仅导出，不实现
   export { 
     registerPredefinedContent,
     unregisterPredefinedContent 
   } from "../../registration/predefined-registration.js";
   ```

4. **更新 custom/registration.ts**（保持为兼容层）：
   ```typescript
   // 仅导出，不实现
   export { 
     registerCustomResources 
   } from "../../registration/custom-registration.js";
   ```

### 第三阶段：清理和优化

1. **将 predefined/ 和 custom/ 中的注册逻辑删除**
2. **更新导入路径**（如果需要直接调用注册函数）
3. **更新文档和示例代码**
4. **运行完整测试**

## 代码示例对比

### 迁移前（现状）

```typescript
// sdk-instance.ts
import { registerPredefinedContent } from "../../../resources/predefined/registration.js";

// registration/orchestrator.ts
const { registerPredefinedContent } = await import(
  "../predefined/registration.js"
);
```

### 迁移后（方案 A）

```typescript
// sdk-instance.ts - 改进：路径更清晰
import { registerPredefinedContent } from "../../../resources/registration/predefined-registration.js";

// registration/orchestrator.ts - 改进：使用静态导入
import { registerPredefinedContent } from "./predefined-registration.js";
import { registerCustomResources } from "./custom-registration.js";
```

## 关键决策点

### 1. 是否保留兼容层？

**建议：保留**

在迁移期间保留兼容层（predefined/registration.ts、custom/registration.ts）作为重定向，这样：
- 已有代码继续工作
- 可以分阶段迁移调用点
- 最后统一删除兼容层

### 2. 何时移除动态导入？

**建议：在第二阶段立即移除**

- 改为静态导入提高代码质量
- 在 orchestrator.ts 中使用 try-catch 处理错误
- 提供清晰的错误信息

### 3. 内部模块注册的处理

**predefined 目录内的 tools/trigger/workflow 的注册**：

这些模块的 registration.ts 应该**保留**，因为它们是：
- 资源类型的具体实现细节
- 内部使用（被 predefined-registration.ts 调用）
- 职责明确（负责该类型的注册逻辑）

这与顶层的 predefined/registration.ts（协调器）不同。

## 总体优势

| 方面 | 现状 | 迁移后 |
|------|------|--------|
| 代码定位 | 分散 | 集中在 registration/ |
| 导入方式 | 动态导入 | 静态导入 |
| 类型安全 | 低 | 高 |
| 易于维护 | 困难 | 容易 |
| 新增资源 | 需修改多处 | 仅需修改 registration/ |
| IDE 支持 | 受限 | 完全支持 |
| 打包优化 | 困难 | 容易 |

## 实施风险与缓解

| 风险 | 影响 | 缓解方案 |
|------|------|--------|
| 破坏现有调用 | 编译错误 | 保留兼容层，分阶段迁移 |
| 循环依赖 | 构建失败 | 充分测试，检查导入顺序 |
| 遗漏的调用点 | 运行时错误 | 完整搜索和单元测试 |
| 功能回归 | 运行时异常 | 集成测试和端到端测试 |

## 结论

**建议实施方案 A（完全集中化）**，理由：

1. 解决所有问题：注册逻辑分散、动态导入、设计不一致
2. 工作量可控：通过兼容层逐步迁移
3. 长期收益大：代码质量、可维护性、开发体验显著提升
4. 风险可控：完整的兼容层保证安全过渡
