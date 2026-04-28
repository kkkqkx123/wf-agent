# 自动审批功能改进分析

## 1. 概述

本文档基于 roo-code 项目的 auto-approval 实现分析，提出对当前项目 user-interaction 功能的改进方案。

## 2. 参考项目核心特性

### 2.1 细粒度审批控制

roo-code 的 auto-approval 系统支持多种审批类别：

| 类别 | 说明 |
|------|------|
| `alwaysAllowReadOnly` | 只读操作自动审批 |
| `alwaysAllowWrite` | 写操作自动审批 |
| `alwaysAllowExecute` | 命令执行自动审批 |
| `alwaysAllowMcp` | MCP 工具自动审批 |
| `alwaysAllowFollowupQuestions` | 追问自动审批 |
| `alwaysAllowModeSwitch` | 模式切换自动审批 |
| `alwaysAllowSubtasks` | 子任务自动审批 |

### 2.2 工作区边界控制

- `alwaysAllowReadOnlyOutsideWorkspace` - 工作区外只读操作
- `alwaysAllowWriteOutsideWorkspace` - 工作区外写操作
- `alwaysAllowWriteProtected` - 受保护文件写操作

### 2.3 命令执行安全控制

**危险模式检测**：
- 参数扩展操作符 (`${var@P}`, `${var@Q}` 等)
- 带转义序列的参数赋值
- 间接变量引用 (`${!var}`)
- Here-strings 与命令替换
- Zsh 进程替换和 glob 限定符

**最长前缀匹配**：实现 allowlist/denylist 冲突解决策略

### 2.4 追问超时自动响应

支持追问的超时自动选择建议答案，提升用户体验。

## 3. 当前项目现状

### 3.1 工具类型体系

当前项目支持四种工具类型：

| 类型 | 说明 | 执行器 |
|------|------|--------|
| `STATELESS` | 无状态工具 | StatelessExecutor |
| `STATEFUL` | 有状态工具 | StatefulExecutor |
| `REST` | REST API 工具 | RestExecutor |
| `BUILTIN` | 内置工具 | BuiltinExecutor |

### 3.2 工具元数据

```typescript
interface ToolMetadata {
  category?: string;      // 工具分类
  tags?: string[];        // 标签数组
  documentationUrl?: string;
  customFields?: Metadata;
}
```

### 3.3 当前审批机制

`ToolApprovalCoordinator` 仅支持：
- 简单的工具名称白名单 (`autoApprovedTools`)
- 审批超时设置

**缺失功能**：
- 无细粒度分类控制
- 无工作区边界控制
- 无命令安全检测
- 无追问超时响应

## 4. 工具分类方案设计

### 4.1 双层分类体系

基于当前项目的工具架构，设计双层分类：

**第一层：执行类型**（已有）

| 类型 | 安全特性 |
|------|----------|
| `STATELESS` | 纯函数，无副作用，可安全自动审批 |
| `STATEFUL` | 有状态，需评估状态修改风险 |
| `REST` | 外部 API 调用，需评估外部风险 |
| `BUILTIN` | SDK 内部工具，需单独评估 |

**第二层：操作风险级别**（新增）

| 风险级别 | 说明 | 自动审批策略 |
|----------|------|--------------|
| `READ_ONLY` | 只读操作，无副作用 | 默认可自动审批 |
| `WRITE` | 写入操作，有副作用 | 需显式配置 |
| `EXECUTE` | 命令执行，高风险 | 需命令白名单 |
| `NETWORK` | 网络请求 | 需域名白名单 |
| `SYSTEM` | 系统级操作 | 永不自动审批 |

### 4.2 工具风险分类映射

```typescript
// 建议的工具风险分类
const TOOL_RISK_CLASSIFICATION = {
  // STATELESS 工具
  "read_file": "READ_ONLY",
  "list_files": "READ_ONLY",
  "search_files": "READ_ONLY",
  "glob": "READ_ONLY",
  "grep": "READ_ONLY",
  
  "write_file": "WRITE",
  "edit_file": "WRITE",
  "delete_file": "WRITE",
  
  "run_shell": "EXECUTE",
  "bash": "EXECUTE",
  
  // STATEFUL 工具
  "session_note": "WRITE",  // 修改会话状态
  
  // REST 工具
  "http_request": "NETWORK",
  
  // BUILTIN 工具
  "execute_workflow": "SYSTEM",
  "create_subtask": "SYSTEM",
};
```

### 4.3 执行类型与风险级别组合

| 执行类型 | 风险级别 | 自动审批建议 |
|----------|----------|--------------|
| STATELESS | READ_ONLY | ✅ 默认允许 |
| STATELESS | WRITE | ⚠️ 需配置 |
| STATELESS | EXECUTE | ⚠️ 需命令白名单 |
| STATEFUL | READ_ONLY | ✅ 默认允许 |
| STATEFUL | WRITE | ⚠️ 需配置 |
| REST | NETWORK | ⚠️ 需域名白名单 |
| BUILTIN | SYSTEM | ❌ 永不自动审批 |

## 5. 类型定义改进

### 5.1 新增风险级别类型

```typescript
// packages/types/src/tool/approval.ts

/**
 * Tool operation risk level
 */
export type ToolRiskLevel = 
  | "READ_ONLY"    // Read operations, no side effects
  | "WRITE"        // Write operations, has side effects
  | "EXECUTE"      // Command execution, high risk
  | "NETWORK"      // Network requests
  | "SYSTEM";      // System-level operations

/**
 * Auto-approval category
 */
export type AutoApprovalCategory =
  | "alwaysAllowReadOnly"
  | "alwaysAllowWrite"
  | "alwaysAllowExecute"
  | "alwaysAllowNetwork"
  | "alwaysAllowFollowupQuestions";
```

### 5.2 扩展 ToolMetadata

```typescript
// packages/types/src/tool/static-config.ts

export interface ToolMetadata {
  /** Tool category */
  category?: string;
  /** Tags array */
  tags?: string[];
  /** Documentation URL */
  documentationUrl?: string;
  /** Custom fields */
  customFields?: Metadata;
  
  // 新增字段
  /** Risk level for auto-approval */
  riskLevel?: ToolRiskLevel;
  /** Whether this tool is safe to auto-approve */
  autoApprovable?: boolean;
  /** Required approval conditions */
  approvalConditions?: ApprovalCondition[];
}

export interface ApprovalCondition {
  /** Condition type */
  type: "workspace_boundary" | "protected_file" | "command_whitelist" | "domain_whitelist";
  /** Condition configuration */
  config: Record<string, unknown>;
}
```

### 5.3 扩展 ToolApprovalOptions

```typescript
// packages/types/src/tool/approval.ts

export interface ToolApprovalOptions {
  /** Enable auto-approval system */
  autoApprovalEnabled?: boolean;
  
  /** Category-based auto-approval settings */
  categories?: Partial<Record<AutoApprovalCategory, boolean>>;
  
  /** Workspace boundary controls */
  allowOutsideWorkspace?: {
    readOnly?: boolean;
    write?: boolean;
  };
  
  /** Allow writing to protected files */
  allowWriteProtected?: boolean;
  
  /** Command execution settings */
  commandSettings?: {
    allowedCommands?: string[];
    deniedCommands?: string[];
  };
  
  /** Network request settings */
  networkSettings?: {
    allowedDomains?: string[];
    deniedDomains?: string[];
  };
  
  /** Followup question timeout (ms) for auto-selecting suggestion */
  followupAutoApproveTimeoutMs?: number;
  
  /** Legacy: List of auto-approved tool names */
  autoApprovedTools?: string[];
  
  /** Approval timeout in milliseconds */
  approvalTimeout?: number;
}
```

## 6. 核心模块设计

### 6.1 AutoApprovalChecker

```typescript
// sdk/core/services/auto-approval/auto-approval-checker.ts

export interface CheckAutoApprovalParams {
  options: ToolApprovalOptions;
  tool: Tool;
  context: {
    isOutsideWorkspace?: boolean;
    isProtected?: boolean;
    command?: string;
    domain?: string;
  };
}

export type AutoApprovalDecision = 
  | { decision: "approve" }
  | { decision: "deny"; reason?: string }
  | { decision: "ask" }
  | { decision: "timeout"; timeout: number; autoResponse: unknown };

export async function checkAutoApproval(
  params: CheckAutoApprovalParams
): Promise<AutoApprovalDecision>;
```

### 6.2 CommandSafetyChecker

```typescript
// sdk/core/services/auto-approval/command-safety-checker.ts

/**
 * Detect dangerous parameter substitutions
 */
export function containsDangerousSubstitution(command: string): boolean;

/**
 * Find longest matching prefix
 */
export function findLongestPrefixMatch(
  command: string, 
  prefixes: string[]
): string | null;

/**
 * Get command decision using longest prefix match rule
 */
export function getCommandDecision(
  command: string,
  allowedCommands: string[],
  deniedCommands?: string[]
): "auto_approve" | "auto_deny" | "ask_user";
```

### 6.3 ToolRiskClassifier

```typescript
// sdk/core/services/auto-approval/tool-risk-classifier.ts

export class ToolRiskClassifier {
  /**
   * Classify tool risk level
   */
  classify(tool: Tool): ToolRiskLevel {
    // 1. 优先使用工具元数据中的 riskLevel
    if (tool.metadata?.riskLevel) {
      return tool.metadata.riskLevel;
    }
    
    // 2. 根据工具类型推断
    return this.inferFromToolType(tool);
  }
  
  /**
   * Check if tool is safe to auto-approve
   */
  isAutoApprovable(tool: Tool, options: ToolApprovalOptions): boolean;
}
```

## 7. 实现优先级

| 优先级 | 改进项 | 复杂度 | 价值 |
|--------|--------|--------|------|
| P0 | 扩展类型定义 (ToolRiskLevel, ToolApprovalOptions) | 低 | 高 |
| P0 | 创建 ToolRiskClassifier | 低 | 高 |
| P1 | 创建 AutoApprovalChecker | 中 | 高 |
| P1 | 创建 CommandSafetyChecker | 中 | 高 |
| P2 | 增强 ToolApprovalCoordinator | 中 | 高 |
| P2 | 追问超时自动响应 | 低 | 中 |

## 8. 向后兼容

1. **保留 `autoApprovedTools`**：作为简单模式的快捷方式
2. **默认行为不变**：未配置时保持当前审批流程
3. **渐进式启用**：通过 `autoApprovalEnabled` 开关控制

## 9. 安全原则

1. **安全优先**：危险命令模式检测优先于 allowlist 匹配
2. **最长前缀匹配**：解决 allowlist/denylist 冲突
3. **显式配置**：高风险操作需显式配置才能自动审批
4. **永不自动审批**：SYSTEM 级别工具永不自动审批

## 10. 与 MCP 的集成

当前项目暂无 MCP 工具类型，但设计预留扩展点：

```typescript
// 未来 MCP 集成时的扩展
export interface McpToolApprovalOptions {
  /** MCP server specific settings */
  mcpServers?: {
    [serverName: string]: {
      /** Tools that are always allowed for this server */
      alwaysAllowTools?: string[];
      /** Require approval for all tools from this server */
      requireApproval?: boolean;
    };
  };
}
```

## 11. 下一步行动

1. **类型定义**：在 `packages/types/src/tool/` 中添加新类型
2. **工具分类**：为现有工具添加 `riskLevel` 元数据
3. **核心服务**：实现 AutoApprovalChecker 和 CommandSafetyChecker
4. **协调器增强**：更新 ToolApprovalCoordinator 使用新逻辑
5. **测试覆盖**：添加单元测试和集成测试
