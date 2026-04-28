# Zod Schema 快速参考

## 已实现的 Schema

### Message Operations

| Schema | 文件位置 | 用途 |
|--------|---------|------|
| `MessageOperationConfigSchema` | `message/message-operations-schema.ts` | 消息操作配置统一 Schema |
| `AppendMessageOperationSchema` | `message/message-operations-schema.ts` | APPEND 操作 |
| `InsertMessageOperationSchema` | `message/message-operations-schema.ts` | INSERT 操作 |
| `ReplaceMessageOperationSchema` | `message/message-operations-schema.ts` | REPLACE 操作 |
| `TruncateMessageOperationSchema` | `message/message-operations-schema.ts` | TRUNCATE 操作 |
| `ClearMessageOperationSchema` | `message/message-operations-schema.ts` | CLEAR 操作 |
| `FilterMessageOperationSchema` | `message/message-operations-schema.ts` | FILTER 操作 |
| `RollbackMessageOperationSchema` | `message/message-operations-schema.ts` | ROLLBACK 操作 |

### Node Configs

| Schema | 文件位置 | 用途 |
|--------|---------|------|
| `ContextProcessorNodeConfigSchema` | `node/configs/context-configs-schema.ts` | CONTEXT_PROCESSOR 节点 |
| `SubgraphNodeConfigSchema` | `node/configs/subgraph-configs-schema.ts` | SUBGRAPH 节点 |
| `StartFromTriggerNodeConfigSchema` | `node/configs/subgraph-configs-schema.ts` | START_FROM_TRIGGER 节点 |
| `ContinueFromTriggerNodeConfigSchema` | `node/configs/subgraph-configs-schema.ts` | CONTINUE_FROM_TRIGGER 节点 |

### 待实现的 Schema

- [ ] `LLMNodeConfigSchema` - LLM 节点
- [ ] `ScriptNodeConfigSchema` - Script 节点
- [ ] `VariableNodeConfigSchema` - Variable 节点
- [ ] `RouteNodeConfigSchema` - Route 节点
- [ ] `ForkNodeConfigSchema` - Fork 节点
- [ ] `JoinNodeConfigSchema` - Join 节点
- [ ] `LoopStartNodeConfigSchema` - Loop Start 节点
- [ ] `LoopEndNodeConfigSchema` - Loop End 节点
- [ ] `AgentLoopNodeConfigSchema` - Agent Loop 节点
- [ ] `UserInteractionNodeConfigSchema` - User Interaction 节点
- [ ] `AddToolNodeConfigSchema` - Add Tool 节点

## 使用示例

### 基本使用

```typescript
import { ContextProcessorNodeConfigSchema } from "@modular-agent/types";

// 验证配置
const result = ContextProcessorNodeConfigSchema.parse(config);

// 安全解析（不抛出异常）
const safeResult = ContextProcessorNodeConfigSchema.safeParse(config);
if (safeResult.success) {
  console.log("Valid config:", safeResult.data);
} else {
  console.error("Validation errors:", safeResult.error);
}
```

### 类型守卫

```typescript
import { isContextProcessorNodeConfig } from "@modular-agent/types";

const config = { /* ... */ };

if (isContextProcessorNodeConfig(config)) {
  // TypeScript 自动收窄类型
  console.log(config.operationConfig);
}
```

### 在验证器中使用

```typescript
import type { Node } from "@modular-agent/types";
import { ContextProcessorNodeConfigSchema } from "@modular-agent/types";
import { validateNodeType, validateNodeConfig } from "../../../core/validation/utils.js";

export function validateContextProcessorNode(node: Node) {
  const typeResult = validateNodeType(node, "CONTEXT_PROCESSOR");
  if (typeResult.isErr()) {
    return typeResult;
  }

  const configResult = validateNodeConfig(
    node.config,
    ContextProcessorNodeConfigSchema,
    node.id,
    "CONTEXT_PROCESSOR"
  );

  return configResult.isErr() ? configResult : ok(node);
}
```

## 常用 Zod 方法

### 基本类型

```typescript
z.string()           // 字符串
z.number()           // 数字
z.boolean()          // 布尔值
z.array(z.string())  // 字符串数组
z.object({ ... })    // 对象
z.enum([...])        // 枚举
z.union([...])       // 联合类型
z.discriminatedUnion("field", [...])  // 判别联合类型
```

### 验证方法

```typescript
z.string().min(1)           // 最小长度
z.string().max(100)         // 最大长度
z.number().min(0)           // 最小值
z.number().max(100)         // 最大值
z.number().int()            // 整数
z.number().positive()       // 正数
z.string().email()          // 邮箱格式
z.string().url()            // URL 格式
z.string().regex(/pattern/) // 正则表达式
```

### 可选和可空

```typescript
z.string().optional()       // 可选
z.string().nullable()       // 可空
z.string().optional().nullable()  // 可选且可空
```

### 复杂验证

```typescript
z.object({ ... }).refine(
  (data) => data.field1 > 0,
  { message: "field1 must be greater than 0", path: ["field1"] }
)
```

### 类型转换

```typescript
z.string().transform((val) => val.toUpperCase())  // 转换为大写
z.string().pipe(z.coerce.number())  // 强制转换为数字
```

## 错误处理

### 获取错误详情

```typescript
const result = YourSchema.safeParse(config);
if (!result.success) {
  console.error(result.error.errors);
  // 输出格式:
  // [
  //   {
  //     code: "invalid_type",
  //     expected: "string",
  //     received: "number",
  //     path: ["field1"],
  //     message: "Expected string, received number"
  //   }
  // ]
}
```

### 自定义错误消息

```typescript
const schema = z.object({
  field1: z.string().min(1, "field1 is required"),
  field2: z.number().min(0, "field2 must be non-negative"),
});
```

## 性能优化

### 缓存 Schema

```typescript
// 避免重复创建 Schema
const cachedSchema = z.object({ /* ... */ });

// 而不是
function validate(data) {
  const schema = z.object({ /* ... */ });  // 每次都创建新 Schema
  return schema.parse(data);
}
```

### 使用 `.passthrough()` 允许额外字段

```typescript
const schema = z.object({
  requiredField: z.string(),
}).passthrough();  // 允许额外字段而不报错
```

### 使用 `.strict()` 严格模式

```typescript
const schema = z.object({
  requiredField: z.string(),
}).strict();  // 拒绝额外字段
```

## 调试技巧

### 打印 Schema 结构

```typescript
console.log(JSON.stringify(YourSchema.shape, null, 2));
```

### 测试验证结果

```typescript
const testData = { /* ... */ };
const result = YourSchema.safeParse(testData);

console.log("Success:", result.success);
if (!result.success) {
  console.log("Errors:", result.error.format());
}
```

## 相关链接

- [Zod 官方文档](https://zod.dev/)
- [Zod Schema 集成方案](./zod-schema-integration.md)
- [Zod Schema 迁移指南](./zod-schema-migration-guide.md)
- [TypeScript 类型定义](../../packages/types/README.md)
