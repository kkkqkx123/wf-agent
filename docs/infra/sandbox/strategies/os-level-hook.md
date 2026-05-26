# OS 级 Hook 策略

## 概述

- **策略标识**: `"os-hook"`
- **安全级别**: OS级 (平台相关)
- **适用平台**: Linux / Windows / macOS
- **定位**: 作为轻量策略的增强层, 叠加使用

## 设计原则

OS 级 Hook 不替代其他策略, 而是作为**更强的隔离层**叠加：

```
策略优先级:
  shellStrategy: ["os-hook", "static-analyzer"]
  
执行流程:
  1. sandbox-shell-executor 创建时, 优先尝试 os-hook
  2. os-hook 检查当前 OS 支持 → Linux seccomp/Win Job Object/macOS sandbox-exec
  3. 若支持 → 使用 OS 级隔离
  4. 若不支持 → 回退到 static-analyzer
```

## 一、Linux seccomp-bpf

### 原理

通过 seccomp-bpf 限制子进程的系统调用, 只允许白名单内的 syscall：

```
允许: read, write, open, close, stat, mmap, brk, exit_group
拒绝: execve, mount, umount, socket, connect, kill, ptrace
```

### 实现

```typescript
export class LinuxSeccompStrategy implements StrategyImplementation<...> {
  id = "os-hook";
  priority = 50;

  isAvailable(): boolean {
    // 仅 Linux + 内核支持 seccomp
    return process.platform === "linux";
  }

  async execute(options: BaseExecuteOptions, policy: SandboxPolicy): Promise<...> {
    const child = spawn(options.command, {
      shell: true,
      cwd: options.cwd,
      env: this.buildSafeEnv(options.env, policy),
      // Linux: 通过 prctl 在 fork 后 exec 前注入 seccomp
      // Node.js 不支持原生 seccomp, 需要通过:
      // 1. 启动一个辅助进程, 用 C/rust 编译的 seccomp-loader
      // 2. 或通过 LD_PRELOAD 注入
      // 3. 或使用 nsjail/firejail 包装
    });

    return this.monitor(child, policy);
  }

  /**
   * 生成 seccomp BPF 程序 (基于 policy)
   * 白名单: 当前会话需要的 syscall
   * 黑名单: 高危 syscall (execve, mount, ptrace, socket...)
   */
  private generateBPF(policy: SandboxPolicy): Buffer {
    // 用 C/rust 编写 seccomp-loader, 通过子进程调用
    // seccomp-loader --allow read,write,open,close,stat,mmap,brk,exit_group
    //              --deny execve,mount,umount,socket,connect,kill,ptrace
    //              -- ./user-script.sh
  }
}
```

### 限制

- Linux only
- Node.js 无原生 seccomp API, 需要辅助二进制
- 粗粒度: 不能区分 `open("/workspace/file")` 和 `open("/etc/passwd")`

## 二、Windows Job Object

### 原理

通过 Windows Job Object API 限制子进程组：

| 能力 | 说明 |
|------|------|
| 进程组管理 | 所有子进程属于同一 Job, 可一起终止 |
| CPU 限制 | 设置 CPU 速率, 防止 fork bomb |
| 内存限制 | 提交内存上限 |
| 进程数限制 | 最大子进程数 |
| UI 限制 | 禁止弹出窗口 |

### 实现

```typescript
export class WindowsJobObjectStrategy implements StrategyImplementation<...> {
  isAvailable(): boolean {
    return process.platform === "win32";
  }

  async execute(options: BaseExecuteOptions, policy: SandboxPolicy): Promise<...> {
    // 通过 node-ffi / koffi / edge-js 调用 Win32 API
    // 1. CreateJobObject → 创建 Job
    // 2. SetInformationJobObject → 设置限制
    //    - JobObjectBasicLimitInformation
    //    - JobObjectExtendedLimitInformation
    // 3. CreateProcess → 在 Job 内启动进程
    // 4. AssignProcessToJobObject → 分配到 Job
    // 5. 子进程超出限制时 → Job 自动终止
  }
}
```

### 限制

- Windows only
- 需要 native binding 或 IPC 调用辅助进程

## 三、Proot 式路径重定向

### 原理

用户态文件系统重定向, 无需 root 权限：

```
原始路径: /etc/passwd
重定向:   /workspace/.vfs/etc/passwd

技术方案:
  Linux:   ptrace + syscall 拦截 + 路径参数改写
  macOS:   DYLD_INSERT_LIBRARIES + fishhook
  Windows: Detours / DLL 注入
```

### 适用场景

- 与 OverlayVFS 结合, 实现对所有子进程透明的 CoW
- 不需要修改 `builtins.open` 或 `fs.readFileSync`
- 所有子进程 (gcc, git, npm) 自动使用重定向后的文件系统

### 实现

```typescript
export class ProotLikeRedirectStrategy implements StrategyImplementation<...> {
  priority = 30;

  isAvailable(): boolean {
    // Linux ptrace 或 Windows Detours
    return process.platform === "linux";
  }

  async execute(options: BaseExecuteOptions, policy: SandboxPolicy): Promise<...> {
    // 启动 proot 辅助进程
    // proot -b /workspace/.vfs:/etc -b /workspace/.vfs:/home ...
    //      -w /workspace ./user-script.sh
  }
}
```