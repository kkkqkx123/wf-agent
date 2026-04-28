# 架构更新说明

## 📋 更新概述

根据实际迁移情况，对原始架构设计进行了以下更新：

## 🔄 主要变更

### 1. 类型独立化

**已完成**：
- ✅ 创建 `packages/types` 包
- ✅ 迁移所有类型定义从 `sdk/types` 到 `packages/types/src`
- ✅ 更新 `sdk/types/index.ts` 重新导出 `@modular-agent/types`
- ✅ 配置 zod 版本为 4.3.6

**依赖关系**：
```
packages/types (无依赖)
  ↓
packages/common-utils (依赖 types)
  ↓
packages/tool-executors (依赖 types, common-utils)
  ↓
sdk (依赖 types, common-utils, tool-executors)
```

### 2. 通用工具包扩展

**已完成**：
- ✅ 创建 `packages/common-utils` 包
- ✅ 迁移 HTTP 传输层从 `sdk/core/http` 到 `packages/common-utils/src/http`
- ✅ 迁移工具函数从 `sdk/utils` 到 `packages/common-utils/src/utils`
- ✅ 迁移表达式求值器到 `packages/common-utils/src/evalutor`
- ✅ 迁移 LLM 工具到 `packages/common-utils/src/llm`
- ✅ **新增**：迁移 LLM 客户端基础设施到 `packages/common-utils/src/llm-clients`

**LLM 客户端迁移详情**：

保留在 SDK（SDK级别）：
- `sdk/core/llm/profile-manager.ts` - 配置文件管理器
- `sdk/core/llm/wrapper.ts` - LLM 包装器

迁移到 common-utils（基础设施）：
- `packages/common-utils/src/llm-clients/base-client.ts` - 基础客户端
- `packages/common-utils/src/llm-clients/client-factory.ts` - 客户端工厂
- `packages/common-utils/src/llm-clients/message-stream.ts` - 消息流
- `packages/common-utils/src/llm-clients/message-stream-events.ts` - 消息流事件
- `packages/common-utils/src/llm-clients/clients/` - 各种 LLM 客户端实现

### 3. 工具执行器包

**已完成**：
- ✅ 创建 `packages/tool-executors` 包
- ✅ 创建目录结构（mcp, rest, stateful, stateless 及其 impl 子目录）
- ✅ 配置依赖关系

**待实现**：
- ⏳ 实现各执行器的具体逻辑
- ⏳ 编写测试用例

### 4. SDK 层改造

**已完成**：
- ✅ 更新 `sdk/package.json` 添加依赖
- ✅ 创建 `sdk/core/tools/interfaces/tool-executor.ts` - 工具执行器接口
- ✅ 创建 `sdk/core/tools/utils/tool-executor-helper.ts` - 工具执行器辅助类
- ✅ 更新 `sdk/core/services/tool-service.ts` 使用 packages 中的实现
- ✅ 更新 `sdk/core/llm/index.ts` 从 common-utils 导入基础设施

**待删除**：
- ⏳ `sdk/core/tools/base-tool-executor.ts` - 已被接口和辅助类替代
- ⏳ `sdk/core/tools/executors/` 目录 - 已迁移到 packages/tool-executors
- ⏳ `sdk/core/http/` 目录 - 已迁移到 packages/common-utils
- ⏳ `sdk/utils/` 目录 - 已迁移到 packages/common-utils

## 📁 最终目录结构

```
packages/
├── types/                          # 类型定义（基础层）
│   ├── src/
│   │   ├── tool.ts
│   │   ├── errors.ts
│   │   ├── events.ts
│   │   ├── common.ts
│   │   └── ... (其他类型文件)
│   ├── package.json
│   └── tsconfig.json
│
├── common-utils/                   # 通用工具（依赖types）
│   ├── src/
│   │   ├── http/                   # HTTP传输
│   │   │   ├── transport.ts
│   │   │   ├── http-transport.ts
│   │   │   ├── sse-transport.ts
│   │   │   ├── errors.ts
│   │   │   └── __tests__/
│   │   ├── utils/                  # 工具函数
│   │   │   ├── id-utils.ts
│   │   │   ├── timestamp-utils.ts
│   │   │   └── ...
│   │   ├── evalutor/               # 表达式求值器
│   │   │   ├── condition-evaluator.ts
│   │   │   ├── expression-parser.ts
│   │   │   └── ...
│   │   ├── llm/                    # LLM工具
│   │   │   ├── message-helper.ts
│   │   │   └── tool-converter.ts
│   │   └── llm-clients/            # LLM客户端基础设施
│   │       ├── base-client.ts
│   │       ├── client-factory.ts
│   │       ├── message-stream.ts
│   │       ├── message-stream-events.ts
│   │       ├── clients/
│   │       │   ├── anthropic.ts
│   │       │   ├── gemini-native.ts
│   │       │   ├── gemini-openai.ts
│   │       │   ├── openai-chat.ts
│   │       │   └── openai-response.ts
│   │       └── index.ts
│   ├── package.json
│   └── tsconfig.json
│
└── tool-executors/                 # 工具执行器（依赖types, common-utils）
    ├── src/
    │   ├── mcp/
    │   │   ├── mcp-executor.ts
    │   │   └── impl/
    │   │       ├── stdio-transport.ts
    │   │       ├── sse-transport.ts
    │   │       └── mcp-session.ts
    │   ├── rest/
    │   │   ├── rest-executor.ts
    │   │   └── impl/
    │   │       └── http-client.ts
    │   ├── stateful/
    │   │   ├── stateful-executor.ts
    │   │   └── impl/
    │   │       └── instance-manager.ts
    │   └── stateless/
    │       ├── stateless-executor.ts
    │       └── impl/
    │           └── function-wrapper.ts
    ├── package.json
    └── tsconfig.json

sdk/                                 # 核心SDK（依赖packages）
├── src/
│   ├── types/                       # 重新导出packages/types
│   │   └── index.ts
│   ├── core/
│   │   ├── tools/
│   │   │   ├── interfaces/
│   │   │   │   └── tool-executor.ts  # IToolExecutor接口
│   │   │   ├── utils/
│   │   │   │   └── tool-executor-helper.ts  # ToolExecutorHelper
│   │   │   ├── tool-registry.ts
│   │   │   └── index.ts
│   │   ├── services/
│   │   │   └── tool-service.ts      # 使用packages/tool-executors
│   │   ├── llm/
│   │   │   ├── profile-manager.ts   # SDK级别
│   │   │   ├── wrapper.ts           # SDK级别
│   │   │   └── index.ts             # 从common-utils导入基础设施
│   │   └── execution/
│   │       └── executors/
│   │           └── tool-call-executor.ts
│   └── api/
│       └── index.ts
├── package.json
└── tsconfig.json
```

## 🔑 关键设计决策

### 1. 纯接口方案

采用纯接口方案，而非抽象基类：

**SDK层**：
- `IToolExecutor` 接口 - 定义执行器标准
- `ToolExecutorHelper` 工具类 - 提供通用逻辑（验证、重试、超时）

**Packages层**：
- 各执行器实现 `IToolExecutor` 接口
- 不继承任何基类，保持灵活性

### 2. LLM 分层设计

将 LLM 功能分为两层：

**SDK层**（业务逻辑）：
- `ProfileManager` - 配置管理
- `LLMWrapper` - 统一接口

**Common-utils层**（基础设施）：
- `BaseLLMClient` - 基础客户端
- `ClientFactory` - 客户端工厂
- `MessageStream` - 消息流
- 各提供商客户端实现

### 3. 向后兼容性

通过重新导出保持向后兼容：

```typescript
// sdk/types/index.ts
export * from '@modular-agent/types';

// sdk/core/llm/index.ts
export { LLMWrapper } from './wrapper';
export { BaseLLMClient } from '@modular-agent/common-utils';
```

## ✅ 完成状态

| 任务 | 状态 |
|------|------|
| 创建 packages/types | ✅ 完成 |
| 创建 packages/common-utils | ✅ 完成 |
| 迁移 HTTP 传输 | ✅ 完成 |
| 迁移工具函数 | ✅ 完成 |
| 迁移 LLM 客户端基础设施 | ✅ 完成 |
| 创建 packages/tool-executors 框架 | ✅ 完成 |
| 更新 SDK 设计 | ✅ 完成 |
| 实现工具执行器 | ⏳ 待完成 |
| 编写测试用例 | ⏳ 待完成 |
| 更新文档 | ✅ 完成 |

## 🚀 下一步

1. **实现工具执行器**
   - 实现 McpExecutor
   - 实现 RestExecutor
   - 实现 StatefulExecutor
   - 实现 StatelessExecutor

2. **编写测试**
   - 单元测试
   - 集成测试

3. **清理旧代码**
   - 删除 `sdk/core/tools/base-tool-executor.ts`
   - 删除 `sdk/core/tools/executors/` 目录
   - 删除 `sdk/core/http/` 目录
   - 删除 `sdk/utils/` 目录

4. **验证和测试**
   - 运行所有测试
   - 验证依赖关系
   - 性能测试

## 📝 注意事项

1. **TypeScript 错误**：当前存在一些 TypeScript 错误，这是因为包还未构建。需要先构建 packages，然后这些错误会消失。

2. **导入路径**：所有从 SDK 迁移到 packages 的代码，其导入路径都需要更新。

3. **测试迁移**：测试文件也需要相应迁移或更新导入路径。

4. **文档更新**：需要更新所有相关文档以反映新的架构。

---

**文档版本**：v2.0  
**最后更新**：2024  
**维护者**：Modular Agent Framework Team