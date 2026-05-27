# Shell 工具安全加固修改方案

> 基于对 `run-shell` 和 `backend-shell` 的架构分析，提出分阶段修改方案。
> 关联任务: [终端沙箱分阶段任务](../tasks/terminal-sandbox-phases.md)

---

## Q1: 当前存在哪些安全问题？

### A1: 四个层级的安全缺陷

| # | 问题 | 影响范围 | 严重程度 |
|---|------|----------|----------|
| 1 | Shell 工具绕过沙箱系统 | `run_shell`, `backend_shell` | **高危** |
| 2 | 工具层无二次权限校验 | `run_shell`, `backend_shell` | **中危** |
| 3 | `backend_shell` 无超时控制 | `backend_shell` 启动的进程 | **中危** |
| 4 | TerminalService 配置废弃 | `allowedCommands`, `deniedCommands`, `maxSessions`, `checkApproval` | **低危** |

### 详细说明

**问题 1: Shell 工具绕过沙箱**

现有的沙箱系统 (`services/sandbox/`) 已实现完善的 shell 策略引擎，包括：
- Shell 分析器（bash/cmd/powershell）的命令黑名单和危险模式检测
- `SandboxShellExecutor` 已具备完整的沙箱执行路径

但 `run_shell` 和 `backend_shell` 的 handler 直接调用 `terminalService.executeOneOff()` 和 `terminalService.startBackgroundCommand()`，完全绕过沙箱：

```
ScriptEngine → sandbox-shell → SandboxRuntime → ShellStaticAnalyzer → executeOneOff  ✅（有沙箱）
run_shell handler → executeOneOff                                                    ❌（无沙箱）
backend_shell handler → startBackgroundCommand                                       ❌（无沙箱）
```

**问题 2: 工具层无二次校验**

Auto-Approval 在上游做权限检查，但 tool handler 自身不做任何校验。如果上游被配置为宽松模式（如 `PERMISSIVE` 预设设定了 `alwaysAllowExecute: true`），或通过编程接口直接调用 handler，所有命令都会被放行。

**问题 3: backend_shell 无超时**

`backend_shell` 的 handler 调用了 `getOrCreateSession` 和 `startBackgroundCommand`，但没有传入任何 timeout 参数。`startBackgroundCommand` 方法内部也不支持 timeout。

**问题 4: TerminalServiceConfig 配置废弃**

`TerminalServiceConfig` 中声明的 `allowedCommands`、`deniedCommands`、`maxSessions` 字段以及 `ExecuteOptions` 中的 `checkApproval` 字段，在 `terminal-service.ts` 中均未被实际使用。

---

## Q2: 修改目标是什么？

### A2: 四个目标

1. **防御纵深**: 确保 shell 工具在无上游审批时仍有沙箱和黑名单保护
2. **能力复用**: 将 SandboxRuntime 的策略能力下沉到所有 shell 执行路径
3. **配置落实**: 清理 TerminalService 中废弃的配置声明，或将其实装
4. **超时兜底**: 为 `backend_shell` 增加超时保护

---

## Q3: 修改方案是什么？

### A3: 分四个阶段实施

---

### Phase A — 工具层接入 Shell 沙箱（高优先级）

**目标**: 在 `run_shell` 和 `backend_shell` 的 handler 中引入策略检查

#### A-1: 为 `createRunShellHandler` 增加沙箱策略参数

**文件**: [sdk/resources/predefined/tools/stateless/shell/run-shell/handler.ts](file:///d:/项目/agent/wf-agent/sdk/resources/predefined/tools/stateless/shell/run-shell/handler.ts)

```typescript
// 修改 createRunShellHandler 签名，支持传入沙箱策略
import { getSandboxRuntime } from "@wf-agent/sdk/services";
import type { ShellPolicy } from "@wf-agent/types";

export function createRunShellHandler(config?: RunShellConfig & { shellPolicy?: ShellPolicy }) {
  const maxTimeout = config?.maxTimeout ?? 600000;
  const terminalService = getTerminalService();
  const sandboxRuntime = getSandboxRuntime();

  return async (params: Record<string, unknown>): Promise<ToolOutput> => {
    const { command, ...rest } = params as { command: string; timeout?: number; ... };

    // [新增] 在 TerminalService 执行前运行策略检查
    if (config?.shellPolicy) {
      const analyzer = getShellAnalyzer(detectShellType(command)); // 根据命令检测 shell 类型
      const decision = await analyzer.analyze(command, config.shellPolicy);
      if (!decision.allowed) {
        return {
          success: false,
          content: "",
          error: `Command rejected by shell policy: ${decision.reason}`,
        };
      }
    }

    // 原有的 TerminalService 执行逻辑
    const result = await terminalService.executeOneOff(command, { ... });
    // ...
  };
}
```

#### A-2: 为 `createBackendShellFactory` 增加策略检查

**文件**: [sdk/resources/predefined/tools/stateful/shell/backend-shell/handler.ts](file:///d:/项目/agent/wf-agent/sdk/resources/predefined/tools/stateful/shell/backend-shell/handler.ts)

在 `backend_shell` 的 `execute` 方法中，执行 `startBackgroundCommand` 之前，插入同样的策略检查逻辑。策略可来源于 `BackendShellConfig`。

#### A-3: 在 `registration.ts` 中传入默认策略

**文件**: [sdk/resources/predefined/tools/registration.ts](file:///d:/项目/agent/wf-agent/sdk/resources/predefined/tools/registration.ts)

```typescript
// registerPredefinedTools 中的 config 增加默认 shell policy
config?: {
  runShell?: RunShellConfig & {
    shellPolicy?: ShellPolicy;  // 新增
  };
  backendShell?: BackendShellConfig & {
    shellPolicy?: ShellPolicy;  // 新增
  };
}
```

同时从 `services/sandbox/default-policy.ts` 导入 `DEFAULT_SHELL_POLICY` 作为默认值。

---

### Phase B — TerminalService 层安全加固（中优先级）

**目标**: 在 TerminalService 内部实现最基本的安全校验

#### B-1: 在 `executeOneOff` 中增加命令空值检查

```typescript
// terminal-service.ts executeOneOff 方法入口
async executeOneOff(command: string, options?: ...): Promise<ExecuteResult> {
  if (!command?.trim()) {
    return {
      success: false,
      stdout: "",
      stderr: "",
      exitCode: -1,
      error: "Empty command",
    };
  }
  // ...
}
```

#### B-2: 清理或实现 TerminalServiceConfig 中的废弃字段

**选项 A（推荐）**: 从 `TerminalServiceConfig` 中移除 `allowedCommands`、`deniedCommands`、`enableAutoApproval` 字段。这些职责已由 SandboxRuntime 和 Auto-Approval 系统承担。

**选项 B**: 在 `executeCommand` 方法中实装这些字段：
```typescript
// executeCommand 中增加白名单/黑名单检查
if (this.config.allowedCommands?.length) {
  const isAllowed = this.config.allowedCommands.some(prefix => 
    command.trim().toLowerCase().startsWith(prefix.toLowerCase())
  );
  if (!isAllowed) {
    // 拒绝执行
  }
}
```

#### B-3: 实装 `maxSessions` 限制

```typescript
// createSession 方法中
async createSession(options?: ...): Promise<TerminalSession> {
  if (this.config.maxSessions && this.registry.getCount() >= this.config.maxSessions) {
    throw new Error(`Max sessions (${this.config.maxSessions}) reached`);
  }
  // ...
}
```

---

### Phase C — backend_shell 超时控制（中优先级）

**目标**: 为后台 shell 增加可配置的超时兜底

#### C-1: 扩展 `BackendShellConfig`

**文件**: [sdk/resources/predefined/tools/types.ts](file:///d:/项目/agent/wf-agent/sdk/resources/predefined/tools/types.ts)

```typescript
export interface BackendShellConfig {
  workspaceDir?: string;
  /** Maximum runtime for background commands (ms), 0 = no limit, default: 3600000 (1h) */
  maxBackgroundTimeout?: number;
}
```

#### C-2: backend_shell handler 中启用超时

**文件**: [backend-shell/handler.ts](file:///d:/项目/agent/wf-agent/sdk/resources/predefined/tools/stateful/shell/backend-shell/handler.ts)

```typescript
export function createBackendShellFactory(config?: BackendShellConfig) {
  const maxBackgroundTimeout = config?.maxBackgroundTimeout ?? 3600000; // 默认 1h

  return () => {
    const terminalService = getTerminalService();

    return {
      execute: async (params: Record<string, unknown>): Promise<ShellOutputResult> => {
        // ... 创建 session 和启动 command ...

        // [新增] 如果设置了超时，启动定时器兜底 kill
        if (maxBackgroundTimeout > 0) {
          setTimeout(async () => {
            await terminalService.killBackgroundCommand(session.sessionId);
          }, maxBackgroundTimeout).unref(); // unref 防止阻止进程退出
        }

        // ...
      },
    };
  };
}
```

#### C-3: `startBackgroundCommand` 支持 timeout 参数

**文件**: [terminal-service.ts](file:///d:/项目/agent/wf-agent/sdk/services/terminal/terminal-service.ts)

```typescript
async startBackgroundCommand(
  sessionId: string,
  command: string,
  options?: { timeout?: number }  // 新增 timeout 参数
): Promise<ExecuteResult> {
  // ... 现有逻辑 ...

  // [新增] 自动超时兜底
  if (options?.timeout && options.timeout > 0) {
    setTimeout(() => {
      this.killBackgroundCommand(sessionId);
    }, options.timeout).unref();
  }

  return { success: true, ... };
}
```

---

### Phase D — 防御纵深体系建立（长期）

**目标**: 建立多层防御体系，确保单层失效时仍有保护

#### D-1: Shell 策略作用域示意

```
LLM → ToolApprovalCoordinator (Auto-Approval 层)
  ├─ 风险等级: EXECUTE
  ├─ 命令白名单/黑名单
  ├─ 危险模式检测
  │
  ▼
RunShellHandler (工具层)                     ← Phase A
  ├─ ShellPolicy 预检（静态分析）
  │   ├─ 命令黑名单（sudo, dd, mkfs...）
  │   ├─ 危险模式（fork bomb, pipe-to-shell...）
  │   └─ 路径限制
  │
  ▼
TerminalService (服务层)                     ← Phase B
  ├─ 命令空值/非法检查
  ├─ maxSessions 限制
  │
  ▼
child_process.spawn (操作系统层)
  ├─ windowsHide: true
  └─ env 合并（后续可增加环境变量白名单）
```

#### D-2: 默认策略分级

在 `registration.ts` 中为 shell 工具提供三级默认策略：

```typescript
export const SHELL_POLICY_PRESETS = {
  SAFE: {        // 严格模式：仅白名单命令
    allowedCommands: ["git", "npm", "pnpm", "node", "ls", "cat", "echo", "cd", "pwd", "mkdir", "cp", "mv", "rm"],
    deniedCommands: DEFAULT_SHELL_POLICY.deniedCommands,
    dangerousPatterns: DEFAULT_SHELL_POLICY.dangerousPatterns,
    allowPipe: false,
    allowRedirect: false,
  },
  BALANCED: {    // 平衡模式：禁止高危命令
    allowedCommands: [],
    deniedCommands: DEFAULT_SHELL_POLICY.deniedCommands,
    dangerousPatterns: DEFAULT_SHELL_POLICY.dangerousPatterns,
    allowPipe: true,
    allowRedirect: false,
  },
  PERMISSIVE: {  // 宽松模式：仅禁止最危险的操作
    allowedCommands: [],
    deniedCommands: ["sudo", "su", "chroot", "dd", "mkfs", "reboot", "shutdown"],
    dangerousPatterns: ["rm\\s+(-rf|--recursive)\\s+\\/\\s*"],
    allowPipe: true,
    allowRedirect: true,
  },
};
```

#### D-3: 注册时按安全预设注入策略

```typescript
// registration.ts
const shellPolicy = getShellPolicyForPreset(options?.securityPreset); // 根据安全预设选择策略
const runShellConfig = { ...config?.runShell, shellPolicy };
```

---

## Q4: 修改有哪些依赖和风险？

### A4: 影响分析

| 修改 | 前置依赖 | 风险 | 缓解措施 |
|------|----------|------|----------|
| Phase A | SandboxRuntime + ShellAnalyzer 已就绪 | 引入策略可能误拦截合法命令 | 默认使用 `BALANCED` 级别，错误信息具体到被拦截的命令 |
| Phase B | 无 | 移除配置字段有破坏性变更，实装字段则改变运行行为 | 采用选项 A（移除），配合属性访问检查（`?.[key]`）避免运行时错误 |
| Phase C | 无 | 超时 kill 可能导致调试进程意外终止 | 默认超时设为 1h，通过配置可按需关闭（设为 0） |
| Phase D | Phase A-C 完成 | 多层检查增加调用链长度 | 策略检查在内存中完成，无 I/O 操作，性能开销可忽略 |

---

## Q5: 如何验证修改正确？

### A5: 验证策略

| 验证类型 | 验证内容 | 工具/方式 |
|----------|----------|-----------|
| 单元测试 | ShellPolicy 拦截逻辑 | 对每个 shell 分析器编写测试用例（合法命令通过、非法命令拒绝） |
| 单元测试 | TerminalService 安全检查 | 空命令、超长命令、特殊字符的处理 |
| 集成测试 | run_shell 端到端 | 配置不同策略级别，验证执行结果 |
| 集成测试 | backend_shell 超时 | 启动长时间 sleep，验证超时后被 kill |
| 回归测试 | 现有工作流不受影响 | 运行 `pnpm test` 确保已有用例通过 |

---

## Q6: 各阶段工作量评估

### A6: 预估

| 阶段 | 新增文件 | 修改文件 | 预估工时 | 并行度 |
|------|----------|----------|----------|--------|
| **Phase A** 工具层接入沙箱 | 0 | 4 (handler.ts × 2, types.ts, registration.ts) | 2-3h | 可独立 |
| **Phase B** TerminalService 加固 | 0 | 1 (terminal-service.ts) | 1-2h | 可独立 |
| **Phase C** backend_shell 超时 | 0 | 2 (handler.ts, types.ts) + 1 (terminal-service.ts) | 1h | 可独立 |
| **Phase D** 防御纵深 | 0 | 2 (registration.ts, types.ts) | 1h | 依赖 A-C |

**总计**: 约 5-7 小时，四个阶段可并行，建议顺序执行以确保安全策略一致。

---

## 附录: 关键代码位置

| 文件 | 作用 |
|------|------|
| [run-shell/handler.ts](file:///d:/项目/agent/wf-agent/sdk/resources/predefined/tools/stateless/shell/run-shell/handler.ts) | run_shell 工具执行入口 |
| [backend-shell/handler.ts](file:///d:/项目/agent/wf-agent/sdk/resources/predefined/tools/stateful/shell/backend-shell/handler.ts) | backend_shell 工具执行入口 |
| [terminal-service.ts](file:///d:/项目/agent/wf-agent/sdk/services/terminal/terminal-service.ts) | 终端服务核心实现 |
| [terminal/types.ts](file:///d:/项目/agent/wf-agent/sdk/services/terminal/types.ts) | 终端服务类型定义（含废弃字段） |
| [registration.ts](file:///d:/项目/agent/wf-agent/sdk/resources/predefined/tools/registration.ts) | 工具注册入口 |
| [types.ts](file:///d:/项目/agent/wf-agent/sdk/resources/predefined/tools/types.ts) | 工具配置类型定义 |
| [sandbox-runtime.ts](file:///d:/项目/agent/wf-agent/sdk/services/sandbox/sandbox-runtime.ts) | 沙箱运行时（已有但未接入） |
| [default-policy.ts](file:///d:/项目/agent/wf-agent/sdk/services/sandbox/default-policy.ts) | 默认沙箱策略 |
| [shell-static-analyzer.ts](file:///d:/项目/agent/wf-agent/sdk/services/sandbox/strategies/shell-static-analyzer.ts) | Shell 静态分析策略 |
| [bash.ts](file:///d:/项目/agent/wf-agent/sdk/services/sandbox/strategies/shell-analyzers/bash.ts) | Bash 分析器 |
| [cmd.ts](file:///d:/项目/agent/wf-agent/sdk/services/sandbox/strategies/shell-analyzers/cmd.ts) | CMD 分析器 |
| [powershell.ts](file:///d:/项目/agent/wf-agent/sdk/services/sandbox/strategies/shell-analyzers/powershell.ts) | PowerShell 分析器 |
| [command-safety-checker.ts](file:///d:/项目/agent/wf-agent/sdk/services/auto-approval/command-safety-checker.ts) | Auto-Approval 命令安全检查 |
| [auto-approval-checker.ts](file:///d:/项目/agent/wf-agent/sdk/services/auto-approval/auto-approval-checker.ts) | Auto-Approval 决策引擎 |
| [risk-classification.ts](file:///d:/项目/agent/wf-agent/sdk/resources/predefined/tools/risk-classification.ts) | 工具风险等级分类 |