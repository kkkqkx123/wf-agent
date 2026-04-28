# 工具风险分类与类型改造设计

## 1. 概述

本文档基于自动审批改进分析，详细设计当前项目的工具风险分类方案和类型改造计划。

## 2. 当前工具清单分析

### 2.1 现有工具分类

| 工具 ID | 执行类型 | 功能分类 | 当前风险判断 |
|---------|----------|----------|--------------|
| `read_file` | STATELESS | filesystem | 只读，低风险 |
| `write_file` | STATELESS | filesystem | 写入，中风险 |
| `edit` | STATELESS | filesystem | 写入，中风险 |
| `apply_diff` | STATELESS | filesystem | 写入，中风险 |
| `apply_patch` | STATELESS | filesystem | 写入，中风险 |
| `list_files` | STATELESS | filesystem | 只读，低风险 |
| `search_files` | STATELESS | filesystem | 只读，低风险 |
| `run_shell` | STATELESS | shell | 命令执行，高风险 |
| `record_note` | STATEFUL | memory | 状态写入，中风险 |
| `recall_notes` | STATEFUL | memory | 状态读取，低风险 |
| `backend_shell` | STATEFUL | shell | 命令执行，高风险 |
| `shell_output` | STATEFUL | shell | 状态读取，低风险 |
| `shell_kill` | STATEFUL | shell | 进程控制，高风险 |
| `ask_followup_question` | STATELESS | interaction | 用户交互，特殊 |
| `run_slash_command` | STATELESS | interaction | 命令执行，高风险 |
| `skill` | STATELESS | interaction | 指令加载，低风险 |
| `update_todo_list` | STATELESS | interaction | 状态更新，低风险 |
| `use_mcp` | STATELESS | mcp | MCP 调用，独立分类 |
| `execute_workflow` | BUILTIN | workflow | 系统操作，高风险 |
| `cancel_workflow` | BUILTIN | workflow | 系统操作，高风险 |
| `query_workflow_status` | BUILTIN | workflow | 状态查询，低风险 |

### 2.2 风险级别定义

```typescript
/**
 * Tool operation risk level
 * Used for auto-approval decision making
 */
export type ToolRiskLevel =
  /** Read operations, no side effects - safe to auto-approve by default */
  | "READ_ONLY"
  /** Write operations, has side effects - requires explicit configuration */
  | "WRITE"
  /** Command execution, high risk - requires command whitelist */
  | "EXECUTE"
  /** MCP protocol operations - independent category with fine-grained control */
  | "MCP"
  /** HTTP network requests - requires domain whitelist */
  | "NETWORK"
  /** System-level operations - never auto-approve */
  | "SYSTEM"
  /** User interaction - special handling (timeout auto-response) */
  | "INTERACTION";
```

**风险级别说明**：

| 级别 | 说明 | 自动审批策略 |
|------|------|--------------|
| `READ_ONLY` | 只读操作，无副作用 | 默认可自动审批 |
| `WRITE` | 写入操作，有副作用 | 需显式配置 |
| `EXECUTE` | 命令执行，高风险 | 需命令白名单 |
| `MCP` | MCP 协议调用，独立分类 | 需 MCP 配置（支持 stdio） |
| `NETWORK` | HTTP 网络请求 | 需域名白名单 |
| `SYSTEM` | 系统级操作 | 永不自动审批 |
| `INTERACTION` | 用户交互 | 超时自动响应 |

### 2.3 工具风险分类映射

```typescript
// sdk/resources/predefined/tools/risk-classification.ts

import type { ToolRiskLevel } from "@wf-agent/types";

/**
 * Predefined tool risk classification
 * This mapping defines the risk level for each predefined tool
 */
export const TOOL_RISK_CLASSIFICATION: Record<string, ToolRiskLevel> = {
  // === STATELESS - Filesystem (Read) ===
  "read_file": "READ_ONLY",
  "list_files": "READ_ONLY",
  "search_files": "READ_ONLY",

  // === STATELESS - Filesystem (Write) ===
  "write_file": "WRITE",
  "edit": "WRITE",
  "apply_diff": "WRITE",
  "apply_patch": "WRITE",

  // === STATELESS - Shell ===
  "run_shell": "EXECUTE",

  // === STATELESS - Interaction ===
  "ask_followup_question": "INTERACTION",
  "run_slash_command": "EXECUTE",
  "skill": "READ_ONLY",        // Loading instructions is safe
  "update_todo_list": "READ_ONLY",  // Todo update is safe

  // === STATELESS - MCP (Independent Category) ===
  "use_mcp": "MCP",            // MCP protocol calls (stdio, etc.)

  // === STATEFUL - Memory ===
  "record_note": "WRITE",
  "recall_notes": "READ_ONLY",

  // === STATEFUL - Shell ===
  "backend_shell": "EXECUTE",
  "shell_output": "READ_ONLY",
  "shell_kill": "EXECUTE",

  // === BUILTIN - Workflow ===
  "execute_workflow": "SYSTEM",
  "cancel_workflow": "SYSTEM",
  "query_workflow_status": "READ_ONLY",
};

/**
 * Get risk level for a tool
 * @param toolId Tool ID
 * @returns Risk level, defaults to WRITE if not found
 */
export function getToolRiskLevel(toolId: string): ToolRiskLevel {
  return TOOL_RISK_CLASSIFICATION[toolId] ?? "WRITE";
}
```

## 3. 文件权限控制设计

### 3.1 文件权限优先级

文件权限控制具有**最高优先级**，优先于工具风险级别判断：

```
文件权限检查 → 工具风险级别检查 → 分类审批检查 → 用户审批
```

### 3.2 文件权限类型

```typescript
/**
 * File permission level
 */
export type FilePermissionLevel =
  /** No restrictions */
  | "none"
  /** Read-only access */
  | "read"
  /** Read and write access */
  | "write"
  /** Full access including delete */
  | "full"
  /** Explicitly denied */
  | "denied";

/**
 * File permission rule
 */
export interface FilePermissionRule {
  /** File path pattern (glob or exact) */
  pattern: string;
  /** Permission level */
  permission: FilePermissionLevel;
  /** Rule description */
  description?: string;
}

/**
 * File permission settings
 */
export interface FilePermissionSettings {
  /** Permission rules (evaluated in order, first match wins) */
  rules: FilePermissionRule[];
  /** Default permission for files not matching any rule */
  defaultPermission?: FilePermissionLevel;
}
```

### 3.3 文件权限检查器

```typescript
// sdk/core/services/auto-approval/file-permission-checker.ts

import type { FilePermissionSettings, FilePermissionLevel } from "@wf-agent/types";
import minimatch from "minimatch";

/**
 * Check file permission
 */
export function checkFilePermission(
  filePath: string,
  operation: "read" | "write" | "delete",
  settings: FilePermissionSettings
): { allowed: boolean; reason?: string } {
  // 1. Find matching rule
  const rule = findMatchingRule(filePath, settings.rules);

  // 2. Get permission level
  const permission = rule?.permission ?? settings.defaultPermission ?? "write";

  // 3. Check if operation is allowed
  switch (permission) {
    case "denied":
      return { allowed: false, reason: `File ${filePath} is explicitly denied` };
    case "none":
      return { allowed: true };
    case "read":
      if (operation === "read") return { allowed: true };
      return { allowed: false, reason: `File ${filePath} is read-only` };
    case "write":
      if (operation === "read" || operation === "write") return { allowed: true };
      return { allowed: false, reason: `File ${filePath} does not allow delete` };
    case "full":
      return { allowed: true };
  }
}

/**
 * Find first matching rule
 */
function findMatchingRule(
  filePath: string,
  rules: FilePermissionRule[]
): FilePermissionRule | undefined {
  for (const rule of rules) {
    if (minimatch(filePath, rule.pattern)) {
      return rule;
    }
  }
  return undefined;
}
```

### 3.4 文件权限配置示例

```typescript
const filePermissionSettings: FilePermissionSettings = {
  rules: [
    // Deny access to sensitive files
    { pattern: "**/.env", permission: "denied", description: "Environment files" },
    { pattern: "**/.env.*", permission: "denied", description: "Environment files" },
    { pattern: "**/credentials.json", permission: "denied", description: "Credentials" },
    { pattern: "**/secrets/**", permission: "denied", description: "Secrets directory" },

    // Read-only for configuration files
    { pattern: "**/package.json", permission: "read", description: "Package config" },
    { pattern: "**/tsconfig.json", permission: "read", description: "TypeScript config" },
    { pattern: "**/.git/**", permission: "read", description: "Git directory" },

    // Full access for source files
    { pattern: "**/src/**", permission: "full", description: "Source files" },
    { pattern: "**/lib/**", permission: "full", description: "Library files" },

    // Write access for docs
    { pattern: "**/*.md", permission: "write", description: "Markdown files" },
  ],
  defaultPermission: "write",  // Default allow read and write
};
```

## 4. 类型定义改造

### 4.1 新增类型文件

```typescript
// packages/types/src/tool/risk-level.ts

/**
 * Tool operation risk level
 */
export type ToolRiskLevel =
  | "READ_ONLY"
  | "WRITE"
  | "EXECUTE"
  | "MCP"
  | "NETWORK"
  | "SYSTEM"
  | "INTERACTION";

/**
 * Auto-approval category
 */
export type AutoApprovalCategory =
  | "alwaysAllowReadOnly"
  | "alwaysAllowWrite"
  | "alwaysAllowExecute"
  | "alwaysAllowMcp"
  | "alwaysAllowNetwork"
  | "alwaysAllowInteraction";

/**
 * Risk level to approval category mapping
 */
export const RISK_TO_CATEGORY: Record<ToolRiskLevel, AutoApprovalCategory | null> = {
  "READ_ONLY": "alwaysAllowReadOnly",
  "WRITE": "alwaysAllowWrite",
  "EXECUTE": "alwaysAllowExecute",
  "MCP": "alwaysAllowMcp",
  "NETWORK": "alwaysAllowNetwork",
  "INTERACTION": "alwaysAllowInteraction",
  "SYSTEM": null,  // SYSTEM level never auto-approves
};
```

### 4.2 文件权限类型

```typescript
// packages/types/src/tool/file-permission.ts

/**
 * File permission level
 */
export type FilePermissionLevel =
  | "none"
  | "read"
  | "write"
  | "full"
  | "denied";

/**
 * File permission rule
 */
export interface FilePermissionRule {
  /** File path pattern (glob or exact) */
  pattern: string;
  /** Permission level */
  permission: FilePermissionLevel;
  /** Rule description */
  description?: string;
}

/**
 * File permission settings
 */
export interface FilePermissionSettings {
  /** Permission rules (evaluated in order, first match wins) */
  rules: FilePermissionRule[];
  /** Default permission for files not matching any rule */
  defaultPermission?: FilePermissionLevel;
}
```

### 4.3 扩展 ToolMetadata

```typescript
// packages/types/src/tool/static-config.ts (修改)

import type { ToolRiskLevel } from "./risk-level.js";

export interface ToolMetadata {
  /** Tool category */
  category?: string;
  /** Tags array */
  tags?: string[];
  /** Documentation URL */
  documentationUrl?: string;
  /** Custom fields */
  customFields?: Metadata;

  // === 新增字段 ===
  /** Risk level for auto-approval decision */
  riskLevel?: ToolRiskLevel;
  /** Whether this tool can be auto-approved (overrides risk level) */
  autoApprovable?: boolean;
  /** Approval conditions that must be met */
  approvalConditions?: ApprovalCondition[];
}

/**
 * Approval condition for fine-grained control
 */
export interface ApprovalCondition {
  /** Condition type */
  type:
    | "workspace_boundary"    // Check if operation is within workspace
    | "protected_file"        // Check if file is protected
    | "command_whitelist"     // Check command against whitelist
    | "domain_whitelist"      // Check domain against whitelist
    | "mcp_server_whitelist"; // Check MCP server against whitelist
  /** Condition configuration */
  config: Record<string, unknown>;
}
```

### 4.4 扩展 ToolApprovalOptions

```typescript
// packages/types/src/tool/approval.ts (修改)

import type { ToolRiskLevel, AutoApprovalCategory } from "./risk-level.js";
import type { FilePermissionSettings } from "./file-permission.js";

/**
 * Workspace boundary control settings
 */
export interface WorkspaceBoundarySettings {
  /** Allow read operations outside workspace */
  allowReadOnlyOutsideWorkspace?: boolean;
  /** Allow write operations outside workspace */
  allowWriteOutsideWorkspace?: boolean;
}

/**
 * Command execution settings
 */
export interface CommandExecutionSettings {
  /** Allowed command prefixes (longest prefix match) */
  allowedCommands?: string[];
  /** Denied command prefixes (longest prefix match) */
  deniedCommands?: string[];
}

/**
 * Network request settings (for HTTP tools like web_fetch)
 */
export interface NetworkSettings {
  /** Allowed domains */
  allowedDomains?: string[];
  /** Denied domains */
  deniedDomains?: string[];
}

/**
 * Interaction settings
 */
export interface InteractionSettings {
  /** Followup question timeout (ms) for auto-selecting suggestion */
  followupAutoApproveTimeoutMs?: number;
}

/**
 * Tool approval options (expanded)
 */
export interface ToolApprovalOptions {
  // === Main switch ===
  /** Enable auto-approval system (default: false) */
  autoApprovalEnabled?: boolean;

  // === File permission (HIGHEST PRIORITY) ===
  /** File permission settings - evaluated first before any other checks */
  filePermissions?: FilePermissionSettings;

  // === Category-based settings ===
  /** Category-based auto-approval settings */
  categories?: Partial<Record<AutoApprovalCategory, boolean>>;

  // === Boundary settings ===
  /** Workspace boundary controls */
  workspaceBoundary?: WorkspaceBoundarySettings;
  /** Allow writing to protected files */
  allowWriteProtected?: boolean;

  // === Operation-specific settings ===
  /** Command execution settings */
  command?: CommandExecutionSettings;
  /** Network request settings (for HTTP tools) */
  network?: NetworkSettings;
  /** Interaction settings */
  interaction?: InteractionSettings;

  // === Legacy support ===
  /** Legacy: List of auto-approved tool names */
  autoApprovedTools?: string[];

  // === General settings ===
  /** Approval timeout in milliseconds (0 = no timeout) */
  approvalTimeout?: number;
}
```

## 5. 自动审批检查流程

### 5.1 检查优先级

```
1. 文件权限检查 (最高优先级)
   ↓ (如果涉及文件操作)
2. 工具风险级别检查
   ↓
3. 分类审批检查
   ↓
4. 操作特定检查 (命令/网络/MCP)
   ↓
5. 用户审批
```

### 5.2 核心检查逻辑

```typescript
// sdk/core/services/auto-approval/auto-approval-checker.ts

export async function checkAutoApproval(
  params: CheckAutoApprovalParams
): Promise<AutoApprovalDecision> {
  const { options, tool, context } = params;

  // 0. Check if auto-approval is enabled
  if (!options.autoApprovalEnabled) {
    return { decision: "ask" };
  }

  // 1. File permission check (HIGHEST PRIORITY)
  if (options.filePermissions && context.filePath) {
    const fileDecision = checkFilePermission(
      context.filePath,
      context.fileOperation ?? "read",
      options.filePermissions
    );
    if (!fileDecision.allowed) {
      return { decision: "deny", reason: fileDecision.reason };
    }
  }

  // 2. Get risk level
  const riskLevel = tool.metadata?.riskLevel ?? "WRITE";

  // 3. SYSTEM level never auto-approves
  if (riskLevel === "SYSTEM") {
    return { decision: "ask" };
  }

  // 4. Check tool-specific autoApprovable flag
  if (tool.metadata?.autoApprovable === false) {
    return { decision: "ask" };
  }

  // 5. Check legacy autoApprovedTools list
  if (options.autoApprovedTools?.includes(tool.id)) {
    return { decision: "approve" };
  }

  // 6. Handle by risk level
  switch (riskLevel) {
    case "READ_ONLY":
      return handleReadOnlyApproval(options, context);
    case "WRITE":
      return handleWriteApproval(options, context);
    case "EXECUTE":
      return handleExecuteApproval(options, context);
    case "MCP":
      return handleMcpApproval(options, context);
    case "NETWORK":
      return handleNetworkApproval(options, context);
    case "INTERACTION":
      return handleInteractionApproval(options, tool);
    default:
      return { decision: "ask" };
  }
}
```

## 6. MCP 独立分类处理

### 6.1 MCP 审批处理

```typescript
/**
 * Handle MCP approval (independent category)
 */
function handleMcpApproval(
  options: ToolApprovalOptions,
  context: CheckAutoApprovalParams["context"]
): AutoApprovalDecision {
  // MCP requires explicit configuration
  const category = options.categories?.alwaysAllowMcp;

  if (!category) {
    return { decision: "ask" };
  }

  // If MCP settings are provided, use fine-grained control
  if (options.mcp && context.mcpRequest) {
    const mcpDecision = checkMcpApproval({
      settings: options.mcp,
      request: context.mcpRequest,
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

  return { decision: "approve" };
}
```

## 7. 实施步骤

### Phase 1: 类型定义 (P0)

1. 创建 `packages/types/src/tool/risk-level.ts`
2. 创建 `packages/types/src/tool/file-permission.ts`
3. 修改 `packages/types/src/tool/static-config.ts` 扩展 ToolMetadata
4. 修改 `packages/types/src/tool/approval.ts` 扩展 ToolApprovalOptions
5. 更新 `packages/types/src/tool/index.ts` 导出新类型

### Phase 2: 工具分类 (P0)

1. 创建 `sdk/resources/predefined/tools/risk-classification.ts`
2. 修改 `sdk/resources/predefined/tools/types.ts` 扩展 PredefinedToolDefinition
3. 修改 `sdk/resources/predefined/tools/registry.ts` 注入 riskLevel

### Phase 3: 核心服务 (P1)

1. 创建 `sdk/core/services/auto-approval/file-permission-checker.ts`
2. 创建 `sdk/core/services/auto-approval/command-safety-checker.ts`
3. 创建 `sdk/core/services/auto-approval/auto-approval-checker.ts`
4. 创建 `sdk/core/services/auto-approval/mcp-approval-checker.ts`
5. 创建 `sdk/core/services/auto-approval/index.ts` 统一导出

### Phase 4: 协调器改造 (P2)

1. 修改 `sdk/core/coordinators/tool-approval-coordinator.ts`
2. 添加自动审批检查逻辑

### Phase 5: 测试覆盖 (P2)

1. 添加单元测试
2. 添加集成测试

## 8. 配置示例

### 8.1 完整配置示例

```typescript
const approvalOptions: ToolApprovalOptions = {
  autoApprovalEnabled: true,

  // 文件权限 (最高优先级)
  filePermissions: {
    rules: [
      { pattern: "**/.env", permission: "denied" },
      { pattern: "**/src/**", permission: "full" },
      { pattern: "**/*.md", permission: "write" },
    ],
    defaultPermission: "write",
  },

  // 分类控制
  categories: {
    alwaysAllowReadOnly: true,
    alwaysAllowWrite: true,
    alwaysAllowExecute: false,
    alwaysAllowMcp: true,
    alwaysAllowNetwork: false,
  },

  // 工作区边界
  workspaceBoundary: {
    allowReadOnlyOutsideWorkspace: true,
    allowWriteOutsideWorkspace: false,
  },

  // 命令执行
  command: {
    allowedCommands: ["git", "npm", "pnpm"],
    deniedCommands: ["rm -rf", "sudo"],
  },

  // 网络请求 (用于 web_fetch 等工具)
  network: {
    allowedDomains: ["api.example.com"],
    deniedDomains: [],
  },

  // 追问超时
  interaction: {
    followupAutoApproveTimeoutMs: 5000,
  },
};
```

## 9. 向后兼容保证

1. **默认行为不变**：`autoApprovalEnabled` 默认为 `false`
2. **保留 legacy 字段**：`autoApprovedTools` 继续支持
3. **渐进式迁移**：工具可逐步添加 `riskLevel`，未添加的默认为 `WRITE`
4. **显式配置**：所有新功能需显式配置才生效
5. **文件权限可选**：不配置 `filePermissions` 时跳过文件权限检查
