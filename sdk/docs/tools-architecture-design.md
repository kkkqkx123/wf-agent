# SDK 预定义工具架构设计

## 1. 概述

本文档描述 SDK 内置工具的架构设计。工具作为预定义资源，与预定义工作流、触发器位于同一层级，提供给应用层开箱即用的功能。

## 2. 架构定位

### 2.1 分层职责

| 层级 | 路径 | 职责 |
|------|------|------|
| **执行框架** | `packages/tool-executors` | 工具执行基础设施（StatelessExecutor、StatefulExecutor、RestExecutor、McpExecutor） |
| **提示词模板** | `packages/prompt-templates` | 工具描述模板、参数 schema 模板 |
| **预定义资源** | `sdk/resources/predefined` | 开箱即用的工作流、触发器、工具定义 |
| **核心服务** | `sdk/core` | ToolService、验证器、资源管理（不含具体工具实现） |
| **应用层** | `apps/*` | 界面、配置、应用特有工具覆盖 |

### 2.2 工具作为资源的理由

1. **可选择性**：工具是可选的预定义功能，应用可以选择使用或不使用
2. **与 Workflow/Trigger 一致**：三者都是预定义资源，组织方式保持一致
3. **核心层保持最小化**：`sdk/core` 只保留基础设施，不包含业务工具实现

## 3. 目录结构

```
sdk/resources/predefined/
├── index.ts                    # 统一导出所有预定义资源
├── workflows/                  # 预定义工作流（已有）
│   └── context-compression.ts
├── triggers/                   # 预定义触发器（已有）
│   └── context-compression.ts
└── tools/                      # [新增] 预定义工具
    ├── index.ts                # 工具注册函数
    ├── types.ts                # 工具定义相关类型扩展
    │
    ├── stateless/              # 无状态工具
    │   ├── filesystem/         # 文件操作类
    │   │   ├── read-file/
    │   │   │   ├── schema.ts   # 参数 schema 定义
    │   │   │   ├── handler.ts  # 执行逻辑
    │   │   │   └── index.ts    # 统一导出
    │   │   ├── write-file/
    │   │   │   ├── schema.ts
    │   │   │   └── handler.ts
    │   │   └── edit-file/
    │   │       ├── schema.ts
    │   │       └── handler.ts
    │   │
    │   ├── shell/              # Shell 执行类
    │   │   └── bash/
    │   │       ├── schema.ts
    │   │       └── handler.ts
    │   │
    │   └── code/               # 代码操作类
    │       └── search/
    │           ├── schema.ts
    │           └── handler.ts
    │
    ├── stateful/               # 有状态工具
    │   └── memory/             # 内存/会话类
    │       └── session-note/
    │           ├── schema.ts
    │           ├── handler.ts  # 含工厂函数
    │           └── types.ts    # 状态类型定义
    │
    ├── rest/                   # REST 工具
    │   └── http-request/
    │       ├── schema.ts
    │       └── config.ts       # REST 特有配置
    │
    └── mcp/                    # MCP 工具
        └── mcp-invoke/
            ├── schema.ts
            └── config.ts
```

## 4. 两层分类体系

### 4.1 第一层：按执行类型划分

对应 `packages/tool-executors` 中的四种执行器：

| 目录 | 执行器 | 特性 |
|------|--------|------|
| `stateless/` | `StatelessExecutor` | 纯函数，无状态，幂等 |
| `stateful/` | `StatefulExecutor` | 维护实例状态，生命周期管理 |
| `rest/` | `RestExecutor` | HTTP API 调用，配置驱动 |
| `mcp/` | `McpExecutor` | MCP 协议，动态发现 |

### 4.2 第二层：按功能分类

| 分类 | 用途 | 示例工具 |
|------|------|----------|
| `filesystem/` | 文件操作 | read_file, write_file, edit_file |
| `shell/` | 命令执行 | bash |
| `memory/` | 内存/会话管理 | session_note |
| `code/` | 代码相关 | search, analyze |
| `http/` | HTTP 请求 | http_request |

## 5. 工具定义结构

每个工具由三个文件组成：

### 5.1 schema.ts - 参数 Schema

```typescript
import type { ToolParameterSchema } from '@modular-agent/types';

export const readFileSchema: ToolParameterSchema = {
  type: 'object',
  properties: {
    path: {
      type: 'string',
      description: '文件的绝对路径或相对工作目录的路径'
    },
    offset: {
      type: 'integer',
      description: '起始行号（1-indexed，可选）',
      minimum: 1
    },
    limit: {
      type: 'integer',
      description: '读取的最大行数（可选）',
      minimum: 1
    }
  },
  required: ['path']
};
```

### 5.2 handler.ts - 执行逻辑

```typescript
import type { ToolOutput } from '@modular-agent/types';
import { readFile } from 'fs/promises';

export interface ReadFileConfig {
  workspaceDir: string;
  maxFileSize?: number;
}

export function createReadFileHandler(config: ReadFileConfig) {
  return async (params: {
    path: string;
    offset?: number;
    limit?: number;
  }): Promise<ToolOutput> => {
    try {
      // 实现逻辑
      const content = await readFile(fullPath, 'utf-8');
      // 处理 offset/limit
      // ...
      return {
        success: true,
        result: processedContent
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : '未知错误'
      };
    }
  };
}
```

### 5.3 index.ts - 统一导出

```typescript
export { readFileSchema } from './schema.js';
export { createReadFileHandler, type ReadFileConfig } from './handler.js';
```

## 6. 与现有架构的集成

### 6.1 使用 tool-executors 执行框架

```typescript
// sdk/resources/predefined/tools/stateless/filesystem/read-file/handler.ts
import { FunctionRegistry } from '@modular-agent/tool-executors';

// handler 被包装为 StatelessExecutor 可调用的形式
export function createReadFileHandler(config: ReadFileConfig) {
  const registry = new FunctionRegistry();
  // 注册执行逻辑
  return registry.wrap(async (params) => {
    // 实现
  });
}
```

### 6.2 使用 prompt-templates 描述模板

```typescript
// sdk/resources/predefined/tools/index.ts
import { TOOL_DESCRIPTION_TABLE_TEMPLATE } from '@modular-agent/prompt-templates';

function generateToolDescription(tool: PredefinedTool): string {
  // 使用 prompt-templates 渲染
  return renderTemplate(TOOL_DESCRIPTION_TABLE_TEMPLATE.content, {
    toolName: tool.name,
    parameters: JSON.stringify(tool.parameters, null, 2)
  });
}
```

### 6.3 验证器集成

```typescript
// 注册时验证（sdk/core/validation/tool-static-validator.ts）
import { validateToolStatic } from '@modular-agent/sdk/core/validation';

// 执行时验证（sdk/core/validation/tool-runtime-validator.ts）
import { validateToolRuntime } from '@modular-agent/sdk/core/validation';
```

## 7. 注册机制

### 7.1 预定义工具注册

```typescript
// sdk/resources/predefined/tools/index.ts
import { ToolService } from '@modular-agent/sdk/core/services';
import { readFileSchema, createReadFileHandler } from './stateless/filesystem/read-file/index.js';
// ... 其他工具

export interface PredefinedToolsOptions {
  enabled?: string[];      // 只启用指定工具（白名单）
  disabled?: string[];     // 禁用指定工具（黑名单）
  config?: {
    readFile?: { workspaceDir: string };
    sessionNote?: { memoryFile: string };
    // ... 其他工具配置
  };
}

export function registerPredefinedTools(
  toolService: ToolService,
  options?: PredefinedToolsOptions
): void {
  const tools = [
    {
      id: 'read_file',
      name: 'read_file',
      type: 'STATELESS' as const,
      category: 'filesystem',
      parameters: readFileSchema,
      createHandler: () => createReadFileHandler(options?.config?.readFile)
    },
    // ... 其他工具定义
  ];

  for (const tool of tools) {
    // 检查是否启用
    if (options?.disabled?.includes(tool.id)) continue;
    if (options?.enabled && !options.enabled.includes(tool.id)) continue;

    toolService.registerTool({
      id: tool.id,
      name: tool.name,
      type: tool.type,
      description: generateToolDescription(tool),
      parameters: tool.parameters,
      config: {
        execute: tool.createHandler()
      }
    });
  }
}
```

### 7.2 SDK 初始化

```typescript
// sdk/api/shared/core/sdk.ts
import { registerPredefinedTools } from '../../../resources/predefined/tools/index.js';
import { registerContextCompression } from '../../../resources/predefined/index.js';

class SDK {
  private bootstrap(options?: SDKOptions): void {
    // 注册预定义工具（默认可用）
    if (options?.predefinedTools?.enabled !== false) {
      registerPredefinedTools(
        this.dependencies.getToolService(),
        options?.predefinedTools
      );
    }

    // 注册预定义工作流和触发器
    if (options?.presets?.contextCompression?.enabled !== false) {
      registerContextCompression(...);
    }
  }
}
```

## 8. 迁移策略

### 8.1 创建目录结构

```bash
sdk/resources/predefined/tools/
├── index.ts
├── types.ts
├── stateless/
│   ├── filesystem/
│   │   ├── read-file/
│   │   ├── write-file/
│   │   └── edit-file/
│   ├── shell/
│   │   └── bash/
│   └── code/
│       └── search/
├── stateful/
│   └── memory/
│       └── session-note/
└── ...
```

### 8.2 迁移 CLI App 工具

将 `apps/cli-app/src/tools/` 中的工具实现迁移到 SDK：

| CLI App 原位置 | SDK 新位置 | 说明 |
|----------------|-----------|------|
| `tools/stateless/read-tool.ts` | `stateless/filesystem/read-file/` | 拆分 schema 和 handler |
| `tools/stateless/write-tool.ts` | `stateless/filesystem/write-file/` | 拆分 schema 和 handler |
| `tools/stateful/background-shell-tool.ts` | `stateful/memory/session-note/` | 使用 StatefulExecutor |

### 8.3 Apps 层简化

```typescript
// apps/cli-app/src/index.ts (迁移后)
import { getSDK } from '@modular-agent/sdk';

// SDK 初始化时已自动注册预定义工具
const sdk = getSDK({
  predefinedTools: {
    // 可选配置
    enabled: ['read_file', 'write_file', 'run_shell'],
    config: {
      readFile: { workspaceDir: process.cwd() }
    }
  }
});

// 如有 CLI 特有工具，可额外注册
// sdk.tools.register(cliSpecificTool);
```

## 9. 架构优势

1. **一致性**：与 workflows、triggers 同层，组织方式统一
2. **可选性**：应用可选择启用/禁用特定工具
3. **可扩展**：新增工具只需在对应目录添加文件
4. **复用性**：CLI 和 Web 后端共享相同工具实现
5. **清晰分层**：core 保持基础设施，resources 包含具体实现

## 10. 注意事项

1. **Tool vs Util**
   - Tool = LLM 可调用的业务工具（read_file 等）
   - Util = 编程工具函数（在 `packages/common-utils`）

2. **环境兼容性**
   - 当前设计假设所有后端为 Node.js 环境
   - 如未来有特殊环境，在 apps 层覆盖或使用条件加载

3. **Schema 复用**
   - 参数 schema 同时用于：
     - LLM API 调用（发给 LLM 供应商）
     - 运行时参数验证（sdk/core/validation）
     - 提示词模板变量生成
