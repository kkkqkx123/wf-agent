# 沙箱模块 — 分阶段代码修改任务

> 基于 [architecture.md](./architecture.md) 及 `strategies/` 目录下的策略方案文档。
> 状态标识: ✅ 完成 · 🔄 进行中 · ⏳ 待开始 · ❌ 阻塞

---

## 阶段总览

| 阶段 | 名称 | 涉及模块 | 预估文件数 | 前置依赖 |
|------|------|---------|-----------|---------|
| P1 | 类型体系落地 | `packages/types/` | 4 | 无 |
| P2 | 策略实现层 | `sdk/services/sandbox/` | 8 | P1 |
| P3 | Sandbox Runtime & Resolver | `sdk/services/sandbox/` | 3 | P2 |
| P4 | 沙箱执行器 | `sdk/core/script/executors/` | 4 | P3 |
| P5 | VFS Overlay | `sdk/services/vfs/` | 5 | P1 |
| P6 | OS级 Hook | `sdk/services/sandbox/strategies/os-hook/` | 3 | P2 |
| P7 | 集成对接 | `sdk/core/executors/` + `sdk/core/script/engine/` | 3 | P4 |
| P8 | 测试覆盖 | 各模块 `__tests__/` | 10+ | P1~P7 |

---

## Phase 1: 类型体系落地

**目标**: 将 architecture.md §4 中的全部类型定义写入 `packages/types/src/script/`，并更新导出。

### 1.1 新建 `script-sandbox.ts`

**位置**: `packages/types/src/script/script-sandbox.ts`

**内容** (按 architecture.md §4 逐项实现):

| 类型/类型 | 说明 | 参考文档位置 |
|-----------|------|------------|
| `SandboxMode` | `"disabled" \| "lenient" \| "strict" \| "custom"` | §4.1 |
| `SandboxPolicy` + 子 Policy | 完整 Policy 体系 | §4.2 |
| `ShellSandboxStrategy` / `PythonSandboxStrategy` / `JavaScriptSandboxStrategy` | 策略标识符联合类型 | §4.3 |
| `StrategyImplementation<TResult>` | 策略实现接口 | §4.3 |
| `StrategyResolver` | 策略解析器接口 | §4.3 |
| `SandboxConfig` (重构版) | 合并旧 `type`/`image` 兼容字段 | §4.5 |
| `ScriptLanguage` | `"auto" \| "shell" \| "python" \| "javascript"` | §4.4 |
| `ExecutorMode` 扩展 | 追加 `"sandbox-shell" \| "sandbox-python" \| "sandbox-javascript"` | §4.4 |
| `VFSConfig` | VFS 配置类型 | vfs-overlay.md §VFS 配置 |

### 1.2 修改 `script.ts`

**位置**: `packages/types/src/script/script.ts`

- `Script` 接口增加 `language?: ScriptLanguage`
- `ScriptExecutionOptions` 中 `executorMode` 类型从 `"direct" | "shared" | "pty"` 扩展为完整 `ExecutorMode`
- `ScriptExecutionOptions` 中 `sandboxConfig` 类型从旧 `SandboxConfig` 升级为新 `SandboxConfig`（保持向后兼容：旧字段通过 `@deprecated` 标记 + 运行时自动映射）

### 1.3 修改 `script-executor.ts`

**位置**: `packages/types/src/script/script-executor.ts`

- `ExecutorMode` 类型定义扩展：追加 `"sandbox-shell" | "sandbox-python" | "sandbox-javascript"`

### 1.4 修改 `index.ts`

**位置**: `packages/types/src/script/index.ts`

- 追加 `export * from "./script-sandbox.js";`
- 确保新 Schema 导出追加

### 1.5 修改 `script-schema.ts`

**位置**: `packages/types/src/script/script-schema.ts`

- 新增 `SandboxModeSchema`
- 新增 `SandboxPolicySchema` 及子 Policy Schema
- 新增 `ScriptLanguageSchema`
- 更新 `SandboxConfigSchema`：追加新字段，旧字段标记 optional
- 更新 `ScriptExecutionOptionsSchema`：`executorMode` 增加新枚举值
- 新增对应 type guard 函数

---

## Phase 2: 策略实现层

**目标**: 在 `sdk/services/sandbox/` 下实现各语言策略的具体逻辑。

### 2.1 目录初始化

新建目录结构：

```
sdk/services/sandbox/
├── types.ts                  # 运行时类型（引用 packages/types）
├── sandbox-runtime.ts        # (Phase 3)
├── strategy-resolver.ts      # (Phase 3)
├── default-policy.ts         # (Phase 3)
├── index.ts                  # 模块导出
└── strategies/
    ├── shell-static-analyzer.ts
    ├── python-builtin-hook.ts
    ├── python-ast-analyzer.ts
    ├── js-vm-context.ts
    └── os-hook/              # (Phase 6)
```

### 2.2 `types.ts`

- 从 `@wf-agent/types` re-export 沙箱相关类型
- 定义 `SandboxExecutionResult` 扩展类型（如果需要追加沙箱特有的执行结果字段）

### 2.3 Shell 静态分析策略

**文件**: `sdk/services/sandbox/strategies/shell-static-analyzer.ts`

参考 [shell-static-analyzer.md](./strategies/shell-static-analyzer.md) 实现：

- [x] `ShellStaticAnalyzerStrategy` 类，实现 `StrategyImplementation<ScriptExecutionResult>`
- [x] `id = "static-analyzer"`, `priority = 10`
- [x] `isAvailable()` 返回 `true`
- [x] 三层检测：
  - Layer 1: 命令黑白名单（通过 `extractPrimaryCommand()` 提取顶层命令）
  - Layer 2: 高危模式正则匹配（`dangerousPatterns`）
  - Layer 3: 路径访问检查 + pipe/redirect 检查
- [x] 通过返回 `this.executeCommand()` 放行，或 `this.deny()` 拒绝
- [x] `dangerousPatterns` 默认内置规则集（rm -rf /, fork bomb, curl|sh, LD_PRELOAD, python -c 恶意代码, node -e 恶意代码）

### 2.4 Python builtin-hook 策略

**文件**: `sdk/services/sandbox/strategies/python-builtin-hook.ts`

参考 [python-sandbox.md](./strategies/python-sandbox.md) §策略一 实现：

- [x] `PythonBuiltinHookStrategy` 类
- [x] `id = "builtin-hook"`, `priority = 20`
- [x] `isAvailable()` 检查本机 Python 是否可用
- [x] `wrapWithSandbox(code, policy)` 生成受限 Python 脚本：
  - 清空 `sys.path`
  - 替换 `builtins.open` 为安全版本（检查写路径白名单）
  - 替换 `builtins.__import__` 为安全版本（检查模块黑白名单）
  - 禁用 `sys.modules["os"]` 等高危已加载模块
- [x] 写入临时文件 → `subprocess` 受限执行
- [x] 环境变量清理：`PYTHONPATH=""`, `PYTHONDONTWRITEBYTECODE="1"`

### 2.5 Python AST 分析策略

**文件**: `sdk/services/sandbox/strategies/python-ast-analyzer.ts`

参考 [python-sandbox.md](./strategies/python-sandbox.md) §策略二 实现：

- [x] `PythonASTAnalyzerStrategy` 类
- [x] `id = "ast-analyzer"`, `priority = 25`
- [x] `isAvailable()` 检查本机 Python + `ast` 模块可用
- [x] 生成 AST 分析脚本，通过子进程运行 `python ast` 模块：
  - 检测 `import os` / `from os import remove` 等模块导入
  - 检测 `os.remove(...)` 等高危属性调用
  - 检测 `eval()`, `exec()`, `compile()`, `open(mode='w')`
  - 检测 `getattr(__import__('os'), 'remove')` 等动态访问模式（AST 无法检测的需在文档中标注）
- [x] 分析通过后，复用 `PythonBuiltinHookStrategy` 的包装执行逻辑
- [x] 返回 `{ safe, violations }` 结构

### 2.6 JavaScript VM Context 策略

**文件**: `sdk/services/sandbox/strategies/js-vm-context.ts`

参考 [javascript-sandbox.md](./strategies/javascript-sandbox.md) §策略一 实现：

- [x] `JavaScriptVmContextStrategy` 类
- [x] `id = "vm-context"`, `priority = 30`
- [x] `isAvailable()` 返回 `true`（依赖 Node.js 内置 `node:vm`）
- [x] 创建受限 `vm.Context`：
  - 受限 `require()`：模块黑白名单 + 只读 fs 代理 + 禁用 child_process
  - 安全 `console`：捕获 stdout/stderr
  - 受限 `setTimeout`/`setInterval`：最大延迟限制
  - 禁用 `eval`, `Function`, `global`, `globalThis`
  - 安全 `process` 子集：只暴露 `env`, `cwd()`, `argv`
  - 安全 `Buffer` 子集：只暴露 `from()`, `isBuffer()`
- [x] `vm.runInNewContext()` 执行，设置 `timeout`, `breakOnSigint`
- [x] 输出捕获：重写 `console.log/error/warn` 到内部 buffer
- [x] 只读 fs 代理：通过 `Proxy` 拦截 `fs.writeFile/appendFile/mkdir/rmdir/unlink/rename/chmod/copyFile` 等

---

## Phase 3: Sandbox Runtime & Resolver

**目标**: 实现策略解析、优先级回退、默认策略声明。

### 3.1 `default-policy.ts`

**位置**: `sdk/services/sandbox/default-policy.ts`

- [x] 导出 `DEFAULT_SANDBOX_POLICY: SandboxPolicy` 常量
- [x] 各子 Policy 提供合理安全默认值：
  - `shell.deniedCommands`: `["sudo", "su", "chroot", "mount", "dd", "mkfs", "reboot", "shutdown", "passwd"]`
  - `shell.dangerousPatterns`: 内置高危正则集合
  - `python.deniedModules`: `["os", "subprocess", "shutil", "ctypes", "socket", "pty"]`
  - `javascript.deniedModules`: `["child_process", "cluster", "worker_threads"]`
  - `resource.timeoutLimit`: `30000`
- [x] `DEFAULT_SHELL_POLICY`, `DEFAULT_PYTHON_POLICY`, `DEFAULT_JS_POLICY` 子常量

### 3.2 `strategy-resolver.ts`

**位置**: `sdk/services/sandbox/strategy-resolver.ts`

参考 architecture.md §4.3 StrategyResolver 接口：

- [x] `DefaultStrategyResolver` 类，实现 `StrategyResolver`
- [x] 内置策略注册表：
  - Shell: `{ "static-analyzer": ShellStaticAnalyzerStrategy, "os-hook": LinuxSeccompStrategy/WindowsJobObjectStrategy }`
  - Python: `{ "builtin-hook": PythonBuiltinHookStrategy, "ast-analyzer": PythonASTAnalyzerStrategy }`
  - JavaScript: `{ "vm-context": JavaScriptVmContextStrategy }`
- [x] `resolveShellStrategy(ids)`: 按优先级数组遍历，返回第一个 `isAvailable()` 为 `true` 的策略
- [x] `resolvePythonStrategy(ids)`: 同上
- [x] `resolveJavaScriptStrategy(ids)`: 同上
- [x] `registerStrategy(language, impl)`: 允许外部注入自定义策略
- [x] 回退策略（fallback）：当所有策略均不可用时，返回一个 "passthrough" 策略（不做安全检查，直接执行）

### 3.3 `sandbox-runtime.ts`

**位置**: `sdk/services/sandbox/sandbox-runtime.ts`

参考 architecture.md §5 沙箱执行器内部委托逻辑：

- [x] `SandboxRuntime` 类（全局单例模式，或通过依赖注入）
- [x] 核心方法：
  - `createShellRuntime(options, policy, strategyPriority[])` → 返回 `StrategyImplementation`
  - `createPythonRuntime(...)` → 同上
  - `createJavaScriptRuntime(...)` → 同上
- [x] 配置解析逻辑：
  - `mode === undefined` → 等价于 `"disabled"`
  - `mode === "disabled"` → 返回 null，调用方走标准 Executor
  - `mode === "lenient"` → 执行策略但不拒绝（仅日志警告）
  - `mode === "strict"` → 执行策略并拒绝违规
  - `mode === "custom"` → 完全由 policy 控制
- [x] 旧配置兼容映射：
  - `type: "docker"` → `shellStrategy: ["container"]`
  - `type: "nodejs"` → `javascriptStrategy: ["vm-context"]`
  - `type: "python"` → `pythonStrategy: ["ast-analyzer", "builtin-hook"]`
- [x] `isEnabled(config)`: 静态方法判断沙箱是否启用

### 3.4 `index.ts`

**位置**: `sdk/services/sandbox/index.ts`

- [x] 导出 `SandboxRuntime`
- [x] 导出 `DefaultStrategyResolver`
- [x] 导出 `DEFAULT_SANDBOX_POLICY` 及子常量
- [x] 导出 `getSandboxRuntime()` 单例获取函数

---

## Phase 4: 沙箱执行器

**目标**: 新建三个沙箱执行器，按 architecture.md §5 的类结构。

### 4.1 `sandbox-shell-executor.ts`

**位置**: `sdk/core/script/executors/sandbox-shell-executor.ts`

参考 architecture.md §5 `SandboxShellExecutor`：

- [x] `SandboxShellExecutor extends BaseExecutor`
- [x] 构造函数注入 `SandboxRuntime` 或自动获取单例
- [x] `execute(options)` 流程：
  1. 从 `options` 或全局配置获取 `SandboxConfig`
  2. 调用 `SandboxRuntime.createShellRuntime(config)` 获取策略
  3. 若返回 null（沙箱禁用）或 mode=disabled → 委托 `DirectExecutor` 执行
  4. 若 mode=lenient → 执行策略，违规仅日志不拒绝
  5. 若 mode=strict → 执行策略，违规拒绝返回错误
  6. 结果封装为 `ScriptExecutionResult`

### 4.2 `sandbox-python-executor.ts`

**位置**: `sdk/core/script/executors/sandbox-python-executor.ts`

- [x] `SandboxPythonExecutor extends BaseExecutor`
- [x] 与 `sandbox-shell-executor` 结构类似，调用 `createPythonRuntime`

### 4.3 `sandbox-javascript-executor.ts`

**位置**: `sdk/core/script/executors/sandbox-javascript-executor.ts`

- [x] `SandboxJavaScriptExecutor extends BaseExecutor`
- [x] 与上类似，调用 `createJavaScriptRuntime`

### 4.4 `base-executor.ts` 扩展

**位置**: `sdk/core/script/executors/base-executor.ts`

- [x] `BaseExecuteOptions` 增加 `sandboxConfig?: SandboxConfig`
- [x] `BaseExecuteOptions` 增加 `language?: ScriptLanguage`
- [x] `execute()` 方法签名不变，保持向后兼容

### 4.5 Executor `index.ts` 扩展

**位置**: `sdk/core/script/executors/index.ts`

- [x] 追加导出三个沙箱执行器

---

## Phase 5: VFS Overlay

**目标**: 实现虚拟文件系统，支持 Copy-on-Write 和文件隔离。

参考 [vfs-overlay.md](./strategies/vfs-overlay.md) 实现。

### 5.1 初始化目录

```
sdk/services/vfs/
├── types.ts
├── overlay-vfs.ts
├── whiteout-cache.ts
├── delta-fs.ts           # MemoryDelta 实现
├── base-fs.ts
└── index.ts
```

### 5.2 `types.ts`

- `VFSEntry` 接口
- `VFSOperations` 接口
- `VFSConfig` 接口（与 `packages/types` 同步）
- `DeltaFileSystem` 接口（Delta 层契约）

### 5.3 `whiteout-cache.ts`

- `WhiteoutCache` 类，基于 Trie 结构
- `hasWhiteout(path)` / `markWhiteout(path)` 方法
- 祖先 whiteout 传播

### 5.4 `base-fs.ts`

- `HostFS` 类，实现只读 Base 层
- 路径策略白名单检查
- 代理到真实 `fs` 模块的读操作

### 5.5 `delta-fs.ts`

- `MemoryDelta` 类
- `stat`, `writeFile`, `readFile`, `remove`, `readdir`, `mkdir`, `rmdir`
- `snapshot()` / `restore()` 快照支持
- `SQLiteDelta` 骨架（标注为 future work）

### 5.6 `overlay-vfs.ts`

- `OverlayVFS` 类，整合三层：
  1. Delta → 2. Whiteout → 3. Base
- 实现完整 `VFSOperations` 接口
- `CheckpointAwareVFS` wrapper 类：
  - `onCheckpointCreate(checkpointId)` → 创建快照并存储映射
  - `onCheckpointRestore(checkpointId)` → 从映射恢复快照

### 5.7 `index.ts`

- 导出所有 VFS 组件

---

## Phase 6: OS 级 Hook

**目标**: 实现平台相关的 OS 级隔离策略。参考 [os-level-hook.md](./strategies/os-level-hook.md)。

### 6.1 目录结构

```
sdk/services/sandbox/strategies/os-hook/
├── linux-seccomp.ts
├── windows-job-object.ts
└── index.ts
```

### 6.2 `linux-seccomp.ts`

- `LinuxSeccompStrategy` 类
- `id = "os-hook"`, `priority = 50`
- `isAvailable()`: `process.platform === "linux"`
- 三种实现方式骨架（标注为 future work）：
  - 方式 A: 辅助 seccomp-loader 二进制进程
  - 方式 B: LD_PRELOAD 注入
  - 方式 C: nsjail/firejail 包装
- 生成 BPF 程序逻辑描述（类型定义 + 白名单/黑名单配置）
- 当前阶段实现：返回 `isAvailable() === false`，策略解析时自动回退

### 6.3 `windows-job-object.ts`

- `WindowsJobObjectStrategy` 类
- `id = "os-hook"`, `priority = 50`（与 linux 同优先级，平台互斥）
- `isAvailable()`: `process.platform === "win32"`
- 通过 `koffi` 或 `edge-js` 调用 Win32 API 骨架：
  - `CreateJobObject`
  - `SetInformationJobObject`（CPU/内存/进程数限制）
  - `AssignProcessToJobObject`
- 当前阶段实现：返回 `isAvailable() === false`，策略解析时自动回退

### 6.4 `index.ts`

- 导出两个平台策略
- `getPlatformOSHookStrategy()` 工厂函数：根据当前平台返回对应策略实例

> **注意**: Phase 6 在早期阶段仅提供接口检测 + 骨架实现，`isAvailable()` 返回 `false` 确保不影响策略回退链。完整 OS hook 功能列为未来迭代。

---

## Phase 7: 集成对接

**目标**: 将沙箱执行器接入 `ScriptExecutor` 和 `ScriptEngine`，实现完整的执行路由。

### 7.1 `script-executor.ts` 改造

**位置**: `sdk/core/executors/script-executor.ts`

- [x] `ExecutorMode` 导入更新（从 types 获取扩展后的联合类型）
- [x] `executors` Map 中注册沙箱执行器：
  - `"sandbox-shell"` → `SandboxShellExecutor`
  - `"sandbox-python"` → `SandboxPythonExecutor`
  - `"sandbox-javascript"` → `SandboxJavaScriptExecutor`
- [x] `execute()` 方法中 `executorMode` 解析逻辑：优先使用 `script.executor?.mode`，其次 `options?.executorMode`，默认 `"direct"`
- [x] 将 `options.sandboxConfig` 传递到沙箱执行器的 `BaseExecuteOptions` 中

### 7.2 `script-engine.ts` 改造

**位置**: `sdk/core/script/engine/script-engine.ts`

- [x] 与 7.1 类似的改造
- [x] `executors` Map 注册所有标准执行器 + 沙箱执行器
- [x] `execute()` 中增加 `language` 传递逻辑：从 `script.language` 或 `options?.sandboxConfig` 推断
- [x] 确保 `pty` 模式与沙箱模式互斥（sandbox-pty 组合返回错误）

### 7.3 `BaseExecuteOptions` 传递链路

**检查点**：

- [x] `DirectExecutor.execute()`: 确认忽略 `sandboxConfig`
- [x] `SharedExecutor.execute()`: 确认忽略 `sandboxConfig`
- [x] `PtyExecutor.execute()`: 确认忽略 `sandboxConfig`
- [x] 沙箱执行器: 正确消费 `sandboxConfig`

---

## Phase 8: 测试覆盖

**目标**: 在每个模块的 `__tests__/` 下编写测试，确保功能正确性。

### 8.1 类型测试

**位置**: `packages/types/__tests__/test-d/script/sandbox-types.test-d.ts`

- [x] 验证 `SandboxMode` 联合类型
- [x] 验证 `SandboxConfig` 新旧兼容字段
- [x] 验证 `ExecutorMode` 扩展后的完整联合类型
- [x] 验证 `ScriptLanguage` 联合类型

### 8.2 Schema 测试

**位置**: `packages/types/__tests__/script/sandbox-schema.test.ts`

- [x] `SandboxModeSchema` 解析验证
- [x] `SandboxConfigSchema` 新旧格式兼容解析
- [x] 默认值测试

### 8.3 Shell 静态分析策略测试

**位置**: `sdk/services/sandbox/__tests__/strategies/shell-static-analyzer.test.ts`

- [x] 黑名单命令被拒绝（`sudo`, `rm -rf /`）
- [x] 白名单命令通过
- [x] 高危模式正则匹配（fork bomb, curl|sh, LD_PRELOAD）
- [x] 正常命令通过
- [x] pipe/redirect 开关测试

### 8.4 Python 策略测试

**位置**: `sdk/services/sandbox/__tests__/strategies/python-builtin-hook.test.ts`
**位置**: `sdk/services/sandbox/__tests__/strategies/python-ast-analyzer.test.ts`

- [x] builtin-hook: 禁用模块被拒绝（`import os`）
- [x] builtin-hook: 写入受限路径被拒绝
- [x] builtin-hook: 安全脚本正常执行
- [x] ast-analyzer: AST 检测高危调用
- [x] ast-analyzer: AST 通过后正常执行
- [x] `isAvailable()`: Python 不可用时的回退行为

### 8.5 JavaScript VM Context 策略测试

**位置**: `sdk/services/sandbox/__tests__/strategies/js-vm-context.test.ts`

- [x] 禁用模块被拒绝（`require('child_process')`）
- [x] 只读 fs 写入被拒绝
- [x] `eval`/`Function` 被禁用
- [x] 简单 JS 表达式正常执行
- [x] 超时处理

### 8.6 Sandbox Runtime 测试

**位置**: `sdk/services/sandbox/__tests__/sandbox-runtime.test.ts`

- [x] `mode: undefined` → 等价 disabled
- [x] `mode: disabled` → 返回 null
- [x] 策略优先级解析正确
- [x] 策略回退（首选不可用 → 次选）
- [x] 自定义策略注册 + 解析
- [x] 旧配置兼容映射（`type: "docker"` → shellStrategy）

### 8.7 Sandbox Executor 测试

**位置**: `sdk/core/script/executors/__tests__/sandbox-shell-executor.test.ts`
**位置**: `sdk/core/script/executors/__tests__/sandbox-python-executor.test.ts`
**位置**: `sdk/core/script/executors/__tests__/sandbox-javascript-executor.test.ts`

- [x] 沙箱禁用时委托 DirectExecutor
- [x] 沙箱严格模式拒绝违规
- [x] 沙箱宽松模式仅警告
- [x] 结果格式正确

### 8.8 VFS 测试

**位置**: `sdk/services/vfs/__tests__/overlay-vfs.test.ts`
**位置**: `sdk/services/vfs/__tests__/whiteout-cache.test.ts`

- [x] WhiteoutCache: 基本标记/查询
- [x] WhiteoutCache: 祖先 whiteout 传播
- [x] OverlayVFS: 三层查找语义
- [x] OverlayVFS: 写入 Delta 层
- [x] OverlayVFS: snapshot/restore

### 8.9 集成测试

**位置**: `sdk/__tests__/sandbox/sandbox-integration.int.test.ts`

- [x] ScriptExecutor 完整链路：sandbox-shell → sandbox runtime → 策略执行
- [x] ScriptEngine 完整链路：template 渲染 → 沙箱执行 → 结果返回
- [x] executorMode: "sandbox-shell" 路由正确
- [x] 向后兼容：无 sandboxConfig 的旧脚本正常执行

### 8.10 安全测试（负面测试重点）

- [x] Shell: 绕过尝试（`rm -rf /etc` 变体）
- [x] Python: `builtins` 恢复绕过（`__builtins__.__dict__['__import__']('os')`）
- [x] Python: 动态构造绕过（`getattr(__import__('o'+'s'),'remove')`）
- [x] JS: 原型链污染绕过
- [x] JS: `process.binding()` 绕过（如果不可行，文档注明限制）

---

## 执行顺序建议

```
Phase 1 (类型体系)
  ↓
Phase 2 + Phase 5 (可并行: 策略实现 + VFS)
  ↓
Phase 3 (Runtime & Resolver)
  ↓
Phase 4 (沙箱执行器)
  ↓
Phase 6 (OS Hook, 可选并行)
  ↓
Phase 7 (集成对接)
  ↓
Phase 8 (测试)
```

### 快速启动建议

最小可行子集（MVP）：

```
P1 → P2.3 (shell-static-analyzer) → P3 → P4.1 (sandbox-shell-executor)
  → P7.1 (ScriptExecutor 集成) → P8.3 + P8.9 (集成测试)
```

MVP 完成后即可获得可运行的 shell 沙箱能力，后续逐步追加 Python/JS/VFS。