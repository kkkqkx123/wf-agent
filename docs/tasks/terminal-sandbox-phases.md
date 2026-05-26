# 终端沙箱 — 分阶段代码修改任务

## 执行顺序

```
Phase 1 ─→ Phase 2 ─→ Phase 3 ─→ Phase 4
 (类型)     (框架)     (策略)     (集成)
                              ↘
                           Phase 5 (可选 VFS)
```

## Phase 1: 类型定义层

**目标**: 定义完整的类型体系, 不修改运行时代码

| # | 文件 | 操作 | 内容 |
|---|------|------|------|
| P1.1 | `packages/types/src/script/script-sandbox.ts` | **新建** | `SandboxMode`, `SandboxPolicy`, `ShellPolicy`, `PythonPolicy`, `JavaScriptPolicy`, `FilesystemPolicy`, `ProcessPolicy`, `NetworkPolicy`, `ResourcePolicy`, `ShellSandboxStrategy`, `PythonSandboxStrategy`, `JavaScriptSandboxStrategy`, `StrategyImplementation`, `StrategyResolver`, `DEFAULT_SANDBOX_POLICY` |
| P1.2 | `packages/types/src/script/script-executor.ts` | 修改 | `ExecutorMode` 扩展 `"sandbox-shell" \| "sandbox-python" \| "sandbox-javascript"`；新增 `ScriptLanguage` (`"auto" \| "shell" \| "python" \| "javascript"`) |
| P1.3 | `packages/types/src/script/script.ts` | 修改 | `Script` 接口增加 `language?: ScriptLanguage`；重构 `SandboxConfig` (mode/policy/strategies/customProvider/vfs + 遗留兼容字段) |
| P1.4 | `packages/types/src/script/script-schema.ts` | 修改 | Schema 适配新 `SandboxConfig` |
| P1.5 | `packages/types/src/script/index.ts` | 修改 | 导出 `script-sandbox.ts` 类型 |

**验证**: `pnpm --filter types build` + `pnpm --filter types test`

## Phase 2: 沙箱框架层

**目标**: 实现 `SandboxRuntime` + `DefaultStrategyResolver`, 完成配置解析和策略路由

| # | 文件 | 操作 | 内容 |
|---|------|------|------|
| P2.1 | `sdk/services/sandbox/default-policy.ts` | **新建** | `DEFAULT_SANDBOX_POLICY` 常量 (从 `script-sandbox.ts` 移出或引用) |
| P2.2 | `sdk/services/sandbox/strategy-resolver.ts` | **新建** | `DefaultStrategyResolver` 实现 `StrategyResolver` 接口, 内置默认策略实现注册 |
| P2.3 | `sdk/services/sandbox/sandbox-runtime.ts` | **新建** | `SandboxRuntime` 类: 解析 `SandboxConfig`, 提供 `createShellRuntime()` / `createPythonRuntime()` / `createJavaScriptRuntime()`, 按优先级列表解析最佳策略 |
| P2.4 | `sdk/services/sandbox/types.ts` | **新建** | 运行时类型 (`ShellSandboxRuntime`, `PythonSandboxRuntime`, `JavaScriptSandboxRuntime`, `PassthroughRuntime`) |
| P2.5 | `sdk/services/sandbox/index.ts` | **新建** | 模块导出 |

**验证**: 单元测试 `SandboxRuntime` 配置解析逻辑

## Phase 3: 策略实现

### Phase 3A: 轻量策略

| # | 文件 | 操作 | 内容 |
|---|------|------|------|
| P3A.1 | `sdk/services/sandbox/strategies/shell-static-analyzer.ts` | **新建** | 命令黑白名单 + 高危模式正则检测 + 路径参数检查 |
| P3A.2 | `sdk/services/sandbox/strategies/python-builtin-hook.ts` | **新建** | 注入安全包装代码, 替换 `builtins.open`/`__import__`, 通过子进程执行 |
| P3A.3 | `sdk/services/sandbox/strategies/python-ast-analyzer.ts` | **新建** | 通过子进程运行 `ast.parse`, 预检测高危调用 |
| P3A.4 | `sdk/services/sandbox/strategies/js-vm-context.ts` | **新建** | `vm.createContext()` + 受限 `require` + Proxy 拦截高危模块 |

**验证**: 单元测试每种策略的检测逻辑

### Phase 3B: OS 级 Hook (可选, 平台相关)

| # | 文件 | 操作 | 内容 |
|---|------|------|------|
| P3B.1 | `sdk/services/sandbox/strategies/os-hook/linux-seccomp.ts` | **新建** | seccomp-bpf syscall 过滤 (通过辅助进程或 LD_PRELOAD) |
| P3B.2 | `sdk/services/sandbox/strategies/os-hook/windows-job-object.ts` | **新建** | Win32 Job Object API (通过 node-ffi) |
| P3B.3 | `sdk/services/sandbox/strategies/os-hook/proot-like-redirect.ts` | **新建** | ptrace/DLL 系统调用拦截路径重定向 |

**验证**: 在对应平台上集成测试

## Phase 4: Executor + Engine 集成

| # | 文件 | 操作 | 内容 |
|---|------|------|------|
| P4.1 | `sdk/core/script/executors/base-executor.ts` | 修改 | `BaseExecuteOptions` 增加 `language?: ScriptLanguage`, `sandboxConfig?: Pick<SandboxConfig, 'mode' \| 'policy'>` |
| P4.2 | `sdk/core/script/executors/sandbox-shell-executor.ts` | **新建** | 委托 `SandboxRuntime.createShellRuntime()`, 调用对应 strategy |
| P4.3 | `sdk/core/script/executors/sandbox-python-executor.ts` | **新建** | 委托 `SandboxRuntime.createPythonRuntime()` |
| P4.4 | `sdk/core/script/executors/sandbox-javascript-executor.ts` | **新建** | 委托 `SandboxRuntime.createJavaScriptRuntime()` |
| P4.5 | `sdk/core/script/executors/index.ts` | 修改 | 注册三种 sandbox executor |
| P4.6 | `sdk/core/script/engine/script-engine.ts` | 修改 | 集成 `SandboxRuntime`, 实现语言检测 + executor 自动路由 |

**验证**: `pnpm --filter sdk test` + 端到端工作流测试

## Phase 5: VFS 模块 (可选)

| # | 文件 | 操作 | 内容 |
|---|------|------|------|
| P5.1 | `sdk/services/vfs/types.ts` | **新建** | `VFSEntry`, `VFSOperations`, `VFSConfig`, `DeltaFileSystem` 接口 |
| P5.2 | `sdk/services/vfs/whiteout-cache.ts` | **新建** | Trie 结构 Whiteout 缓存 |
| P5.3 | `sdk/services/vfs/delta-fs.ts` | **新建** | `MemoryDelta` 实现 |
| P5.4 | `sdk/services/vfs/base-fs.ts` | **新建** | `BaseFileSystem` (HostFS + PathPolicy) |
| P5.5 | `sdk/services/vfs/overlay-vfs.ts` | **新建** | `OverlayVFS` 核心 (三层查找语义) |
| P5.6 | `sdk/services/vfs/index.ts` | **新建** | 模块导出 |

**验证**: VFS 读写 + snapshot/restore 单元测试

## 详细任务列表

### Phase 1: 类型定义

- [ ] P1.1: 创建 `packages/types/src/script/script-sandbox.ts`
- [ ] P1.2: 修改 `packages/types/src/script/script-executor.ts`
- [ ] P1.3: 修改 `packages/types/src/script/script.ts`
- [ ] P1.4: 修改 `packages/types/src/script/script-schema.ts`
- [ ] P1.5: 修改 `packages/types/src/script/index.ts`

### Phase 2: 沙箱框架

- [ ] P2.1: 创建 `sdk/services/sandbox/default-policy.ts`
- [ ] P2.2: 创建 `sdk/services/sandbox/strategy-resolver.ts`
- [ ] P2.3: 创建 `sdk/services/sandbox/sandbox-runtime.ts`
- [ ] P2.4: 创建 `sdk/services/sandbox/types.ts`
- [ ] P2.5: 创建 `sdk/services/sandbox/index.ts`

### Phase 3A: 轻量策略

- [ ] P3A.1: 创建 `sdk/services/sandbox/strategies/shell-static-analyzer.ts`
- [ ] P3A.2: 创建 `sdk/services/sandbox/strategies/python-builtin-hook.ts`
- [ ] P3A.3: 创建 `sdk/services/sandbox/strategies/python-ast-analyzer.ts`
- [ ] P3A.4: 创建 `sdk/services/sandbox/strategies/js-vm-context.ts`

### Phase 3B: OS 级 Hook (可选)

- [ ] P3B.1: 创建 `sdk/services/sandbox/strategies/os-hook/linux-seccomp.ts`
- [ ] P3B.2: 创建 `sdk/services/sandbox/strategies/os-hook/windows-job-object.ts`
- [ ] P3B.3: 创建 `sdk/services/sandbox/strategies/os-hook/proot-like-redirect.ts`

### Phase 4: Executor + Engine 集成

- [ ] P4.1: 修改 `sdk/core/script/executors/base-executor.ts`
- [ ] P4.2: 创建 `sdk/core/script/executors/sandbox-shell-executor.ts`
- [ ] P4.3: 创建 `sdk/core/script/executors/sandbox-python-executor.ts`
- [ ] P4.4: 创建 `sdk/core/script/executors/sandbox-javascript-executor.ts`
- [ ] P4.5: 修改 `sdk/core/script/executors/index.ts`
- [ ] P4.6: 修改 `sdk/core/script/engine/script-engine.ts`

### Phase 5: VFS 模块 (可选)

- [ ] P5.1: 创建 `sdk/services/vfs/types.ts`
- [ ] P5.2: 创建 `sdk/services/vfs/whiteout-cache.ts`
- [ ] P5.3: 创建 `sdk/services/vfs/delta-fs.ts`
- [ ] P5.4: 创建 `sdk/services/vfs/base-fs.ts`
- [ ] P5.5: 创建 `sdk/services/vfs/overlay-vfs.ts`
- [ ] P5.6: 创建 `sdk/services/vfs/index.ts`

## 目录结构变化

```
修改前:
packages/types/src/script/
├── script.ts
├── script-executor.ts
├── script-schema.ts
└── index.ts

无 sdk/services/sandbox/
无 sdk/services/vfs/

修改后:
packages/types/src/script/
├── script.ts                        # 修改
├── script-executor.ts               # 修改
├── script-sandbox.ts                # 新建
├── script-schema.ts                 # 修改
└── index.ts                         # 修改

sdk/services/
├── sandbox/
│   ├── types.ts                     # 新建
│   ├── sandbox-runtime.ts           # 新建
│   ├── strategy-resolver.ts         # 新建
│   ├── default-policy.ts            # 新建
│   ├── index.ts                     # 新建
│   └── strategies/
│       ├── shell-static-analyzer.ts # 新建
│       ├── python-builtin-hook.ts   # 新建
│       ├── python-ast-analyzer.ts   # 新建
│       ├── js-vm-context.ts         # 新建
│       └── os-hook/                 # 新建 (可选)
│           ├── linux-seccomp.ts
│           ├── windows-job-object.ts
│           └── proot-like-redirect.ts
│
├── vfs/                             # 新建 (可选)
│   ├── types.ts
│   ├── overlay-vfs.ts
│   ├── whiteout-cache.ts
│   ├── delta-fs.ts
│   ├── base-fs.ts
│   └── index.ts
│
└── ... (terminal 等现有模块不变)

sdk/core/script/executors/
├── base-executor.ts                 # 修改
├── sandbox-shell-executor.ts        # 新建
├── sandbox-python-executor.ts       # 新建
├── sandbox-javascript-executor.ts   # 新建
├── index.ts                         # 修改
└── (direct/shared/pty 不变)
```