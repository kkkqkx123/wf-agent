# wf-agent · cli-app 执行模式分析报告

> 仓库：`https://github.com/kkkqkx123/wf-agent`
> 分析对象：`apps/cli-app`（包名 `@wf-agent/cli-app`，bin 名 `wf-agent`）
> 核心结论：**cli-app 的执行模式是"两层模型 + 正交校验"** ——
> - 顶层「应用运行模式」决定 CLI 整体运行方式（interactive / headless / programmatic）
> - 工作流层「执行模式」决定单次工作流调度方式（blocking / detached / background）
> - **新增的 `validateModeCombination()` 矩阵校验确保不合法组合被自动降级**

---

## 1. 总体架构速览

CLI 采用 **Commander.js → Adapter → SDK** 的分层结构。所有命令最终落到统一的 `ExecutionService`，再经 SDK 执行。

```
                 ┌─────────────────────────────────────────────┐
   wf-agent ───▶ │  index.ts (Commander 入口 + 全局 preAction)   │
                 └───────────────┬─────────────────────────────┘
                                 │  preAction: 加载配置 → 初始化 Output →
                                 │  createAppSDK() → 初始化 Container
                                 ▼
                 ┌─────────────────────────────────────────────┐
                 │  Runtime Mode Detector  (@wf-agent/runtime/mode) │
                 │   → interactive | headless | programmatic     │
                 │   支持 configFallback 参数（环境变量 > 配置 > 默认值）│
                 └───────────────┬─────────────────────────────┘
                                 │
            ┌────────────────────┼─────────────────────┐
            ▼                    ▼                     ▼
     [正常命令]           [--tui 且无参数]         [无参数+headless]
            │              → 启动 CLIAppTUI          → 退出
            ▼
  Adapter → ExecutionService → SDK（统一执行层）
                                 │
              ┌──────────────────┼───────────────────┐
              ▼    校验矩阵       ▼                   ▼
        detached (默认)      background          blocking
       (进程内异步+event展示) (后台+日志文件)      (当前终端阻塞)
```

---

## 2. 第一层：顶层「应用运行模式」（Runtime Execution Mode）

### 2.1 检测机制

定义在 `packages/runtime/src/mode/detector.ts`，由 `apps/cli-app/src/utils/mode-detector.ts` 薄封装后复用。检测优先级为：

```
环境变量 (CLI_MODE / HEADLESS / TEST_MODE)     ← 优先级最高
AppConfig.executionMode（从配置文件读取）        ← 中间层
硬编码默认值 "interactive"                      ← 最低优先级
```

`getMode(configFallback?)` 函数支持将配置文件中的 `executionMode` 字段作为回退参数传入。

| 环境变量 | 取值 | 效果 |
|---|---|---|
| `CLI_MODE` | `programmatic` | 强制进入编程模式 |
| `CLI_MODE` | `headless` | 强制进入无头模式 |
| `HEADLESS` | `true` | 兼容旧版，进入无头模式 |
| `TEST_MODE` | `true` | 兼容旧版，无头模式（集成测试使用） |
| `CLI_OUTPUT_FORMAT` | `json` / `silent` / `text` | 指定输出格式 |
| `NO_COLOR` | 任意 | 禁用 ANSI 颜色 |

```ts
// 简化版检测逻辑（packages/runtime/src/mode/detector.ts）
function detectMode(configFallback?: ExecutionMode): ExecutionMode {
  const cliMode = process.env[CLI_MODE];
  if (cliMode === "programmatic") return "programmatic";
  if (cliMode === "headless" || process.env.HEADLESS === "true"
      || process.env.TEST_MODE === "true") return "headless";
  if (configFallback === "headless" || configFallback === "programmatic")
    return configFallback;
  return "interactive";
}
```

### 2.2 三种应用模式对照

| 模式 | 触发方式 | 输出格式默认 | 颜色 | TTY 要求 | 退出行为 | 典型场景 |
|---|---|---|---|---|---|---|
| **interactive** | 默认 / 未设 env | `text` | 跟随 TTY | 需要 | 命令后保持运行 | 人工终端使用 |
| **headless** | `CLI_MODE=headless` / `HEADLESS=true` / `TEST_MODE=true` | `json` | 关闭 | 不要求 | 命令完成自动 `exit(0)` | CI/CD、自动化测试 |
| **programmatic** | `CLI_MODE=programmatic` | 可编程结构 | 关闭 | 不要求 | 由调用方控制 | 程序调用 |

### 2.3 配置文件中的 executionMode

`packages/runtime/src/config/schema.ts` 中的 `DefaultAppConfigSchema` 新增了可选字段：

```ts
executionMode: z.enum(["interactive", "headless", "programmatic"]).optional()
```

在配置文件中可以写作：

```yaml
# wf-agent.yaml
executionMode: headless  # 默认无头模式（可由环境变量覆盖）
```

---

## 3. 第二层：工作流「执行模式」（Workflow Execution Mode）

作用于 `wf-agent execution run <workflow-id>` 子命令。三种模式由 `ExecutionService` 统一调度：

| 模式 | Flag | 执行位置 | 终端 | 阻塞？ | 返回内容 | 实现方法 |
|---|---|---|---|---|---|---|
| **detached**（默认） | 无 flag | SDK 共享实例 | node-pty 伪终端实时展示事件 | 否 | `executionId` + `terminalId` + `pid` | `executeDetached()` |
| **background** | `--background` | SDK 共享实例 | 无窗口，输出写入日志文件 | 否 | `executionId` + `pid` + `logFile` | `executeBackground()` |
| **blocking** | `-b` / `--blocking` | SDK 共享实例 | 当前终端 | 是 | 最终 `WorkflowExecutionResult` | `executeBlocking()` |

> ⚠ **重要说明**：`detached` 模式名为"分离"，但实际上工作流与 CLI 运行在**同一进程**中。`node-pty` 伪终端仅用于展示事件流，并非真正的 OS 级进程分离。如果 CLI 进程退出，detached 模式的工作流也会终止。真正的后台持久化运行应使用 `--background` 模式。

### 3.1 各模式详细流程

**detached（默认）**
1. `adapter.executeWorkflow()` 通过 SDK 启动工作流；
2. `terminalManager.createTerminal({ background: false })` 用 node-pty 建前台伪终端；
3. 调用 `subscribeToExecutionEvents()` 订阅 SDK 事件（`NODE_COMPLETED`、`WORKFLOW_EXECUTION_COMPLETED/FAILED`），实时渲染到伪终端；
4. 立即返回，主终端不阻塞。

**background（后台）**
1. 同样经 SDK 启动工作流；
2. `createTerminal({ background: true, logFile })` 用 `child_process.spawn`；
3. 调用 `subscribeToExecutionEvents()`（含 `NODE_STARTED` 事件订阅），写入日志文件；
4. 返回 `pid` 与 `logFile`，可用 `execution status <id>` 轮询。

**blocking（阻塞）**
1. `adapter.executeWorkflow()` 经 SDK 执行并 `await` 完成；
2. 结果经 `router.render()` 格式化输出；
3. 进程等待工作流结束后才返回。

### 3.2 公共事件流引擎

`setupEventStreaming()` 和 `setupBackgroundLogging()` 原有的重复代码已被抽取为 `subscribeToExecutionEvents()` 共享方法：

```
subscribeToExecutionEvents(executionId, terminal, options?)
  ├── 获取 SDK Factory + Dependencies（含 null 安全检查）
  ├── [可选] NODE_STARTED 事件订阅（仅 background 模式）
  ├── NODE_COMPLETED 事件订阅
  ├── WORKFLOW_EXECUTION_COMPLETED 事件订阅（含清理所有订阅的回调）
  └── WORKFLOW_EXECUTION_FAILED 事件订阅（含清理所有订阅的回调）
```

该方法自动适配前台（`terminal.pty.write()`）和后台（`terminal.pty.stdin.write()`）两种终端类型。

### 3.3 新增：execution follow 命令

新增了 `wf-agent execution follow <execution-id>` 命令，实时跟踪工作流执行进度：

- 直接向 `stdout`（而非 node-pty）输出事件流
- 输出格式为普通文本（非 box-drawing 字符），VSCode 友好
- 支持 headless 和 remote 场景
- 通过 `followExecution()` 方法实现，返回 `Promise<void>`，执行完成/失败时 resolve
- 可在 CI/CD 或远程 SSH 会话中使用

---

## 4. 模式组合校验矩阵

`validateModeCombination()` 方法是本次重构的核心新增。它在 `ExecutionService.execute()` 入口处自动校验应用运行模式与工作流执行模式是否兼容：

```
  app mode ↓  \  exec mode → | detached (默认) | blocking | background
  ---------------------------|--------------------|----------|------------
  interactive                | ✓                  | ✓        | ✓
  headless                   | ⚠ → blocking      | ✓        | ⚠ → blocking
  programmatic               | ⚠ → blocking      | ✓        | ✓

  图例: ✓ = 合法，直接使用 | ⚠ → = 不合法，自动降级并告警
```

**降级策略详解：**

| 触发组合 | 降级结果 | 原因 |
|---|---|---|
| `headless` + `detached` | `blocking` | headless 不应创建 node-pty 终端 |
| `headless` + `background` | `blocking` | headless 不应创建日志文件/子进程 |
| `programmatic` + `detached` | `blocking` | programmatic 不应创建非必要的终端 |

所有降级都伴随 `output.warnLog()` 告警输出。

---

## 5. 第三层：TUI 交互模式

当满足以下全部条件时，无子命令启动会进入全屏 TUI（`startTUI()` 动态 `import("./tui/index.js")`）：

```ts
const shouldStartTUI =
  (hasTuiFlag || executionMode === "interactive")
  && !process.argv.slice(2).length;
// 且 outputFormat === "text" 且 process.stdout.isTTY 存在
```

- 启动 `CLIAppTUI`，注册 `SIGINT`/`SIGTERM` 清理钩子；
- 退出时销毁 SDK → `container.cleanup()` → 关闭输出流；
- 非 TTY 或非 text 格式会降级为 `--help`。

---

## 6. 核心设计原则

> **"统一执行层 + 单 SDK 实例 + 模式矩阵校验"**
> - 所有工作流执行都走 SDK，`ExecutionService` 是唯一入口
> - 终端（node-pty）只负责展示/输出，不参与执行
> - 不合法模式组合被自动检测并降级，保护运行时稳定性

设计收益：
1. **避免双 SDK 实例**：detached/background 复用 CLI 已初始化的同一 SDK 实例，规避了 SQLite/WAL 存储竞争；
2. **进程可干净退出**：统一 `shutdown()` 关闭 storage、销毁 SDK、关闭输出流；
3. **模式正交 + 安全校验**：应用层、输出层、工作流调度层三组维度仍保持正交，但通过 `validateModeCombination()` 确保每维度的交叉组合在运行时是安全的。

---

## 7. 模式组合矩阵（速查）

| 场景 | 应用模式 | 输出 | execution run 模式 | 命令示例 |
|---|---|---|---|---|
| 人工实时观察 | interactive | text | detached（默认） | `wf-agent execution run wf-1` |
| 当前终端跑完拿结果 | interactive | text | blocking | `wf-agent execution run wf-1 -b` |
| 不占窗口跑长任务 | interactive | text | background | `wf-agent execution run wf-1 --background` |
| 实时跟踪执行进展 | interactive/headless | text/json | follow | `wf-agent execution follow exec-123` |
| CI 解析结果 | headless | json | blocking（自动降级） | `CLI_MODE=headless wf-agent execution run wf-1` |
| 集成测试 | headless（`TEST_MODE=true`） | json | blocking（自动降级） | `TEST_MODE=true wf-agent execution run wf-1` |
| 程序调用 | programmatic | 结构化 | blocking（自动降级） | `CLI_MODE=programmatic wf-agent execution run wf-1` |
| 全屏交互 | interactive（`--tui`） | text | （TUI 内操作） | `wf-agent --tui` |

> 注意：headless + detached/background 会被自检机制自动降级为 blocking，运行时看到的总是 blocking。

---

## 8. Server 端执行模式

Server 端的 `apps/server/src/services/execution-service.ts` 已完成 SDK 集成，移除了所有 TODO 占位符：

| 方法 | 原实现 | 现实现 |
|---|---|---|
| `execute()` | 生成 mock ID `exec_${Date.now()}` | 使用 `ExecuteWorkflowCommand` |
| `pause()` | 仅更新本地缓存 | 使用 `PauseWorkflowCommand` + 本地更新 |
| `resume()` | 仅更新本地缓存 | 使用 `ResumeWorkflowCommand` + 本地更新 |
| `stop()` | 仅更新本地缓存 | 使用 `CancelWorkflowCommand` + 本地更新 |
| `list()` | 仅本地缓存 | 本地缓存 + 回退 `sdk.executions.getAll()` |
| `getStatus()` | 仅本地缓存 | 本地缓存 + 回退 `sdk.executions.get()` |

Server 端也共享 `packages/runtime/src/mode/detector.ts` 的检测逻辑，但模式组合校验仅属于 cli-app——Server 执行本身就是 blocking 模式，不存在 detached/background 的概念。

---

## 9. 关键文件索引

| 职责 | 路径 |
|---|---|
| CLI 入口 / 模式分发 / shutdown 兜底 | `apps/cli-app/src/index.ts` |
| 应用运行模式检测（含 configFallback） | `packages/runtime/src/mode/detector.ts`、`types.ts` |
| 模式检测薄封装 | `apps/cli-app/src/utils/mode-detector.ts` |
| 配置层 executionMode 字段 | `packages/runtime/src/config/types.ts`、`schema.ts` |
| 工作流执行模式命令（含 follow） | `apps/cli-app/src/commands/workflow-execution/index.ts` |
| 统一执行层（三模式 + 校验矩阵 + follow） | `apps/cli-app/src/services/execution/execution-service.ts` |
| 公共事件流引擎 | `apps/cli-app/src/services/execution/execution-service.ts`（`subscribeToExecutionEvents`） |
| 终端管理（node-pty / spawn） | `apps/cli-app/src/services/terminal/terminal-manager.ts` |
| 输出系统 / 格式预设 | `apps/cli-app/src/utils/output.ts`、`formatter.ts` |
| TUI 入口 | `apps/cli-app/src/tui/index.ts` |
| Server 执行服务（已 SDK 集成） | `apps/server/src/services/execution-service.ts` |
| 设计文档 | `apps/cli-app/docs/headless-mode-design.md`、`docs/modes-and-usage.md` |
