# MCP 工具自动审批设计

## 1. 概述

本文档详细设计 MCP (Model Context Protocol) 工具的自动审批机制。当前项目已有 `use_mcp` 工具，需要为其设计细粒度的自动审批控制。

## 2. 当前 MCP 工具分析

### 2.1 use_mcp 工具定义

```typescript
// sdk/resources/predefined/tools/stateless/interaction/use-mcp/
{
  id: "use_mcp",
  name: "use_mcp",
  type: "STATELESS",
  category: "interaction",
  riskLevel: "NETWORK",  // MCP 调用属于网络操作
}
```

### 2.2 MCP 调用类型

MCP 协议支持多种操作类型：

| 操作类型 | 说明 | 风险级别 |
|----------|------|----------|
| `use_mcp` (tool call) | 调用 MCP 服务器提供的工具 | 需评估具体工具 |
| `read_resource` | 读取 MCP 服务器提供的资源 | READ_ONLY |
| `list_tools` | 列出 MCP 服务器的工具 | READ_ONLY |
| `list_resources` | 列出 MCP 服务器的资源 | READ_ONLY |

## 3. MCP 自动审批类型设计

### 3.1 MCP 工具配置类型

```typescript
// packages/types/src/tool/mcp-approval.ts

/**
 * MCP Tool Configuration
 */
export interface McpToolConfig {
  /** Tool name */
  name: string;
  /** Tool description */
  description?: string;
  /** Whether this tool is always allowed */
  alwaysAllow?: boolean;
  /** Risk level override */
  riskLevel?: ToolRiskLevel;
}

/**
 * MCP Server Configuration
 */
export interface McpServerConfig {
  /** Server name */
  name: string;
  /** Server description */
  description?: string;
  /** Tools provided by this server */
  tools?: McpToolConfig[];
  /** Resources provided by this server */
  resources?: {
    /** Resource URI pattern */
    uriPattern: string;
    /** Whether this resource is always allowed to read */
    alwaysAllow?: boolean;
  }[];
  /** Default behavior for tools not explicitly configured */
  defaultToolBehavior?: "always_approve" | "always_ask" | "always_deny";
  /** Default behavior for resources not explicitly configured */
  defaultResourceBehavior?: "always_approve" | "always_ask";
}

/**
 * MCP Approval Settings
 */
export interface McpApprovalSettings {
  /** Server configurations */
  servers: McpServerConfig[];
  /** Global default behavior for unknown servers */
  defaultServerBehavior?: "always_ask" | "always_deny";
}
```

### 3.2 MCP 调用请求类型

```typescript
// packages/types/src/tool/mcp-approval.ts

/**
 * MCP Tool Call Request
 */
export interface McpToolCallRequest {
  /** Request type */
  type: "use_mcp";
  /** Server name */
  serverName: string;
  /** Tool name */
  toolName: string;
  /** Tool arguments */
  arguments?: Record<string, unknown>;
}

/**
 * MCP Resource Read Request
 */
export interface McpResourceReadRequest {
  /** Request type */
  type: "read_resource";
  /** Server name */
  serverName: string;
  /** Resource URI */
  uri: string;
}

/**
 * MCP List Request
 */
export interface McpListRequest {
  /** Request type */
  type: "list_tools" | "list_resources";
  /** Server name */
  serverName: string;
}

/**
 * Unified MCP Request
 */
export type McpRequest =
  | McpToolCallRequest
  | McpResourceReadRequest
  | McpListRequest;
```

## 4. MCP 自动审批检查器

### 4.1 核心检查逻辑

```typescript
// sdk/core/services/auto-approval/mcp-approval-checker.ts

import type {
  McpApprovalSettings,
  McpRequest,
  McpServerConfig,
  McpToolConfig,
} from "@wf-agent/types";

export interface CheckMcpApprovalParams {
  /** MCP approval settings */
  settings: McpApprovalSettings;
  /** MCP request */
  request: McpRequest;
}

export type McpApprovalDecision =
  | { decision: "approve" }
  | { decision: "deny"; reason: string }
  | { decision: "ask" };

/**
 * Check if an MCP request should be auto-approved
 */
export function checkMcpApproval(params: CheckMcpApprovalParams): McpApprovalDecision {
  const { settings, request } = params;

  // 1. Find server configuration
  const serverConfig = settings.servers.find(s => s.name === request.serverName);

  if (!serverConfig) {
    // Unknown server
    switch (settings.defaultServerBehavior ?? "always_ask") {
      case "always_ask":
        return { decision: "ask" };
      case "always_deny":
        return { decision: "deny", reason: `Unknown MCP server: ${request.serverName}` };
    }
  }

  // 2. Handle by request type
  switch (request.type) {
    case "use_mcp":
      return checkMcpToolApproval(serverConfig, request);

    case "read_resource":
      return checkMcpResourceApproval(serverConfig, request);

    case "list_tools":
    case "list_resources":
      // List operations are generally safe
      return { decision: "approve" };
  }
}

/**
 * Check MCP tool call approval
 */
function checkMcpToolApproval(
  serverConfig: McpServerConfig,
  request: McpToolCallRequest
): McpApprovalDecision {
  // 1. Find tool configuration
  const toolConfig = serverConfig.tools?.find(t => t.name === request.toolName);

  if (toolConfig) {
    // Tool explicitly configured
    if (toolConfig.alwaysAllow) {
      return { decision: "approve" };
    }

    // Check risk level override
    if (toolConfig.riskLevel === "READ_ONLY") {
      return { decision: "approve" };
    }

    return { decision: "ask" };
  }

  // 2. Tool not explicitly configured - use default behavior
  switch (serverConfig.defaultToolBehavior ?? "always_ask") {
    case "always_approve":
      return { decision: "approve" };
    case "always_deny":
      return { decision: "deny", reason: `Tool ${request.toolName} not in allowlist` };
    default:
      return { decision: "ask" };
  }
}

/**
 * Check MCP resource read approval
 */
function checkMcpResourceApproval(
  serverConfig: McpServerConfig,
  request: McpResourceReadRequest
): McpApprovalDecision {
  // 1. Find matching resource configuration
  const resourceConfig = serverConfig.resources?.find(r =>
    matchUriPattern(request.uri, r.uriPattern)
  );

  if (resourceConfig) {
    if (resourceConfig.alwaysAllow) {
      return { decision: "approve" };
    }
    return { decision: "ask" };
  }

  // 2. Resource not explicitly configured
  switch (serverConfig.defaultResourceBehavior ?? "always_ask") {
    case "always_approve":
      return { decision: "approve" };
    default:
      return { decision: "ask" };
  }
}

/**
 * Match URI against pattern
 * Supports simple wildcard patterns like "file:///*"
 */
function matchUriPattern(uri: string, pattern: string): boolean {
  const regex = new RegExp(
    "^" + pattern.replace(/\*/g, ".*").replace(/\?/g, ".") + "$"
  );
  return regex.test(uri);
}
```

## 5. MCP 配置示例

### 5.1 数据库 MCP 服务器配置

```typescript
const databaseMcpConfig: McpServerConfig = {
  name: "database",
  description: "Database query MCP server",
  tools: [
    {
      name: "query",
      description: "Execute SQL query",
      alwaysAllow: false,  // Always require approval for queries
      riskLevel: "READ_ONLY",  // But mark as read-only for logging
    },
    {
      name: "execute",
      description: "Execute SQL statement",
      alwaysAllow: false,
      riskLevel: "WRITE",  // Write operations
    },
    {
      name: "list_tables",
      description: "List database tables",
      alwaysAllow: true,  // Safe to auto-approve
      riskLevel: "READ_ONLY",
    },
    {
      name: "describe_table",
      description: "Describe table schema",
      alwaysAllow: true,
      riskLevel: "READ_ONLY",
    },
  ],
  resources: [
    {
      uriPattern: "db://tables/*",
      alwaysAllow: true,  // Allow reading table schemas
    },
    {
      uriPattern: "db://data/*",
      alwaysAllow: false,  // Require approval for data access
    },
  ],
  defaultToolBehavior: "always_ask",
  defaultResourceBehavior: "always_ask",
};
```

### 5.2 文件系统 MCP 服务器配置

```typescript
const filesystemMcpConfig: McpServerConfig = {
  name: "filesystem",
  description: "File system MCP server",
  tools: [
    {
      name: "read_file",
      alwaysAllow: true,
      riskLevel: "READ_ONLY",
    },
    {
      name: "write_file",
      alwaysAllow: false,
      riskLevel: "WRITE",
    },
    {
      name: "list_directory",
      alwaysAllow: true,
      riskLevel: "READ_ONLY",
    },
  ],
  resources: [
    {
      uriPattern: "file:///*",
      alwaysAllow: true,  // Allow reading any file via resource API
    },
  ],
  defaultToolBehavior: "always_ask",
};
```

### 5.3 完整 MCP 审批配置

```typescript
const mcpApprovalSettings: McpApprovalSettings = {
  servers: [
    databaseMcpConfig,
    filesystemMcpConfig,
  ],
  defaultServerBehavior: "always_ask",  // Unknown servers require approval
};
```

## 6. 与 ToolApprovalOptions 集成

### 6.1 扩展 ToolApprovalOptions

```typescript
// packages/types/src/tool/approval.ts

import type { McpApprovalSettings } from "./mcp-approval.js";

export interface ToolApprovalOptions {
  // ... existing fields ...

  /** MCP approval settings */
  mcp?: McpApprovalSettings;
}
```

### 6.2 在 AutoApprovalChecker 中集成

```typescript
// sdk/core/services/auto-approval/auto-approval-checker.ts

import { checkMcpApproval } from "./mcp-approval-checker.js";

async function checkAutoApproval(
  params: CheckAutoApprovalParams
): Promise<AutoApprovalDecision> {
  // ... existing logic ...

  // Handle MCP tools
  if (tool.id === "use_mcp" && options.mcp) {
    const mcpRequest = parseMcpRequest(toolCall);
    const mcpDecision = checkMcpApproval({
      settings: options.mcp,
      request: mcpRequest,
    });

    switch (mcpDecision.decision) {
      case "approve":
        return { decision: "approve" };
      case "deny":
        return { decision: "deny", reason: mcpDecision.reason };
      case "ask":
        return { decision: "ask" };
    }
  }

  // ... rest of logic ...
}

/**
 * Parse MCP request from tool call
 */
function parseMcpRequest(toolCall: LLMToolCall): McpRequest {
  const args = JSON.parse(toolCall.function?.arguments ?? "{}");

  return {
    type: args.type ?? "use_mcp",
    serverName: args.server_name,
    toolName: args.tool_name,
    arguments: args.arguments,
    uri: args.uri,
  };
}
```

## 7. MCP 工具发现与配置

### 7.1 动态工具发现

```typescript
// sdk/core/services/mcp/mcp-discovery.ts

/**
 * Discover tools from MCP server
 */
export async function discoverMcpTools(
  serverName: string,
  client: McpClient
): Promise<McpToolConfig[]> {
  const tools = await client.listTools(serverName);

  return tools.map(tool => ({
    name: tool.name,
    description: tool.description,
    alwaysAllow: false,  // Default to requiring approval
    riskLevel: inferRiskLevel(tool),
  }));
}

/**
 * Infer risk level from tool schema
 */
function inferRiskLevel(tool: McpToolInfo): ToolRiskLevel {
  const name = tool.name.toLowerCase();

  // Heuristics for risk level inference
  if (name.includes("write") || name.includes("create") || name.includes("delete")) {
    return "WRITE";
  }
  if (name.includes("execute") || name.includes("run") || name.includes("shell")) {
    return "EXECUTE";
  }
  if (name.includes("read") || name.includes("list") || name.includes("get")) {
    return "READ_ONLY";
  }

  // Default to WRITE for safety
  return "WRITE";
}
```

### 7.2 配置持久化

```typescript
// apps/web-app/src/config/mcp-config.ts

/**
 * Load MCP approval configuration
 */
export async function loadMcpApprovalConfig(): Promise<McpApprovalSettings> {
  const configPath = path.join(process.cwd(), ".mcp-approval.json");

  try {
    const content = await fs.readFile(configPath, "utf-8");
    return JSON.parse(content);
  } catch {
    // Return default config
    return {
      servers: [],
      defaultServerBehavior: "always_ask",
    };
  }
}

/**
 * Save MCP approval configuration
 */
export async function saveMcpApprovalConfig(
  settings: McpApprovalSettings
): Promise<void> {
  const configPath = path.join(process.cwd(), ".mcp-approval.json");
  await fs.writeFile(configPath, JSON.stringify(settings, null, 2));
}
```

## 8. 配置文件格式

### 8.1 .mcp-approval.json 示例

```json
{
  "servers": [
    {
      "name": "database",
      "description": "Database query MCP server",
      "tools": [
        {
          "name": "query",
          "alwaysAllow": false,
          "riskLevel": "READ_ONLY"
        },
        {
          "name": "list_tables",
          "alwaysAllow": true,
          "riskLevel": "READ_ONLY"
        }
      ],
      "defaultToolBehavior": "always_ask"
    },
    {
      "name": "filesystem",
      "tools": [
        {
          "name": "read_file",
          "alwaysAllow": true,
          "riskLevel": "READ_ONLY"
        }
      ]
    }
  ],
  "defaultServerBehavior": "always_ask"
}
```

## 9. 实施步骤

### Phase 1: 类型定义

1. 创建 `packages/types/src/tool/mcp-approval.ts`
2. 更新 `packages/types/src/tool/index.ts` 导出
3. 扩展 `ToolApprovalOptions` 添加 `mcp` 字段

### Phase 2: 检查器实现

1. 创建 `sdk/core/services/auto-approval/mcp-approval-checker.ts`
2. 集成到 `auto-approval-checker.ts`

### Phase 3: 配置支持

1. 创建 MCP 配置加载/保存工具
2. 添加配置文件 schema 验证

### Phase 4: UI 支持 (可选)

1. 添加 MCP 工具审批配置界面
2. 支持动态发现和配置

## 10. 安全考虑

1. **默认拒绝**：未知服务器和工具默认需要审批
2. **显式配置**：所有自动审批需显式配置
3. **风险级别**：即使 `alwaysAllow: true`，仍记录风险级别用于审计
4. **审计日志**：所有 MCP 调用应记录审批决策
