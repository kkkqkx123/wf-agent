# ToolDeclarationFormatter 迁移说明

## 概述

`ToolDeclarationFormatter` 已从 `packages/prompt-templates` 迁移到 `sdk/core/utils/tools/`。

## 迁移原因

1. **职责分离**：`ToolDeclarationFormatter` 是一个运行时格式化工具，而非静态模板定义
2. **架构合理性**：作为 SDK 核心工具，应该位于 SDK 包中而非 prompt-templates 包
3. **依赖优化**：减少 prompt-templates 包的运行时依赖

## 变更内容

### 文件位置变更

**之前：**
```
packages/prompt-templates/src/formatters/tool-declaration-formatter.ts
packages/prompt-templates/__tests__/formatters/declaration-formatter.test.ts
```

**之后：**
```
sdk/core/utils/tools/tool-declaration-formatter.ts
sdk/core/utils/tools/__tests__/tool-declaration-formatter.test.ts
```

### 导入路径变更

**之前：**
```typescript
import { ToolDeclarationFormatter } from '@wf-agent/prompt-templates';
```

**之后：**
```typescript
import { ToolDeclarationFormatter } from '@wf-agent/sdk';
// 或者在 SDK 内部使用时
import { ToolDeclarationFormatter } from '../utils/tools/index.js';
```

### 已更新的文件

以下文件的导入语句已自动更新：

1. `sdk/core/llm/formatters/openai-chat.ts`
2. `sdk/core/llm/formatters/anthropic.ts`
3. `sdk/core/messaging/history-converter.ts`

## API 保持不变

所有公开 API 保持不变，无需修改调用代码（除了导入路径）：

- `ToolDeclarationFormatter.formatTools()`
- `ToolDeclarationFormatter.formatToolCalls()`
- `ToolDeclarationFormatter.formatToolResult()`
- `ToolDeclarationOptions` 类型

## 测试

所有测试已迁移并通过：

```bash
pnpm test core/utils/tools/__tests__/tool-declaration-formatter.test.ts
```

测试结果：✅ 15/15 测试通过

## 后续改进建议

基于之前的分析，建议在后续迭代中进行以下改进：

1. **添加错误处理**：为 `JSON.parse()` 添加 try-catch
2. **补充缺失 API**：添加 `formatToolCall()` 单数版本
3. **增强参数格式化**：支持嵌套对象、数组、enum 等更多 JSON Schema 特性
4. **完善测试覆盖**：添加边界情况和错误处理的测试

## 迁移日期

2026-05-13

## 相关文档

- [ToolDeclarationFormatter 设计分析报告](../../../../docs/sdk/tool/tool-declaration-formatter-analysis.md)
