# 终端服务沙箱 — 架构设计

## 一、问题背景

当前终端服务将所有脚本 (Shell/Python/JavaScript) 通过同一路径执行，没有任何安全隔离机制：

```
Script → TerminalService → child_process.spawn → 系统调用 (无拦截)
```

AI 生成的脚本可执行任意操作：`rm -rf /`、`import os; os.remove('/etc')`、`require('fs').unlinkSync('/etc/passwd')`。

## 二、设计原则

| 原则 | 说明 |
|------|------|
| **沙箱可选** | 默认关闭，零额外开销。用户按需开启 |
| **多方案可选** | 每种语言提供轻量/中级/OS级/容器 等多种策略 |
| **非重量级唯一** | Pyodide/WASM 等重方案不能作为唯选项 |
| **完全可自定义** | 用户可通过 StrategyProvider 注入自定义实现 |
| **OS级 Hook 可选** | seccomp/JobObject/Proot 作为可选增强 |
| **向后兼容** | 不破坏现有 `direct`/`shared`/`pty` 执行模式 |

## 三、核心架构：四层模型

```
┌──────────────────────────────────────────────────────────────────────┐
│  Layer 1: Policy (策略声明层)                                        │
│  声明"什么允许/拒绝", 不关心"怎么做"                                 │
│                                                                      │
│  SandboxPolicy → ShellPolicy / PythonPolicy / JavaScriptPolicy       │
│  FilesystemPolicy / NetworkPolicy / ProcessPolicy / ResourcePolicy   │
├──────────────────────────────────────────────────────────────────────┤
│  Layer 2: Strategy (策略实现层)                                      │
│  为每种语言提供多种可选实现方案, 按优先级数组自动回退                │
│                                                                      │
│  shell:    static-analyzer | os-hook | container                     │
│  python:   builtin-hook | ast-analyzer | os-hook | pyodide | container│
│  js:       vm-context | isolated-vm | os-hook | container            │
├──────────────────────────────────────────────────────────────────────┤
│  Layer 3: Executor (执行器路由层)                                    │
│  根据配置将脚本路由到标准执行器或沙箱执行器                          │
│                                                                      │
│  ExecutorMode: direct | shared | pty                                 │
│               | sandbox-shell | sandbox-python | sandbox-javascript  │
├──────────────────────────────────────────────────────────────────────┤
│  Layer 4: Runtime (运行时适配层)                                     │
│  具体的进程控制 / 语言运行时 / 系统调用接口                          │
│                                                                      │
│  child_process | node-pty | node:vm | seccomp | Job Object | Docker  │
└──────────────────────────────────────────────────────────────────────┘
```

## 四、类型设计

### 4.1 SandboxMode

```typescript
/**
 * 沙箱开关
 * - disabled: 完全关闭 (默认)
 * - lenient: 仅日志警告, 不拒绝
 * - strict: 严格模式, 拒绝违规操作
 * - custom: 自定义, 完全由 policy 控制
 */
export type SandboxMode = "disabled" | "lenient" | "strict" | "custom";
```

### 4.2 Policy 体系

```typescript
export interface SandboxPolicy {
  mode: SandboxMode;
  filesystem?: Partial<FilesystemPolicy>;
  process?: Partial<ProcessPolicy>;
  network?: Partial<NetworkPolicy>;
  resource?: Partial<ResourcePolicy>;
  shell?: Partial<ShellPolicy>;
  python?: Partial<PythonPolicy>;
  javascript?: Partial<JavaScriptPolicy>;
}

export interface FilesystemPolicy {
  allowedReadPaths: string[];     // glob: ["/workspace/**"]
  allowedWritePaths: string[];    // glob: ["/workspace/**"]
  allowedRemovePaths: string[];   // glob: ["/workspace/**"]
  allowedExecutePaths: string[];  // glob: ["/workspace/**", "/usr/bin/**"]
  copyOnWrite: boolean;           // 写时复制
  maxFileSize: number;            // bytes
}

export interface ProcessPolicy {
  allowedChildProcesses: string[];   // 白名单
  deniedChildProcesses: string[];    // 黑名单
  maxChildProcesses: number;         // 最大子进程数
  allowFork: boolean;
  allowExec: boolean;
}

export interface NetworkPolicy {
  access: "none" | "localhost" | "specific" | "all";
  allowedDomains?: string[];
  allowedPorts?: [number, number][];
  allowDns: boolean;
}

export interface ResourcePolicy {
  cpuLimit?: number;      // ms
  memoryLimit?: number;   // MB
  diskLimit?: number;     // MB
  timeoutLimit: number;   // ms
}

export interface ShellPolicy {
  allowedCommands: string[];
  deniedCommands: string[];
  dangerousPatterns: string[];   // 高危模式正则
  allowPipe: boolean;
  allowRedirect: boolean;
}

export interface PythonPolicy {
  allowedModules: string[];
  deniedModules: string[];
  allowSubprocess: boolean;
  restrictBuiltinOpen: boolean;
  allowDynamicEval: boolean;
}

export interface JavaScriptPolicy {
  allowedModules: string[];
  deniedModules: string[];
  allowChildProcess: boolean;
  allowFSWrite: boolean;
  allowDynamicEval: boolean;
}
```

### 4.3 Strategy 体系

```typescript
/** Shell 沙箱策略标识符 */
export type ShellSandboxStrategy =
  | "static-analyzer"   // 轻量: 字符串模式匹配
  | "os-hook"           // OS级: seccomp/JobObject
  | "container"         // 容器: Docker/Podman
  | "custom";           // 自定义

/** Python 沙箱策略标识符 */
export type PythonSandboxStrategy =
  | "builtin-hook"      // 轻量: builtins 替换
  | "ast-analyzer"      // 轻量: AST 预分析
  | "os-hook"           // OS级: seccomp/Proot
  | "pyodide-wasm"      // 重量: WASM 运行时
  | "container"         // 容器
  | "custom";

/** JavaScript 沙箱策略标识符 */
export type JavaScriptSandboxStrategy =
  | "vm-context"        // 轻量: vm.createContext
  | "isolated-vm"       // 中级: isolated-vm
  | "os-hook"           // OS级
  | "container"         // 容器
  | "custom";

/** 策略实现 —— 可注册自定义实现 */
export interface StrategyImplementation<TResult> {
  id: string;
  name: string;
  description: string;
  priority: number;     // 数字越大优先级越高
  execute(options: BaseExecuteOptions, policy: SandboxPolicy): Promise<TResult>;
  isAvailable(): boolean; // 检测当前环境是否支持
}

/** 策略解析器 —— 支持自定义注入 */
export interface StrategyResolver {
  resolveShellStrategy(id: ShellSandboxStrategy | string): StrategyImplementation<ScriptExecutionResult>;
  resolvePythonStrategy(id: PythonSandboxStrategy | string): StrategyImplementation<ScriptExecutionResult>;
  resolveJavaScriptStrategy(id: JavaScriptSandboxStrategy | string): StrategyImplementation<ScriptExecutionResult>;
  registerStrategy(language: "shell" | "python" | "javascript", impl: StrategyImplementation<ScriptExecutionResult>): void;
}
```

### 4.4 ExecutorMode (扩展)

```typescript
/** 执行器模式 */
export type ExecutorMode =
  // 标准执行器 (无沙箱, 现有行为)
  | "direct"
  | "shared"
  | "pty"
  // 沙箱执行器 (新)
  | "sandbox-shell"
  | "sandbox-python"
  | "sandbox-javascript";

/** 脚本语言类型 */
export type ScriptLanguage =
  | "auto"       // 自动检测
  | "shell"
  | "python"
  | "javascript";
```

### 4.5 SandboxConfig (重构)

```typescript
export interface SandboxConfig {
  // 开关
  mode?: SandboxMode;

  // 策略声明 (Policy Layer)
  policy?: Partial<SandboxPolicy>;

  // 策略实现优先级 (Strategy Layer)
  shellStrategy?: (ShellSandboxStrategy | string)[];
  pythonStrategy?: (PythonSandboxStrategy | string)[];
  javascriptStrategy?: (JavaScriptSandboxStrategy | string)[];

  // 自定义策略解析器 (完全覆写)
  customProvider?: StrategyResolver;

  // VFS 配置
  vfs?: {
    enabled?: boolean;
    storage?: "memory" | "sqlite";
    dbPath?: string;
  };

  // === 遗留兼容字段 ===
  /** @deprecated 用 mode + policy 替代 */
  type?: "docker" | "nodejs" | "python" | "custom";
  image?: string;
  resourceLimits?: { memory?: number; cpu?: number; disk?: number };
  network?: { enabled: boolean; allowedDomains?: string[] };
  filesystem?: { allowedPaths?: string[]; readOnly?: boolean };
}
```

### 4.6 Script / ScriptExecutionOptions 扩展

```typescript
export interface Script {
  // ... 现有字段
  language?: ScriptLanguage;    // 新增
}

export interface ScriptExecutionOptions {
  // ... 现有字段
  executorMode?: ExecutorMode;  // 扩展
}
```

## 五、执行器类结构

```
BaseExecutor (abstract)
├── [标准执行器]
│   ├── DirectExecutor    → "direct"
│   ├── SharedExecutor    → "shared"
│   └── PtyExecutor       → "pty"
│
├── [沙箱执行器]
│   ├── SandboxShellExecutor       → "sandbox-shell"
│   ├── SandboxPythonExecutor      → "sandbox-python"
│   └── SandboxJavaScriptExecutor  → "sandbox-javascript"
│
└── [自定义] StrategyResolver.registerStrategy()
```

沙箱执行器内部委托 `SandboxRuntime` 解析策略:

```
SandboxShellExecutor.execute()
  → SandboxRuntime.createShellRuntime(options)
    → 按 config.shellStrategy 优先级解析最佳策略
    → 回退规则: os-hook → static-analyzer → passthrough
  → strategy.execute(command, policy)
```

## 六、数据流

```
用户配置 (preset 或 script.options.sandboxConfig)
  │
  ▼
SandboxRuntime (全局单例, 解析配置)
  │
  ├── SandboxRuntime.enabled === false
  │     → 走标准 Executor (direct/shared/pty)
  │
  └── SandboxRuntime.enabled === true
        → 创建对应语言的 SandboxRuntime
        → 按 strategy[] 优先级选择实现
        → 执行沙箱策略
        → 返回 SandboxExecutionResult
```

## 七、向后兼容

- `mode: undefined` → 等价于 `"disabled"`，现有代码零改动
- 旧 `type: "docker"` → 自动映射为 `shellStrategy: ["container"]`
- 现有 `direct`/`shared`/`pty` 模式完全不变
- 新 `sandbox-*` 模式仅在显式配置时激活

## 八、文件组织

```
packages/types/src/script/
├── script.ts                 # 修改: Script.language, ScriptExecutionOptions.executorMode
├── script-executor.ts        # 修改: ExecutorMode 扩展, ScriptLanguage 新增
├── script-sandbox.ts         # 新建: 全部沙箱类型 (Policy/Strategy/Runtime/Config)
├── script-schema.ts          # 修改: Schema 适配新 SandboxConfig
└── index.ts                  # 修改: 导出新类型

sdk/services/sandbox/
├── types.ts                  # 运行时类型 (同 script-sandbox.ts 的引用)
├── sandbox-runtime.ts        # SandboxRuntime: 配置解析 + 策略路由
├── strategy-resolver.ts      # DefaultStrategyResolver
├── default-policy.ts         # DEFAULT_SANDBOX_POLICY 常量
├── index.ts                  # 模块导出
└── strategies/
    ├── shell-static-analyzer.ts
    ├── python-builtin-hook.ts
    ├── python-ast-analyzer.ts
    ├── js-vm-context.ts
    └── os-hook/              # 平台相关
        ├── linux-seccomp.ts
        ├── windows-job-object.ts
        └── proot-like-redirect.ts

sdk/core/script/executors/
├── base-executor.ts           # 修改: BaseExecuteOptions 扩展
├── sandbox-shell-executor.ts  # 新建
├── sandbox-python-executor.ts # 新建
├── sandbox-javascript-executor.ts # 新建
├── index.ts                   # 修改: 导出新 executor
└── direct/shared/pty-executor.ts # 不变

sdk/services/vfs/
├── types.ts
├── overlay-vfs.ts
├── whiteout-cache.ts
├── delta-fs.ts
├── base-fs.ts
└── index.ts
```