# 日志系统迁移指南

## 背景

本文档指导开发者如何将代码从直接使用 `console.*` 迁移到使用统一的日志系统。

## 为什么要迁移？

1. **输出污染**：直接使用 `console.log/error/warn` 会绕过日志系统，直接输出到终端
2. **无法重定向**：无法将日志统一输出到文件
3. **缺乏上下文**：无法附加结构化上下文信息
4. **级别控制**：无法通过配置控制日志级别

## 迁移步骤

### 1. 替换 console.log/info

**迁移前：**
```typescript
console.log("Processing started");
console.log(`User ${userId} logged in`);
```

**迁移后：**
```typescript
import { logger } from "../../utils/logger.js";

logger.info("Processing started");
logger.info(`User ${userId} logged in`, { userId });
```

### 2. 替换 console.warn

**迁移前：**
```typescript
console.warn(`Template '${templateId}' not found`);
```

**迁移后：**
```typescript
import { logger } from "../../utils/logger.js";

logger.warn(`Template '${templateId}' not found`, { templateId });
```

### 3. 替换 console.error

**迁移前：**
```typescript
console.error("Operation failed:", error);
console.error(`Error processing thread ${threadId}:`, err);
```

**迁移后：**
```typescript
import { logger } from "../../utils/logger.js";

logger.error("Operation failed", { error: error.message });
logger.error(`Error processing thread ${threadId}`, { 
  threadId, 
  error: err.message 
});
```

### 4. 替换 console.debug

**迁移前：**
```typescript
console.debug("Debug info:", data);
```

**迁移后：**
```typescript
import { logger } from "../../utils/logger.js";

logger.debug("Debug info", { data });
```

## 路径参考

根据文件位置，选择正确的导入路径：

| 文件位置 | 导入路径 |
|---------|---------|
| `sdk/core/**/*.ts` | `../../utils/logger.js` |
| `sdk/core/prompt/*.ts` | `../../utils/logger.js` |
| `sdk/core/services/*.ts` | `../../utils/logger.js` |
| `sdk/core/llm/formatters/*.ts` | `../../../utils/index.js` |
| `sdk/graph/execution/managers/*.ts` | `../../../utils/logger.js` |
| `sdk/graph/execution/handlers/**/*.ts` | `../../../../utils/logger.js` |
| `sdk/api/**/*.ts` | `../../../utils/logger.js` |

## 最佳实践

### 1. 始终添加上下文

```typescript
// 好的做法
logger.warn("Template not found", { 
  templateId, 
  availableTemplates: templateList 
});

// 避免这样做
logger.warn(`Template ${templateId} not found`);
```

### 2. 使用模块级Logger

对于大型模块，创建子logger：

```typescript
import { createModuleLogger } from "@wf-agent/sdk";

const logger = createModuleLogger("execution-pool");

logger.info("Pool initialized");
```

### 3. 错误日志包含堆栈

```typescript
try {
  await riskyOperation();
} catch (error) {
  logger.error("Operation failed", {
    error: error instanceof Error ? error.message : String(error),
    stack: error instanceof Error ? error.stack : undefined,
  });
}
```

### 4. 避免敏感信息

```typescript
// 不要记录敏感信息
logger.info("User login", { 
  userId, 
  // password: user.password  // 不要这样做！
});
```

## 检查清单

迁移完成后，检查：

- [ ] 文件中没有 `console.log`
- [ ] 文件中没有 `console.warn`
- [ ] 文件中没有 `console.error`
- [ ] 文件中没有 `console.debug`
- [ ] 文件中没有 `console.info`
- [ ] 所有日志调用都包含适当的上下文

## 验证

运行以下命令检查是否还有遗漏的 `console.*` 调用：

```bash
# 在SDK目录中搜索
grep -r "console\." sdk/ --include="*.ts" | grep -v "node_modules" | grep -v ".test.ts"
```

## 常见问题

### Q: 紧急调试时可以用console吗？

A: 不建议。如果必须临时使用，请在提交前移除。

### Q: 第三方库的console输出怎么办？

A: 这些输出通常无法避免，但可以通过CLI-APP的重定向机制将其也输出到日志文件。

### Q: 测试代码中可以用console吗？

A: 测试代码中可以使用，但建议使用专门的测试日志工具。
