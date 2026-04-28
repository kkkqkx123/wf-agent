# 用户交互功能改进方案总结

## 1. 文档索引

| 文档 | 说明 |
|------|------|
| [auto-approval-improvement-analysis.md](./auto-approval-improvement-analysis.md) | 自动审批功能改进分析（参考 roo-code） |
| [tool-risk-classification-design.md](./tool-risk-classification-design.md) | 工具风险分类与类型改造设计 |
| [mcp-approval-design.md](./mcp-approval-design.md) | MCP 工具自动审批设计 |

## 2. 改进概览

### 2.1 核心改进点

| 改进项 | 当前状态 | 目标状态 |
|--------|----------|----------|
| 审批控制粒度 | 仅支持工具名称白名单 | 支持分类、风险级别、工作区边界等多维度控制 |
| 命令执行安全 | 无安全检测 | 支持危险模式检测、最长前缀匹配 |
| MCP 工具控制 | 无细粒度控制 | 支持服务器级、工具级配置 |
| 追问自动响应 | 不支持 | 支持超时自动选择建议答案 |
| 工具风险分类 | 无 | 双层分类体系（执行类型 + 风险级别） |

### 2.2 新增功能

1. **工具风险级别**：`READ_ONLY`, `WRITE`, `EXECUTE`, `NETWORK`, `SYSTEM`, `INTERACTION`
2. **分类审批控制**：按风险级别配置自动审批策略
3. **工作区边界控制**：区分工作区内/外操作
4. **命令安全检测**：检测危险参数替换模式
5. **最长前缀匹配**：解决 allowlist/denylist 冲突
6. **MCP 细粒度控制**：服务器级、工具级、资源级配置

## 3. 类型改造汇总

### 3.1 新增类型文件

```
packages/types/src/tool/
├── risk-level.ts          # 新增：风险级别类型
├── mcp-approval.ts        # 新增：MCP 审批类型
├── approval.ts            # 修改：扩展 ToolApprovalOptions
└── static-config.ts       # 修改：扩展 ToolMetadata
```

### 3.2 类型变更清单

| 类型 | 变更类型 | 说明 |
|------|----------|------|
| `ToolRiskLevel` | 新增 | 工具操作风险级别 |
| `AutoApprovalCategory` | 新增 | 自动审批类别 |
| `ApprovalCondition` | 新增 | 审批条件 |
| `ToolMetadata` | 修改 | 添加 `riskLevel`, `autoApprovable`, `approvalConditions` |
| `ToolApprovalOptions` | 修改 | 添加分类控制、边界控制、命令/网络/MCP 设置 |
| `McpApprovalSettings` | 新增 | MCP 审批设置 |
| `McpServerConfig` | 新增 | MCP 服务器配置 |
| `McpToolConfig` | 新增 | MCP 工具配置 |

## 4. 新增服务模块

```
sdk/core/services/auto-approval/
├── index.ts                      # 统一导出
├── auto-approval-checker.ts      # 核心审批检查器
├── command-safety-checker.ts     # 命令安全检查器
├── mcp-approval-checker.ts       # MCP 审批检查器
└── __tests__/
    ├── auto-approval-checker.test.ts
    ├── command-safety-checker.test.ts
    └── mcp-approval-checker.test.ts
```

## 5. 工具改造汇总

### 5.1 新增文件

```
sdk/resources/predefined/tools/
├── risk-classification.ts        # 新增：工具风险分类映射
└── utils.ts                      # 新增：工具元数据注入工具
```

### 5.2 修改文件

| 文件 | 修改内容 |
|------|----------|
| `types.ts` | 扩展 `PredefinedToolDefinition` 添加 `riskLevel` |
| `registry.ts` | 在工具创建时注入 `riskLevel` |

### 5.3 工具风险分类表

| 工具 ID | 执行类型 | 风险级别 | 自动审批策略 |
|---------|----------|----------|--------------|
| `read_file` | STATELESS | READ_ONLY | 默认可自动审批 |
| `list_files` | STATELESS | READ_ONLY | 默认可自动审批 |
| `search_files` | STATELESS | READ_ONLY | 默认可自动审批 |
| `write_file` | STATELESS | WRITE | 需显式配置 |
| `edit` | STATELESS | WRITE | 需显式配置 |
| `apply_diff` | STATELESS | WRITE | 需显式配置 |
| `apply_patch` | STATELESS | WRITE | 需显式配置 |
| `run_shell` | STATELESS | EXECUTE | 需命令白名单 |
| `run_slash_command` | STATELESS | EXECUTE | 需命令白名单 |
| `backend_shell` | STATEFUL | EXECUTE | 需命令白名单 |
| `shell_kill` | STATEFUL | EXECUTE | 需命令白名单 |
| `record_note` | STATEFUL | WRITE | 需显式配置 |
| `recall_notes` | STATEFUL | READ_ONLY | 默认可自动审批 |
| `shell_output` | STATEFUL | READ_ONLY | 默认可自动审批 |
| `ask_followup_question` | STATELESS | INTERACTION | 超时自动响应 |
| `skill` | STATELESS | READ_ONLY | 默认可自动审批 |
| `update_todo_list` | STATELESS | READ_ONLY | 默认可自动审批 |
| `use_mcp` | STATELESS | NETWORK | 需 MCP 配置 |
| `execute_workflow` | BUILTIN | SYSTEM | 永不自动审批 |
| `cancel_workflow` | BUILTIN | SYSTEM | 永不自动审批 |
| `query_workflow_status` | BUILTIN | READ_ONLY | 默认可自动审批 |

## 6. 协调器改造

### 6.1 ToolApprovalCoordinator 改造

```typescript
// 改造前
async processToolApproval(params) {
  if (!this.requiresApproval(toolName, options)) {
    return { approved: true };
  }
  return this.requestUserApproval(params);
}

// 改造后
async processToolApproval(params) {
  // 1. 自动审批检查
  const decision = await checkAutoApproval({ options, tool, context });
  
  if (decision.decision === "approve") {
    return { approved: true };
  }
  if (decision.decision === "deny") {
    return { approved: false, rejectionReason: decision.reason };
  }
  if (decision.decision === "timeout") {
    return { approved: true, autoResponse: decision.autoResponse };
  }
  
  // 2. 用户审批
  return this.requestUserApproval(params);
}
```

## 7. 配置示例

### 7.1 基础配置

```typescript
const approvalOptions: ToolApprovalOptions = {
  autoApprovalEnabled: true,
  
  // 分类控制
  categories: {
    alwaysAllowReadOnly: true,
    alwaysAllowWrite: false,
    alwaysAllowExecute: false,
    alwaysAllowNetwork: false,
  },
  
  // 工作区边界
  workspaceBoundary: {
    allowReadOnlyOutsideWorkspace: true,
    allowWriteOutsideWorkspace: false,
  },
  
  // 受保护文件
  allowWriteProtected: false,
};
```

### 7.2 命令执行配置

```typescript
const commandOptions: ToolApprovalOptions = {
  autoApprovalEnabled: true,
  categories: {
    alwaysAllowExecute: true,
  },
  command: {
    allowedCommands: ["git", "npm", "pnpm", "node"],
    deniedCommands: ["rm -rf", "sudo"],
  },
};
```

### 7.3 MCP 配置

```typescript
const mcpOptions: ToolApprovalOptions = {
  autoApprovalEnabled: true,
  mcp: {
    servers: [
      {
        name: "database",
        tools: [
          { name: "list_tables", alwaysAllow: true },
          { name: "query", alwaysAllow: false },
        ],
        defaultToolBehavior: "always_ask",
      },
    ],
    defaultServerBehavior: "always_ask",
  },
};
```

### 7.4 追问自动响应配置

```typescript
const interactionOptions: ToolApprovalOptions = {
  autoApprovalEnabled: true,
  interaction: {
    followupAutoApproveTimeoutMs: 5000,  // 5秒后自动选择第一个建议
  },
};
```

## 8. 实施计划

### Phase 1: 类型定义 (P0)

- [ ] 创建 `packages/types/src/tool/risk-level.ts`
- [ ] 创建 `packages/types/src/tool/mcp-approval.ts`
- [ ] 修改 `packages/types/src/tool/static-config.ts`
- [ ] 修改 `packages/types/src/tool/approval.ts`
- [ ] 更新 `packages/types/src/tool/index.ts`

### Phase 2: 工具分类 (P0)

- [ ] 创建 `sdk/resources/predefined/tools/risk-classification.ts`
- [ ] 修改 `sdk/resources/predefined/tools/types.ts`
- [ ] 修改 `sdk/resources/predefined/tools/registry.ts`

### Phase 3: 核心服务 (P1)

- [ ] 创建 `sdk/core/services/auto-approval/command-safety-checker.ts`
- [ ] 创建 `sdk/core/services/auto-approval/auto-approval-checker.ts`
- [ ] 创建 `sdk/core/services/auto-approval/mcp-approval-checker.ts`
- [ ] 创建 `sdk/core/services/auto-approval/index.ts`

### Phase 4: 协调器改造 (P2)

- [ ] 修改 `sdk/core/coordinators/tool-approval-coordinator.ts`

### Phase 5: 测试覆盖 (P2)

- [ ] 添加单元测试
- [ ] 添加集成测试

## 9. 向后兼容

1. **默认行为不变**：`autoApprovalEnabled` 默认为 `false`
2. **保留 legacy 字段**：`autoApprovedTools` 继续支持
3. **渐进式迁移**：工具可逐步添加 `riskLevel`
4. **显式配置**：所有新功能需显式配置才生效

## 10. 安全原则

1. **安全优先**：危险模式检测优先于 allowlist 匹配
2. **最长前缀匹配**：解决 allowlist/denylist 冲突
3. **显式配置**：高风险操作需显式配置才能自动审批
4. **永不自动审批**：SYSTEM 级别工具永不自动审批
5. **默认拒绝**：未知服务器和工具默认需要审批
6. **审计日志**：所有自动审批决策应记录日志
