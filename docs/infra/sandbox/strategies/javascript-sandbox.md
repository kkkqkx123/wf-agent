# JavaScript 沙箱策略

## 概述

| 策略 | 标识 | 级别 | 原理 |
|------|------|------|------|
| VM Context | `vm-context` | 轻量 | Node.js `vm.createContext()` |
| Isolated VM | `isolated-vm` | 中级 | 独立 V8 隔离实例 |
| OS Hook | `os-hook` | OS级 | 通过 OS 限制 Node 进程 |
| Container | `container` | 容器 | Docker/Podman |

## 策略一: vm-context

### 原理

使用 Node.js 内置 `node:vm` 模块创建受限上下文。

```typescript
import vm from "node:vm";

export class JavaScriptVmContextStrategy implements StrategyImplementation<...> {
  id = "vm-context";
  priority = 30;

  async execute(options: BaseExecuteOptions, policy: SandboxPolicy): Promise<...> {
    const code = options.command;
    const jsPolicy = policy.javascript || DEFAULT_JS_POLICY;

    // 1. 创建受限上下文
    const sandbox = vm.createContext({
      // 受限的 require
      require: (moduleName: string) => {
        return this.restrictedRequire(moduleName, jsPolicy);
      },
      // 安全控制台
      console: {
        log: (...args: unknown[]) => this.captureOutput("log", args),
        error: (...args: unknown[]) => this.captureOutput("error", args),
        warn: (...args: unknown[]) => this.captureOutput("warn", args),
      },
      // 受限的全局对象
      setTimeout: (fn: () => void, ms: number) => this.safeSetTimeout(fn, ms),
      setInterval: (fn: () => void, ms: number) => this.safeSetInterval(fn, ms),
      // 禁用
      eval: undefined,
      Function: undefined,
      // 安全的过程
      process: {
        env: { NODE_ENV: "sandbox" },
        cwd: () => "/workspace",
        argv: ["sandbox"],
      },
      // Buffer 子集
      Buffer: {
        from: (data: string) => Buffer.from(data),
        isBuffer: (obj: unknown) => Buffer.isBuffer(obj),
      },
      // 空全局
      global: {},
      globalThis: {},
    });

    // 2. 执行代码
    try {
      vm.runInNewContext(code, sandbox, {
        timeout: policy.resource?.timeoutLimit || 30_000,
        filename: "sandbox.js",
        breakOnSigint: true,
      });
    } catch (error) {
      if (error.code === "ERR_SCRIPT_EXECUTION_TIMEOUT") {
        return { success: false, error: "Script execution timeout" };
      }
      throw error;
    }

    return {
      success: true,
      stdout: this.getCapturedOutput(),
    };
  }

  /**
   * 受限 require: 模块白名单 + 拒绝高危模块
   */
  private restrictedRequire(moduleName: string, policy: JavaScriptPolicy): unknown {
    // 检查白名单
    if (policy.allowedModules.length > 0) {
      if (!policy.allowedModules.includes(moduleName)) {
        throw new Error(`Module not allowed: ${moduleName}`);
      }
    }

    // 检查黑名单
    if (policy.deniedModules.includes(moduleName)) {
      throw new Error(`Module denied: ${moduleName}`);
    }

    // 对 fs 模块做安全包装
    if (moduleName === "fs" && policy.allowFSWrite === false) {
      return this.createReadonlyFS();
    }

    // 禁用原生模块
    if (moduleName === "child_process" && !policy.allowChildProcess) {
      throw new Error("child_process module is not allowed");
    }

    return require(moduleName);
  }

  /**
   * 只读 fs 代理 (权限检查 + 路径重定向)
   */
  private createReadonlyFS(): typeof import("fs") {
    const fs = require("fs");
    return new Proxy(fs, {
      get(target, prop) {
        // 禁止写入操作
        if (["writeFile", "writeFileSync", "appendFile", "appendFileSync",
             "mkdir", "mkdirSync", "rmdir", "rmdirSync",
             "unlink", "unlinkSync", "rename", "renameSync",
             "chmod", "chmodSync", "copyFile", "copyFileSync"].includes(prop)) {
          return () => { throw new Error(`Read-only filesystem: ${prop} not allowed`); };
        }
        return target[prop];
      },
    });
  }
}
```

### 限制

- `vm` 模块不是安全沙箱 (存在原型链污染绕过风险)
- 不能限制 `process.binding()` 等底层 API (Node.js 内部)
- 不适用于不受信任的完全恶意代码

## 策略二: isolated-vm (可选增强)

### 原理

使用 `isolated-vm` 创建独立的 V8 隔离实例, 有更强的内存和 CPU 隔离：

```typescript
import ivm from "isolated-vm";

export class IsolatedVmStrategy implements StrategyImplementation<...> {
  async execute(options: BaseExecuteOptions, policy: SandboxPolicy): Promise<...> {
    const isolate = new ivm.Isolate({ memoryLimit: policy.resource?.memoryLimit || 128 });
    const context = await isolate.createContext();

    // 注入受限 API
    context.eval(`
      const console = { log: (...args) => {} };
      const require = (name) => {
        if (name === 'fs') throw new Error('fs denied');
        // ...
      };
    `);

    // 执行
    const result = await context.eval(code, { timeout: policy.resource?.timeoutLimit || 10_000 });
    isolate.dispose();

    return { success: true, stdout: String(result) };
  }
}
```

### 优点

- 真正的 V8 隔离, 不能访问宿主进程
- 内存/CPU 资源限制
- 可限制异步操作

### 限制

- 需要安装 `isolated-vm` 原生模块 (部分环境可能不兼容)
- 性能开销较大 (每次创建新 isolate)