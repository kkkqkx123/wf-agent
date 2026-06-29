# 导出架构改进方案

## 问题诊断

### 当前状态的危害

1. **隐藏的导入方式**
   - 同一个类可以用5种不同的方式导入
   - 维护者不知道哪种是"正确的"
   - 导致代码库中导入方式不统一

2. **循环引用风险**
   - 多层重导出增加了循环依赖的可能性
   - 难以跟踪真实的依赖关系

3. **维护困难**
   - 修改一个导出需要更新多个index.ts
   - 删除一个导出时不知道有多少地方依赖它

4. **打包和tree-shaking问题**
   - 多层重导出导致打包工具难以优化
   - 可能导致更大的bundle体积

## 改进原则

### 原则1：明确的模块结构
```
源文件 (具体实现)
  ↑
  ├─ 同级别的index.ts (仅便利导出，非常有限)
  │
  └─ 消费者直接导入
```

不应该有：`shared/index.ts → persistence/index.ts → core/index.ts → 实现`

### 原则2：导出分类

**应该导出：**
- 稳定的public API
- 供其他模块使用的核心类/接口
- 文档化的、有明确用途的导出

**不应该导出：**
- 内部工具函数
- 实现细节
- 为了"统一管理"的无意义重导出

### 原则3：单一导入路径
```
// ✅ 好
import { BasePersistentRegistry } from "@wf-agent/sdk/shared/persistence/core"
import type { IdExtractor } from "@wf-agent/sdk/shared/persistence/core"

// ❌ 差 (通过多层重导出)
import { BasePersistentRegistry } from "@wf-agent/sdk"
import { BasePersistentRegistry } from "@wf-agent/sdk/shared"
import { BasePersistentRegistry } from "@wf-agent/sdk/shared/persistence"
```

## 具体改进步骤

### 步骤1：清理persistence模块
- ✅ 移除重复的PersistenceStrategy导出
- ✅ 保留core/types.js中的PersistenceStrategy为主导出
- ✅ 持久化index.ts仅作为模块内的便利导出

### 步骤2：明确shared/index.ts的职责
**选项A：完全移除 (推荐)**
```typescript
// 删除 export * from "./persistence/index.js" 等
// 让使用者直接导入需要的模块
```

**选项B：仅导出顶层定义**
```typescript
// 仅导出shared直接定义的东西
export * from "./global-context.js"
// 不导出子模块的聚合
```

### 步骤3：建立导入约定
```typescript
// ✅ 推荐的导入方式

// 从具体的模块导入
import { BasePersistentRegistry, type IdExtractor } 
  from "@wf-agent/sdk/shared/persistence/core"

import { PersistenceEventEmitter } 
  from "@wf-agent/sdk/shared/persistence/events"

import { DataConsistencyValidator } 
  from "@wf-agent/sdk/shared/persistence/validation"

// 不要
import { BasePersistentRegistry } from "@wf-agent/sdk"
import { BasePersistentRegistry } from "@wf-agent/sdk/shared"
```

## 实施计划

### 高优先级 (需要立即修复)
1. ✅ 移除persistence/index.ts的重复PersistenceStrategy导出
2. ⚠️ 清理shared/index.ts的`export *`

### 中优先级 (代码迁移)
3. 更新所有导入语句以使用直接路径
4. 建立导入规范文档

### 低优先级 (代码质量)
5. 考虑是否每个子模块都需要index.ts
6. 评估是否有合理的"公共API"聚合点

## 对现有代码的影响

### 需要更新的导入

```typescript
// 当前可能的导入方式
import { BasePersistentRegistry } from "@wf-agent/sdk"
import { BasePersistentRegistry } from "@wf-agent/sdk/shared"

// 改为明确的直接导入
import { BasePersistentRegistry } from "@wf-agent/sdk/shared/persistence/core"
```

### 迁移路径
1. 先清理persistence/index.ts
2. 更新persistence模块内的导入
3. 然后处理shared/index.ts
4. 最后更新所有消费者

## 不做什么

❌ **不要创建额外的中间层** - 如`facade.ts`或`public-api.ts`

❌ **不要为了"对称性"而保留unused的index.ts** - 如果没有重导出需求，删除它

❌ **不要混合默认导出和命名导出** - 保持一致性

## 参考：正确的导出模式

**persistence/core/index.ts** (可选，内部便利)
```typescript
export { BasePersistentRegistry } from "./base-persistent-registry.js"
export type { IdExtractor, ... } from "./types.js"
```

**persistence/index.ts** (应该删除或仅保留边界定义)
```typescript
// 仅导出persistence作为一个整体的必要东西
export type { RegistryPersistenceConfig } from "./core/types.js"

// 或者完全删除，让使用者导入具体的子模块
```

**shared/index.ts** (应该删除或最小化)
```typescript
// 仅导出shared模块自己定义的
export * from "./global-context.js"

// 不要导出子模块，让他们各自为政
```
