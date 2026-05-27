# VFS 模块集成分析及实施计划

## 当前状态

### 已完成

| 文件/模块 | 状态 |
|---|---|
| **Phase 5: VFS Overlay** — `sdk/services/vfs/` 全部 6 个文件 | ✅ 完成 |
| **Phase 2: 策略实现** — 4 个策略实现 + 3 个 shell analyzer | ✅ 完成 |
| **Phase 6: OS Hook 骨架** — `os-hook.ts` | ✅ 骨架完成 |
| **Phase 1: 类型体系** — `packages/types/` 类型定义 | ✅ 完成 |

### 缺失

| 文件/模块 | 状态 |
|---|---|
| **Phase 3: `sandbox-runtime.ts`** | ❌ 缺失 |
| **Phase 3: `strategy-resolver.ts`** | ❌ 缺失 |
| **Phase 3: `default-policy.ts`** | ❌ 缺失 |
| **Phase 4: 3 个沙箱执行器** | ❌ 缺失 |
| **Phase 7: ScriptExecutor/ScriptEngine 集成** | ❌ 缺失 |
| **Phase 8: 测试** | ❌ 缺失 |

## 关键发现

1. **VFS 模块是"孤岛"** — `OverlayVFS`、`CheckpointAwareVFS` 已被完整实现，但无任何代码引用
2. **VFS 未从 `services/index.ts` 导出**
3. **无 `SandboxRuntime`** — 缺少"胶水层"连接 VFS 与策略执行
4. **`ScriptExecutor` 不认识 sandbox 模式** — 只注册了 `direct` 和 `shared`

## VFS 集成架构

```
SandboxConfig (含 vfs config)
        │
        ▼
SandboxRuntime ───────────────► OverlayVFS
   │                                   │
   │  createShellRuntime(...)          │ Delta (MemoryDelta)
   │  createPythonRuntime(...)         │ WhiteoutCache
   │  createJavaScriptRuntime(...)     │ Base (HostFS)
   │                                   │
   ▼                                   ▼
SandboxExecutor ──────────────►  CheckpointAwareVFS
   │                                   │
   │  execute(options)                 │ onCheckpointCreate → snapshot()
   │                                   │ onCheckpointRestore → restore()
   ▼                                   ▼
ScriptExecutionResult           Checkpoint System
```

## 执行计划

### Phase 3: Runtime & Resolver

- `default-policy.ts` — 默认安全策略常量
- `strategy-resolver.ts` — 策略解析与回退链
- `sandbox-runtime.ts` — 运行时（含 VFS 生命周期管理）

### Phase 4: 沙箱执行器

- `sandbox-shell-executor.ts`
- `sandbox-python-executor.ts`
- `sandbox-javascript-executor.ts`

### Phase 7: 集成对接

- `base-executor.ts` — 扩展 BaseExecuteOptions
- `script-executor.ts` — 注册沙箱执行器
- `script-engine.ts` — 注册沙箱执行器

### VFS 特殊集成

- `services/index.ts` — 导出 VFS 和 Sandbox 模块
- Checkpoint 系统 — 接入 CheckpointAwareVFS

## 文件影响范围

| 操作 | 文件 | 阶段 |
|---|---|---|
| 新建 | `sdk/services/sandbox/default-policy.ts` | P3 |
| 新建 | `sdk/services/sandbox/strategy-resolver.ts` | P3 |
| 新建 | `sdk/services/sandbox/sandbox-runtime.ts` | P3 + VFS |
| 修改 | `sdk/services/sandbox/index.ts` | - |
| 修改 | `sdk/services/index.ts` | - |
| 修改 | `sdk/core/script/executors/base-executor.ts` | P4 |
| 新建 | `sdk/core/script/executors/sandbox-shell-executor.ts` | P4 |
| 新建 | `sdk/core/script/executors/sandbox-python-executor.ts` | P4 |
| 新建 | `sdk/core/script/executors/sandbox-javascript-executor.ts` | P4 |
| 修改 | `sdk/core/executors/script-executor.ts` | P7 |
| 修改 | `sdk/core/script/engine/script-engine.ts` | P7 |
| 修改 | checkpoint 协调器 | VFS |