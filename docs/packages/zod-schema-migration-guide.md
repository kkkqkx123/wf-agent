# Zod Schema 迁移指南

## 概述

本指南帮助开发者将现有的验证器迁移到使用类型包中的 Zod Schema，实现强类型保证。

## 迁移步骤

### 步骤 1: 检查当前验证器

找出需要迁移的验证器文件：

```bash
# 查找所有验证器文件
find sdk/graph/validation/node-validation -name "*-validator.ts"
```

### 步骤 2: 创建对应的 Zod Schema

在 `packages/types/src/node/configs/` 中创建 Schema 文件：

```typescript
// packages/types/src/node/configs/your-node-configs-schema.ts
import { z } from "zod";

export const YourNodeConfigSchema = z.object({
  // 定义你的 Schema
  field1: z.string(),
  field2: z.number().optional(),
});

export const isYourNodeConfig = (config: unknown): config is YourNodeConfig => {
  return YourNodeConfigSchema.safeParse(config).success;
};
```

### 步骤 3: 更新类型导出

在 `packages/types/src/node/configs/index.ts` 中导出新 Schema：

```typescript
export {
  YourNodeConfigSchema,
  isYourNodeConfig,
} from './your-node-configs-schema.js';
```

### 步骤 4: 更新验证器

修改验证器文件以使用类型包中的 Schema：

```typescript
// 修改前
import { z } from "zod";
import type { Node } from "@modular-agent/types";

const yourNodeConfigSchema = z.object({
  field1: z.string(),
  field2: z.number().optional(),
});

export function validateYourNode(node: Node) {
  // 使用本地定义的 Schema
}

// 修改后
import type { Node } from "@modular-agent/types";
import { YourNodeConfigSchema } from "@modular-agent/types";

export function validateYourNode(node: Node) {
  // 使用类型包中的 Schema
  const configResult = validateNodeConfig(
    node.config,
    YourNodeConfigSchema,
    node.id,
    "YOUR_NODE"
  );
  return configResult.isErr() ? configResult : ok(node);
}
```

### 步骤 5: 运行测试

确保迁移后所有测试通过：

```bash
# 运行类型检查
pnpm typecheck

# 运行单元测试
cd sdk
pnpm test graph/validation/node-validation
```

## 示例：迁移 LLM Node 验证器

### 修改前

```typescript
// sdk/graph/validation/node-validation/llm-validator.ts
import { z } from "zod";
import type { Node } from "@modular-agent/types";

const llmNodeConfigSchema = z.object({
  profileId: z.string().min(1),
  prompt: z.string().optional(),
  parameters: z.record(z.string(), z.any()).optional(),
  maxToolCallsPerRequest: z.number().min(1).optional(),
});

export function validateLLMNode(node: Node) {
  const typeResult = validateNodeType(node, "LLM");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(
    node.config,
    llmNodeConfigSchema,
    node.id,
    "LLM"
  );

  return configResult.isErr() ? configResult : ok(node);
}
```

### 修改后

#### 1. 创建 Schema

```typescript
// packages/types/src/node/configs/execution-configs-schema.ts
import { z } from "zod";

export const LLMNodeConfigSchema = z.object({
  profileId: z.string().min(1, "Profile ID is required"),
  prompt: z.string().optional(),
  parameters: z.record(z.string(), z.any()).optional(),
  maxToolCallsPerRequest: z.number().min(1, "Max tool calls per request must be at least 1").optional(),
});

export const isLLMNodeConfig = (config: unknown): config is LLMNodeConfig => {
  return LLMNodeConfigSchema.safeParse(config).success;
};
```

#### 2. 更新导出

```typescript
// packages/types/src/node/configs/index.ts
export {
  LLMNodeConfigSchema,
  isLLMNodeConfig,
} from './execution-configs-schema.js';
```

#### 3. 更新验证器

```typescript
// sdk/graph/validation/node-validation/llm-validator.ts
import type { Node } from "@modular-agent/types";
import { LLMNodeConfigSchema } from "@modular-agent/types";

export function validateLLMNode(node: Node) {
  const typeResult = validateNodeType(node, "LLM");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(
    node.config,
    LLMNodeConfigSchema,
    node.id,
    "LLM"
  );

  return configResult.isErr() ? configResult : ok(node);
}
```

## 常见问题

### Q1: 如何处理复杂的验证逻辑？

**A**: 对于复杂的验证逻辑，可以在 Schema 中使用 `.refine()` 方法：

```typescript
export const YourNodeConfigSchema = z.object({
  field1: z.string(),
  field2: z.number(),
}).refine(
  (data) => data.field2 > 0,
  {
    message: "field2 must be greater than 0",
    path: ["field2"],
  }
);
```

### Q2: 如何处理条件性字段？

**A**: 使用 `.and()` 或 `.or()` 来处理条件性字段：

```typescript
export const YourNodeConfigSchema = z.object({
  type: z.enum(["type1", "type2"]),
  type1Field: z.string().optional(),
  type2Field: z.number().optional(),
}).refine(
  (data) => {
    if (data.type === "type1") return data.type1Field !== undefined;
    return data.type2Field !== undefined;
  },
  {
    message: "Required field for the selected type is missing",
  }
);
```

### Q3: 如何处理嵌套对象？

**A**: 为嵌套对象创建单独的 Schema：

```typescript
const nestedSchema = z.object({
  nestedField1: z.string(),
  nestedField2: z.number(),
});

export const YourNodeConfigSchema = z.object({
  nested: nestedSchema,
});
```

### Q4: 如何处理数组验证？

**A**: 使用 `.array()` 和验证方法：

```typescript
export const YourNodeConfigSchema = z.object({
  items: z.array(z.object({
    id: z.string(),
    value: z.number(),
  })).min(1, "At least one item is required"),
});
```

### Q5: 如何处理可选字段？

**A**: 使用 `.optional()` 或 `.nullable()`：

```typescript
export const YourNodeConfigSchema = z.object({
  optionalField: z.string().optional(),
  nullableField: z.string().nullable(),
  optionalOrNull: z.string().optional().nullable(),
});
```

## 最佳实践

### 1. Schema 命名

- Schema 文件名使用 `-schema.ts` 后缀
- Schema 名称使用 `*Schema` 后缀
- 类型守卫函数使用 `is*` 前缀

### 2. 错误消息

- 提供清晰、具体的错误消息
- 在 `.refine()` 中指定 `path` 以便错误定位
- 使用国际化友好的消息格式

### 3. 性能优化

- 避免在 Schema 中使用复杂的计算
- 对于大型 Schema，考虑分块定义
- 使用 `.passthrough()` 允许额外字段（如果需要）

### 4. 测试

- 为每个 Schema 编写单元测试
- 测试有效和无效的输入
- 测试边界条件

## 迁移检查清单

- [ ] 创建对应的 Zod Schema 文件
- [ ] 实现类型守卫函数
- [ ] 更新类型包导出
- [ ] 修改验证器以使用类型包中的 Schema
- [ ] 移除验证器中的本地 Schema 定义
- [ ] 运行类型检查
- [ ] 运行单元测试
- [ ] 更新相关文档

## 相关资源

- [Zod Schema 集成方案](./zod-schema-integration.md)
- [Zod 官方文档](https://zod.dev/)
- [TypeScript 类型定义](../../packages/types/README.md)
- [验证器架构](../../sdk/graph/validation/README.md)
