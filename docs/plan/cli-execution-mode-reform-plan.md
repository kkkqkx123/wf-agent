# CLI-App Execution Mode Reform Plan

> 基于对 cli-app 执行模式的全面分析，提出的系统整改方案。
> 分析报告见 `apps/cli-app/docs/cli-app-mode-analysis.md`。

---

## 目录

1. [概述](#1-概述)
2. [已识别的问题清单](#2-已识别的问题清单)
3. [整改目标](#3-整改目标)
4. [Phase 1 —— 子进程模型改造](#4-phase-1--子进程模型改造)
5. [Phase 2 —— 测试体系重构](#5-phase-2--测试体系重构)
6. [Phase 3 —— 补充集成测试](#6-phase-3--补充集成测试)
7. [Phase 4 —— 概念清理与文档更新](#7-phase-4--概念清理与文档更新)
8. [影响分析](#8-影响分析)
9. [风险评估](#9-风险评估)

---

## 1. 概述

当前 cli-app 的执行模式（execution mode）采用两层正交模型设计（`ExecutionMode` × `WorkflowExecutionMode`），架构方向合理。但存在以下关键缺陷：

- **进程模型错误**：`foreground` 和 `background` 均为 CLI 同进程执行，无法脱离 CLI 存活，命名与行为严重不符
- **测试覆盖严重缺失**：`execution run` 命令零集成测试覆盖，所有模式组合的运行时行为未经验证
- **TEST_MODE 语义混淆**：等价于 HEADLESS 导致 foreground/background 降级，使非 blocking 模式不可测
- **概念冗余**：programmatic、TEST_MODE 等历史残留增加认知负荷

**核心整改方向**：将 foreground/background 改造为真正的子进程模型，同时重构测试体系使所有模式可测。

---

## 2. 已识别的问题清单

### P0 —— 必须修复

| ID | 问题 | 说明 |
|----|------|------|
| P0-1 | foreground/background 同进程执行 | 两者均在 CLI 进程内通过 SDK 异步执行，postAction hook 调用 shutdown() 时销毁 SDK 导致执行中断。**命名（foreground/background）与实际行为（in-process）严重不符** |
| P0-2 | `execution run` 零集成测试覆盖 | 所有模式组合的运行时行为从未被验证；模式降级逻辑确认但从未执行过 |
| P0-3 | TEST_MODE 等价 HEADLESS | 导致 foreground/background 降级 blocking，无法通过 CI 测试非 blocking 模式 |

### P1 —— 应修复

| ID | 问题 | 说明 |
|----|------|------|
| P1-1 | headless JSON 输出路径未测试 | 集成测试全部用 `CLI_OUTPUT_FORMAT=text`，JSON 消费者依赖的输出格式可能有 bug |
| P1-2 | mode detector 单元测试不完整 | `configFallback` 参数无单元测试；检测优先级路径未被完整覆盖 |
| P1-3 | 无 `test` 执行模式 | 测试与产品运行共享 headless 语义，无法在测试中独立控制降级行为 |

### P2 —— 建议修复

| ID | 问题 | 说明 |
|----|------|------|
| P2-1 | TUI 启动路径无集成测试 | `startTUI()` 从未被集成测试调用/验证 |
| P2-2 | programmatic 历史残留 | `detector.ts:59` 仍有 `process.env.HEADLESS` 和 `TEST_MODE` 独立判断路径，概念冗余 |
| P2-3 | `execution follow/status/cancel` 无测试 | 其他 execution 子命令也未被覆盖 |

---

## 3. 整改目标

1. **P0-1**: foreground/background 改造为子进程模型，实现真正的 OS 级存活分离
2. **P0-2**: 建立完整的 `execution run` 集成测试套件
3. **P0-3**: 引入 `test` 独立执行模式，消除 TEST_MODE 的副作用
4. **P1-1~P1-3**: 补齐 mode detector 单元测试、JSON 输出测试、test mode 基础设施
5. **P2-1~P2-3**: 补充 TUI 启动测试、清理历史残留、补充其他 execution 命令测试

---

## 4. Phase 1 —— 子进程模型改造

### 4.1 设计目标

| 模式 | 当前行为 | 目标行为 |
|------|---------|---------|
| blocking | 同进程同步 + await SDK 完成 | **不变** |
| foreground | 同进程异步 + node-pty 伪终端展示 | 子进程执行 + 子进程内建伪终端展示，CLI 可先行退出 |
| background | 同进程异步 + 日志文件 | 子进程执行 + 日志文件，可脱离 CLI 存活 |

### 4.2 架构变更

```
                 ┌──────────────────┐
                 │   CLI 主进程      │
                 │  (Commander)      │
                 └──────┬──────────┘
                        │ execute()
                 ┌──────▼──────────┐
                 │ ExecutionService │
                 └──────┬──────────┘
                        │
           ┌────────────┼────────────┐
           ▼            ▼            ▼
     blocking      foreground    background
     (同进程)      (子进程)      (子进程)
                    │              │
           ┌────────┘              └────────┐
           ▼                                 ▼
    child_process.fork()              child_process.fork()
    + node-pty 伪终端                  + 日志文件写入
    + 事件流输出到 stdout               + 无终端，纯日志
```

### 4.3 详细设计

#### 4.3.1 子进程入口文件

新建 `apps/cli-app/src/executors/child-executor.ts`，作为子进程入口：

```
功能：
  1. 接收父进程通过 IPC/argv 传递的参数（workflowId, input, mode）
  2. 初始化自己的 SDK 实例（轻量级，共享同一存储）
  3. 执行工作流
  4. 通过进程间通信（IPC）向父进程报告状态/事件
  5. 子进程独立于父进程存活
```

关键决策点：

- **通信方式**：使用 `child_process.fork()` 的 IPC 通道，事件流通过 `process.send()` 向父进程发送
- **SDK 初始化**：子进程需要自己的 SDK 实例。存储层需要支持多进程访问（SQLite 支持并发读，写需要 WAL 模式或锁机制）
- **进程生命周期**：子进程 detach 后父进程可以退出，子进程继续存活。子进程完成后自行 exit

#### 4.3.2 ExecutionService 改动

```
execute() 方法：
  - blocking:        不变（同进程 await SDK）
  - foreground:      fork('child-executor.js', [workflowId, input], { detached: true })
                      + 子进程创建 node-pty（如果 TTY 可用）
                      + 父进程通过 IPC 接收事件流，转发到 node-pty
                      + 父进程可退出，子进程继续运行
  - background:      fork('child-executor.js', [workflowId, input], { detached: true })
                      + 子进程将事件写入日志文件
                      + 父进程立即返回子进程 PID + 执行 ID
                      + 父进程可退出，子进程继续运行
```

**validateModeCombination 调整**：

```
interactive + foreground → ✓（子进程 node-pty 展示）
interactive + background → ✓（子进程日志文件）
interactive + blocking   → ✓（同进程同步）

headless + foreground    → blocking（产品无头场景+前台终端无意义）
headless + background    → background（子进程日志文件仍有意义）
headless + blocking      → blocking

test + foreground        → ✓（子进程，IPC 捕获事件）
test + background        → ✓（子进程日志文件）
test + blocking          → ✓（同进程同步）
```

#### 4.3.3 ExecutionResult 接口变更

```typescript
export interface ExecutionResult {
  mode: WorkflowExecutionMode;
  executionId: string;
  workflowId: string;
  status: string;
  startTime: Date;
  /** 子进程 PID（foreground/background 模式） */
  pid?: number;
  /** 子进程是否已分离（true 表示父进程可先行退出） */
  detached: boolean;
  /** 父进程可 await 等待子进程完成（用于 CLI 场景） */
  completion?: Promise<void>;
  /** 日志文件路径（background 模式） */
  logFile?: string;
  /** blocking 模式的直接结果 */
  result?: WorkflowExecutionResult;
}
```

#### 4.3.4 存储层注意事项

子进程模型要求存储层支持多进程访问：

| 存储类型 | 多进程支持 | 说明 |
|---------|-----------|------|
| Memory | ❌ 不适用 | 子进程无法访问父进程内存，仅用于单进程测试 |
| SQLite (WAL) | ✅ 可支持 | WAL 模式支持多读一写，需要处理并发 |
| JSON 文件 | ✅ 可支持 | 需文件锁防并发写入 |

当前测试使用 SQLite/Memory，生产环境使用 SQLite 需要确保 WAL 模式已启用。

---

## 5. Phase 2 —— 测试体系重构

### 5.1 引入 `test` 执行模式

在 `ExecutionMode` 类型中新增独立值，使测试环境不再等价于 headless：

```typescript
// packages/runtime/src/mode/types.ts
export type ExecutionMode = "interactive" | "headless" | "test";
```

**test 模式语义**：

| 属性 | 值 | 说明 |
|------|----|------|
| 默认输出格式 | `text` | 便于测试断言 |
| foreground 降级 | 不降级 | 允许测试 foreground 路径 |
| background 降级 | 不降级 | 允许测试 background 路径 |
| 退出行为 | ExitManager.exit() | 同 headless，drain 后退出 |
| 颜色 | 禁用 | 同 headless |

**检测优先级**（新增 `TEST_MODE` 语义）：

```
CLI_MODE=test          → test（最高优先级）
CLI_MODE=headless      → headless
TEST_MODE=true         → test（不再是 headless）
CLI_MODE=interactive   → interactive
config.executionMode   → 回退值
默认                   → interactive
```

**向下兼容**：现有所有 `TEST_MODE=true` 在 headless 下工作的测试（全部为 workflow 管理命令，不涉及 `execution run`）不受影响，因为 test 和 headless 的行为差异主要在 execution 模式降级上。

### 5.2 CLIRunner 变更

```typescript
// 默认环境变量
this.defaultEnv = {
  NODE_ENV: "test",
  CLI_MODE: "test",          // 从 TEST_MODE=true 改为 CLI_MODE=test
  CLI_OUTPUT_FORMAT: "text",
  // ...其余不变
};
```

新增支持可选配置：

```typescript
interface CLIRunOptions {
  // ...现有属性
  mode?: "test" | "headless" | "interactive";  // 覆盖执行模式
  mockTTY?: boolean;                            // 为 foreground 测试 mock TTY
}
```

### 5.3 mode detector 单元测试补齐

新增 `packages/runtime/src/mode/__tests__/detector.test.ts`，覆盖：

| 测试用例 | 验证点 |
|---------|--------|
| CLI_MODE=test → mode=test | test 模式识别 |
| TEST_MODE=true → mode=test （不再是 headless） | TEST_MODE 新语义 |
| CLI_MODE=headless + TEST_MODE=true → mode=headless | CLI_MODE 优先级更高 |
| configFallback="headless" + 无 env var → mode=headless | config 回退 |
| configFallback="test" + 无 env var → mode=test | config 回退 test |
| 无任何设置 → interactive | 硬编码默认 |
| test 模式下 outputFormat 默认为 text | test 输出格式 |
| NO_COLOR 设置 → colorEnabled=false | 颜色控制 |
| invalidateModeCache 后重新检测 | 缓存失效机制 |

### 5.4 validateModeCombination 单元测试补齐

新增 `apps/cli-app/__tests__/services/execution/execution-service.test.ts`：

| 测试用例 | 预期 |
|---------|------|
| interactive + foreground | → foreground |
| interactive + background | → background |
| interactive + blocking | → blocking |
| headless + foreground | → blocking（降级 + warnLog） |
| headless + background | → blocking（降级 + warnLog） |
| headless + blocking | → blocking |
| test + foreground | → foreground（不降级） |
| test + background | → background（不降级） |
| test + blocking | → blocking |

---

## 6. Phase 3 —— 补充集成测试

### 6.1 基础 structure

在 `apps/cli-app/__tests__/integration/workflows/06-execution.test.ts` 中新增：

```typescript
describe("Workflow Execution Tests", () => {
  // beforeAll: 注册测试用工作流
  // beforeEach: 创建隔离的 STORAGE_DIR
  // afterEach: 清理
});
```

### 6.2 测试用例矩阵

#### blocking 模式（test mode）

| 用例 | 验证点 |
|------|--------|
| blocking 执行标准工作流 | exitCode=0, stdout 含 executionId/status/completed |
| blocking 执行无效 workflowId | exitCode≠0, stderr 含错误消息 |
| blocking 带 JSON input | stdout JSON 包含 input 对应结果 |
| blocking 执行失败的工作流 | exitCode=0, stdout 含 failed/failure |

#### blocking 模式（headless mode + CLI_OUTPUT_FORMAT=json）

| 用例 | 验证点 |
|------|--------|
| headless + blocking + JSON 输出 | stdout 是有效 JSON，含 executionId/status |
| headless + blocking 带 -b flag | 与默认行为一致 |

#### foreground 模式（test mode，mock TTY）

| 用例 | 验证点 |
|------|--------|
| foreground 启动工作流 | 返回 executionId + pid，事件流通过 IPC 传输 |
| foreground 子进程可脱离父进程存活 | 父进程退出后子进程仍可继续 |
| foreground 输出事件到 stdout | stdout 含 node started/completed 事件 |

#### background 模式（test mode）

| 用例 | 验证点 |
|------|--------|
| background 启动工作流 | 返回 executionId + pid + logFile |
| background 日志文件写入 | 日志文件存在且包含执行事件 |
| background 子进程可脱离父进程存活 | CLI 退出后子进程继续执行，日志继续写入 |

#### follow 命令

| 用例 | 验证点 |
|------|--------|
| follow 存在的 execution | 可接收到事件流 |
| follow 不存在的 execution | 返回错误 |

#### mode 降级验证

| 用例 | 验证点 |
|------|--------|
| headless + 默认(foreground) → blocking | stderr 含降级告警 |
| headless + --background → blocking | stderr 含降级告警 |
| headless + -b | blocking 正常执行，无告警 |
| test + foreground → foreground | 不降级, 无告警 |

### 6.3 TUI 启动测试

```typescript
describe("TUI Starting", () => {
  it("should start TUI when no args and interactive mode", async () => {
    // 设置 CLI_MODE=interactive
    // 不传递子命令
    // 预期：启动 TUI（需要 mock TTY）
  });
  
  it("should fallback to help when no TTY", async () => {
    // stdio 非 TTY
    // 预期：输出 help 而非启动 TUI
  });
  
  it("should fallback to help when output format is not text", async () => {
    // CLI_OUTPUT_FORMAT=json
    // 预期：输出帮助信息并退出
  });
});
```

### 6.4 CLIRunner 测试增强

为支持子进程 + IPC 测试模型，CLIRunner 需要增强：

```typescript
export interface CLIRunOptions {
  // ...现有属性
  receiveIPC?: boolean;         // 是否接收 IPC 消息
  onIPCMessage?: (msg: any) => void;  // IPC 消息回调
  waitForDetach?: boolean;      // 等待子进程 detach
}
```

---

## 7. Phase 4 —— 概念清理与文档更新

### 7.1 代码清理

1. 移除 `detector.ts` 中的 `programmatic` 相关逻辑
   - `detectMode()` 中移除 `cliMode === "programmatic"` 路径（P2-2）
   - `isProgrammatic()` 标记为 `@deprecated` 并返回 `false`（实际已做，但需确认完全移除）
2. `ExecutionMode` 类型删除 `programmatic` 文档中的兼容说明
3. `WorkflowExecutionMode` 类型命名确认无 `detached` 残余（实际已修复 ✅）

### 7.2 概念重命名（可选）

| 当前名 | 建议 | 理由 |
|--------|------|------|
| foreground | live / attached | 子进程 + 实时显示，更准确 |
| background | detached / daemon | 子进程 + 日志文件，脱离 CLI |

> **注意**：重命名是破坏性变更，需与下游消费者（server app、文档、用户脚本）协调。建议放在最后执行或在文档中同时标注新旧名。

### 7.3 文档更新

1. `apps/cli-app/docs/cli-app-mode-analysis.md`：更新为反映子进程模型
2. `apps/cli-app/docs/tests/test-mode-analysis.md`：更新测试策略
3. `packages/runtime/src/mode/types.ts` 注释：更新 `test` 模式说明
4. `packages/types/src/execution/workflow-execution-mode.ts` 注释：更新为子进程模型

---

## 8. 影响分析

### 8.1 正向影响

| 领域 | 影响 |
|------|------|
| 进程模型 | foreground/background 获得真正的 OS 级分离，可脱离 CLI 存活 |
| 测试能力 | 所有执行模式均可通过 CI 测试（子进程 fork + IPC 可 mock） |
| 概念清晰度 | test 模式独立后不再混淆，headless 只用于产品无头场景 |
| 可观测性 | 子进程 IPC 提供标准化的执行事件流 |
| 可维护性 | 架构清晰、测试充分，重构风险降低 |

### 8.2 负面影响

| 领域 | 影响 | 缓解措施 |
|------|------|---------|
| 复杂度 | 子进程管理增加代码量（进程生命周期、IPC 协议、错误传播） | 使用 `child_process.fork()` 标准 API，封装 Executor 抽象 |
| 存储 | 多进程并发访问存储需要 WAL 模式 | 确保 SQLite 使用 WAL，JSON 存储增加文件锁 |
| 测试速度 | 子进程启动比直接调用 SDK 慢 | blocking 模式和部分单元测试仍使用同进程路径 |
| 调试难度 | 子进程内部错误需要通过 IPC 传输 | 统一错误序列化格式，保留 `--debug` flag |

### 8.3 向后兼容性

| 变更 | 兼容性 |
|------|--------|
| `ExecutionMode` 新增 `test` | ✅ 非破坏性（union type 扩展） |
| `ExecutionResult` 新增 `detached` 字段 | ✅ 非破坏性（新字段可选） |
| TEST_MODE 新语义（test ≠ headless） | ⚠️ 潜在破坏：现有依赖 TEST_MODE=headless 假设的代码需调整 |
| foreground/background 改为子进程 | ⚠️ 潜在破坏：依赖同进程行为的消费者需评估 |
| 移除 programmatic | ⚠️ 之前已标记 deprecated，现完全移除 |

---

## 9. 风险评估

| 风险 | 级别 | 缓解 |
|------|------|------|
| 子进程 SDK 初始化资源消耗 | 中 | 子进程 SDK 使用轻量级配置（Memory 存储用于测试） |
| 多进程存储并发写冲突 | 高 | SQLite WAL 模式 + 重试机制；测试使用隔离 STORAGE_DIR |
| 子进程在 Windows 上行为差异 | 中 | `child_process.fork()` 跨平台支持良好，但 detach 可能需额外处理 |
| TEST_MODE 语义变更影响现有测试 | 低 | 现有测试均不涉及 execution run，不受影响 |
| foreground 子进程创建 node-pty 在无 TTY 环境失败 | 低 | 无 TTY 时自动降级 blocking |

---

## 10. 实施路线图

```
Phase 1: 子进程模型改造（2-3周）
├── Week 1: child-executor.ts 入口 + IPC 协议
├── Week 2: ExecutionService 改造 + validateModeCombination 调整
└── Week 3: 存储层并发适配 + 手动集成测试

Phase 2: 测试体系重构（1-2周）
├── Week 1: ExecutionMode 新增 test + detector 单元测试
└── Week 2: validateModeCombination 单元测试 + CLIRunner 增强

Phase 3: 补充集成测试（1-2周）
├── Week 1: execution run 集成测试（blocking + test mode）
└── Week 2: foreground/background 集成测试 + TUI 启动测试

Phase 4: 概念清理与文档（1周）
├── programmatic 残留清理
├── 可选重命名评估
└── 文档更新
```

**总计估计：5-8 周**，取决于子进程存储并发问题的复杂度。

---

## 附录 A：子进程 IPC 协议

```
父进程 → 子进程（启动时 argv）:
  workflowId: string
  input: Record<string, unknown>
  mode: 'foreground' | 'background'
  storageDir: string
  debug: boolean

子进程 → 父进程（IPC send）:
  { type: 'status',   status: 'starting' | 'running' | 'completed' | 'failed' }
  { type: 'event',    event: BaseEvent }
  { type: 'progress', progress: number }
  { type: 'result',   result: WorkflowExecutionResult }
  { type: 'error',    error: { message: string, code: string } }
```

---

## 附录 B：文件改动清单

| 路径 | 改动类型 | 说明 |
|------|---------|------|
| `apps/cli-app/src/executors/child-executor.ts` | **新建** | 子进程入口 |
| `apps/cli-app/src/services/execution/execution-service.ts` | 修改 | 子进程 fork + IPC |
| `packages/runtime/src/mode/types.ts` | 修改 | 新增 test mode |
| `packages/runtime/src/mode/detector.ts` | 修改 | test mode 检测 + 清理 programmatic |
| `packages/runtime/src/mode/index.ts` | 修改 | 导出 test mode |
| `apps/cli-app/src/utils/mode-detector.ts` | 修改 | 同步更新（薄封装层） |
| `apps/cli-app/src/index.ts` | 修改 | postAction 对子进程的处理 |
| `apps/cli-app/__tests__/__shared/cli-runner.ts` | 修改 | 支持 CLI_MODE=test + IPC 选项 |
| `apps/cli-app/__tests__/setup.ts` | 修改 | 测试环境 test mode 配置 |
| `packages/runtime/src/mode/__tests__/detector.test.ts` | **新建** | mode detector 单元测试 |
| `apps/cli-app/__tests__/services/execution/execution-service.test.ts` | **新建** | validateModeCombination + execute 单元测试 |
| `apps/cli-app/__tests__/integration/workflows/06-execution.test.ts` | **新建** | execution run 集成测试 |
| `apps/cli-app/__tests__/integration/workflows/07-execution-modes.test.ts` | **新建** | 模式组合降级集成测试 |
| `apps/cli-app/__tests__/tui/core.test.ts` | 修改 | 补充 TUI 启动路径测试 |
