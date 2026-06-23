# OS 级 Hook 策略实现方案

- **状态**: 设计就绪 / 骨架完成
- **策略标识**: `"os-hook"`（用户配置层）→ `"linux-seccomp"` / `"windows-job"` / `"proot-redirect"`（平台具体 ID）
- **安全级别**: OS 级（平台相关）
- **定位**: 轻量策略的增强层，**叠加使用**而非替换

## 架构概述

```
用户配置: shellStrategy: ["os-hook", "static-analyzer"]
                           │
                    ┌──────▼──────┐
                    │ SandboxRuntime.resolveStrategy()
                    │ 映射 "os-hook" → platformId
                    └──────┬──────┘
                           │
              ┌────────────▼───────────┐
              │  DefaultStrategyResolver │
              │  按 platformId 查找策略    │
              └────────────┬───────────┘
                           │
              ┌────────────▼───────────┐
              │  LinuxSeccompStrategy  │
              │ / WindowsJobObject     │
              │ / ProotLikeRedirect    │
              └────────────┬───────────┘
                           │
              ┌────────────▼───────────┐
              │  TerminalService        │
              │  executeOneOff()        │
              └────────────────────────┘
```

## 当前状态（Phase 2 — Windows Job Object via koffi FFI）

| 组件 | 状态 | 说明 |
|---|---|---|
| `os-hook.ts` | ✅ Phase 2 | 3 个策略：linux-seccomp 骨架 / windows-job koffi FFI / proot-redirect 骨架 |
| `strategy-resolver.ts` | ✅ 已注册 | 3 个策略按 ID 注册到 shell 策略表 |
| `sandbox-runtime.ts` | ✅ ID 映射 | 将 `"os-hook"` 映射为平台具体 ID |
| `TerminalService` | ✅ 基础 API | `executeOneOff()` 可用，`spawn()` 直接用于 Job Object 执行 |
| koffi optional dep | ⚠️ 可选 | 动态 require，未安装时降级到 passthrough |

### 执行流程（当前 — koffi FFI 方案）

```
SandboxShellExecutor.execute()
  → SandboxRuntime.createRuntime("shell", ...)
    → resolveStrategy("shell", config)
      → 默认: shellStrategy = ["os-hook", "static-analyzer"]
      → 映射 "os-hook" → "windows-job" (win32)
      → resolver.resolveBest(["windows-job", "static-analyzer"])
        → 优先尝试 "windows-job".isAvailable()
          → true (win32) → 使用 WindowsJobObjectStrategy
  → strategy.execute(options, policy)
    → WindowsJobObjectStrategy.execute():
      1. getKoffiBinding() — 动态 require("koffi")
         ├─ 成功 → 加载 kernel32.dll FFI 绑定
         │  CreateJobObjectW → SetInformationJobObject
         │  → AssignProcessToJobObject
         └─ 失败（koffi 未安装）→ executeCommand() passthrough
      2. 根据 policy 构建 JOBOBJECT_EXTENDED_LIMIT_INFORMATION
         ├─ policy.process.maxChildProcesses → JOB_OBJECT_LIMIT_ACTIVE_PROCESS
         ├─ policy.resource.memoryLimit → JOB_OBJECT_LIMIT_PROCESS_MEMORY
         └─ policy.resource.timeoutLimit → Node.js 定时器
      3. spawn(userCommand, { shell: true, cwd, env })
      4. OpenProcess(pid) → AssignProcessToJobObject(jobHandle, hProcess)
      5. 收集 stdout/stderr，等待进程完成
      6. TerminateJobObject + CloseHandle（finally）
```

### 三阶段实现计划（已更新 — C# Add-Type 已跳过）

| Phase | 组件 | 状态 |
|---|---|---|
| Phase 1 | 骨架 + 注册 + ID 映射 | ✅ 完成 |
| Phase 2 | Windows Job Object via koffi FFI | ✅ 完成 |
| Phase 3 | Linux seccomp-loader + proot | 📋 待实现 |

### Phase 2 — koffi FFI 绑定（Windows，已实现）

位于 `sdk/services/sandbox/strategies/os-hook.ts` 中 `WindowsJobObjectStrategy` 类。

**原理**: 使用 koffi 可选依赖动态加载 `kernel32.dll`，直接调用 Win32 Job Object API：

```typescript
// 动态加载 koffi（可选依赖）
const koffi = require("koffi");
const k32 = koffi.load("kernel32.dll");

// Win32 API bindings
const CreateJobObjectW = k32.func("void* __stdcall CreateJobObjectW(void*, char*)");
const SetInformationJobObject = k32.func("int __stdcall SetInformationJobObject(void*, int, void*, int)");
const AssignProcessToJobObject = k32.func("int __stdcall AssignProcessToJobObject(void*, void*)");
const TerminateJobObject = k32.func("int __stdcall TerminateJobObject(void*, unsigned int)");
const OpenProcess = k32.func("void* __stdcall OpenProcess(unsigned int, int, unsigned int)");
const CloseHandle = k32.func("int __stdcall CloseHandle(void*)");
```

**Job Object 限制类型与对应 policy 字段**:

| Policy 字段 | Win32 常量 | Job 限制标志 | 说明 |
|---|---|---|---|
| `policy.process.maxChildProcesses` | `JOB_OBJECT_LIMIT_ACTIVE_PROCESS` (0x1000) | `ActiveProcessLimit` | 最大子进程数 |
| `policy.resource.memoryLimit` | `JOB_OBJECT_LIMIT_PROCESS_MEMORY` (0x200) | `ProcessMemoryLimit` | 进程内存限制（每个进程） |
| `policy.resource.memoryLimit × 2` | `JOB_OBJECT_LIMIT_JOB_MEMORY` (0x100) | `JobMemoryLimit` | Job 总内存限制 |
| `policy.resource.cpuLimit` | `JOB_OBJECT_LIMIT_CPU_RATE` (0x80) | — | CPU 速率控制（需额外结构） |
| `options.timeout` | Node.js 定时器 | — | 超时后 TerminateJobObject |

**降级机制**: 若 koffi 未安装（`npm install` 时不包含 koffi），`getKoffiBinding()` 返回 `null`，策略自动降级为 `executeCommand()` passthrough。

**退出时清理**: 不论成功与否，finally 块中调用 `TerminateJobObject` + `CloseHandle` 确保资源释放。

### Phase 3 — Linux seccomp-loader 辅助进程

**目标**: 用 Rust 编写 seccomp-loader，通过子进程方式调用。

```
seccomp-loader --allow read,write,open,close,stat,mmap,brk,exit_group
              --deny execve,mount,umount,socket,connect,kill,ptrace
              -- ./user-script.sh
```

```rust
// seccomp-loader/src/main.rs
use seccompiler::*;
use std::os::unix::process::CommandExt;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Parse allow/deny lists from args
    let allow_syscalls = parse_syscall_list("--allow", &args);
    let deny_syscalls = parse_syscall_list("--deny", &args);

    // Build BPF filter
    let filter = SeccompFilter::new(
        allow_syscalls.into_iter().map(|name| {
            (get_syscall_number(&name), SeccompAction::Allow)
        }).collect(),
        SeccompAction::KillProcess,
    ).unwrap();

    // Apply filter
    filter.apply().unwrap();

    // Execute user command
    let command = &args[args.iter().position(|a| a == "--").unwrap() + 1];
    let error = std::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .exec();

    // exec only returns on error
    std::process::exit(1);
}
```

### Phase 3 — Proot 路径重定向

**目标**: 使用 proot（Linux）或 Windows Detours 实现子进程文件系统透明重定向。

```typescript
export class ProotLikeRedirectStrategy {
  async execute(options: StrategyExecuteOptions, policy: SandboxPolicy): Promise<...> {
    // 构建 proot 命令
    const prootCmd = [
      "proot",
      "-b", `${vfsRoot}:/etc`,
      "-b", `${vfsRoot}:/home`,
      "-b", `${vfsRoot}:/tmp`,
      "-w", options.cwd,
      "--", options.command,
    ].join(" ");

    const result = await this.terminalService.executeOneOff(prootCmd, {
      cwd: options.cwd,
      env: options.env,
      timeout: options.timeout,
    });
    return this.toResult(result, startTime);
  }
}
```

## TerminalService 扩展 API

为实现 OS Hook，TerminalService 需要暴露更底层的进程控制方法：

```typescript
export interface ProcessSpawnOptions {
  command: string;
  shell?: boolean;
  cwd?: string;
  env?: Record<string, string>;
  timeout?: number;
  /** Windows Job Object handle (for job association) */
  jobHandle?: Buffer;
  /** seccomp BPF filter (for Linux seccomp) */
  bpfFilter?: Buffer;
}

export class TerminalService {
  /**
   * 底层进程创建 — 扩展点给 OS Hook 使用
   * 相对于 executeOneOff，这个接口返回 ChildProcess 对象
   * 让调用方可以附加 Job Object / seccomp
   */
  async spawnProcess(options: ProcessSpawnOptions): Promise<ChildProcess> {
    const shellPath = this.shellDetector.getShellPath(options.shellType);
    const shellArgs = this.shellDetector.getShellArgs(options.shellType, options.command);

    const proc = spawn(shellPath, shellArgs, {
      cwd: options.cwd,
      env: { ...process.env, ...options.env },
      windowsHide: true,
      // options.jobHandle 可以通过 customFds 或 windowsVerbatimArguments 传递
    });

    return proc;
  }

  /**
   * 监控进程完成 — 供 OS Hook 复用
   */
  async monitorProcess(proc: ChildProcess, timeout?: number): Promise<ExecuteResult> {
    // 现有 executeCommand() 中的超时 + 输出收集逻辑
  }
}
```

## 策略链执行模型

OS Hook 不独立工作，而是作为链的一部分：

```
shellStrategy: ["os-hook", "static-analyzer"]

策略链执行:
  1. resolveBest("shell", ["windows-job", "static-analyzer"])
     → isAvailable("windows-job") === true (win32)
     → 使用 WindowsJobObjectStrategy

  WindowsJobObjectStrategy.execute():
    1. 解析 policy（resource, process 限制）
    2. 调用 terminalService.spawnProcess() 创建进程
    3. 用 koffi 调用 CreateJobObject
    4. AssignProcessToJobObject(process.pid)
    5. SetInformationJobObject（设置限制）
    6. 调用 terminalService.monitorProcess() 等待完成
    7. 进程组结束后，TerminateJobObject

shellStrategy: ["os-hook", "static-analyzer"]
  如果 os-hook 不可用（平台不支持）→ 回退到 static-analyzer
```

### Docker / Container 策略的关系

```typescript
// sandbox-runtime.ts
private applyLegacyMappings(config: SandboxConfig): SandboxConfig {
  if (config.type === "docker") {
    return { ...config, shellStrategy: ["container"] };
    // container 策略的优先级高于 os-hook
  }
}
```

## 测试计划

### Phase 2 测试

```typescript
describe("WindowsJobObjectStrategy", () => {
  it("should create Job Object and assign process", async () => {
    const strategy = new WindowsJobObjectStrategy(terminalService);
    const result = await strategy.execute(
      { command: "echo hello", shellType: "cmd" },
      { mode: "strict", resource: { processLimit: 2 } },
    );
    expect(result.success).toBe(true);
    expect(result.stdout).toContain("hello");
  });

  it("should enforce process limit", async () => {
    // 启动多个子进程，应被 Job Object 限制
    const strategy = new WindowsJobObjectStrategy(terminalService);
    const result = await strategy.execute(
      { command: "start /B cmd /c ping localhost", shellType: "cmd" },
      { mode: "strict", process: { maxChildProcesses: 1 } },
    );
    // 预期：进程超过限制时自动终止
    expect(result.exitCode).not.toBe(0);
  });

  it("should terminate all processes on timeout", async () => {
    const strategy = new WindowsJobObjectStrategy(terminalService);
    const result = await strategy.execute(
      { command: "ping -n 10 localhost", timeout: 1000 },
      { mode: "strict", resource: { timeoutLimit: 1000 } },
    );
    expect(result.success).toBe(false);
    expect(result.error).toContain("timeout");
  });
});
```

### E2E 测试（Phase 2+）

```typescript
describe("OS Hook Strategy Chain", () => {
  it("should prefer os-hook over static-analyzer on Windows", async () => {
    const runtime = getSandboxRuntime();
    const { strategy } = await runtime.createRuntime("shell", {
      command: "echo hello",
    }, {
      mode: "strict",
      shellStrategy: ["os-hook", "static-analyzer"],
    });
    expect(strategy?.id).toBe("windows-job");
  });

  it("should fallback to static-analyzer when os-hook unavailable", async () => {
    // 模拟非 Windows 平台
    Object.defineProperty(process, "platform", { value: "darwin" });
    const runtime = getSandboxRuntime();
    const { strategy } = await runtime.createRuntime("shell", {
      command: "echo hello",
    }, {
      mode: "strict",
      shellStrategy: ["os-hook", "static-analyzer"],
    });
    expect(strategy?.id).toBe("static-analyzer");
  });
});
```

## 风险与注意事项

| 风险 | 影响 | 缓解措施 |
|---|---|---|
| Job Object 与 Node.js 进程嵌套 | Node.js 本身已在某个 Job 中（VS/terminal） | 嵌套 Job 在 Windows 8+ 支持（`JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK`） |
| seccomp-loader 跨架构 | 需要为 x86_64 / aarch64 分别编译 | 使用 Rust 编译静态链接二进制 |
| proot 版本兼容性 | proot-rs vs proot 不同实现 | 先检测可用性再决定 API |
| koffi 原生绑定大小 | koffi 包含 74MB 跨平台原生二进制 | 作为可选依赖，动态 require 避免打包到编译产物 |
| 策略链中 OS Hook 执行时间 | OS Hook 增加开销 | 仅在高安全级别时启用 |
| koffi 未安装时的安全缺口 | 自动降级到 passthrough | 在 `SandboxRuntime` 层面提供警告日志 |
