# wf-agent · cli-app 模式设置分析（修复后状态）

> 仓库：`https://github.com/kkkqkx123/wf-agent`
> 分析对象：`apps/cli-app`（包名 `@wf-agent/cli-app`，bin 名 `modular-agent` / `wf-agent`）
> 源码核对：`packages/runtime/src/mode/*`、`packages/types/src/execution/workflow-execution-mode.ts`、`apps/cli-app/src/index.ts`、`execution-service.ts`、`terminal-manager.ts`、`commands/workflow-execution/index.ts`

---

## 一、架构总览

cli-app 的模式设置**核心是一套"两层正交模型 + 组合校验矩阵"**：

- **第一层「应用运行模式」**（`ExecutionMode`）：`interactive` / `headless` / `programmatic`
  —— 由环境变量决定（配置文件 `executionMode` 字段作为 fallback）。
- **第二层「工作流执行模式」**（`WorkflowExecutionMode`）：`blocking` / `foreground` / `background`
  —— 仅作用于 `modular-agent execution run <workflow-id>` 子命令，决定单次工作流如何调度。
- **校验矩阵** `validateModeCombination()`：不合法组合（如 headless + foreground）自动降级为 `blocking` 并告警。

**设计方向合理**：分层清晰、单 SDK 实例、运行时保护。

---

## 二、模式设置的实际架构

### 2.1 第一层：应用运行模式（检测机制）

检测逻辑集中在 `packages/runtime/src/mode/detector.ts`（已从 cli-app 上提到 runtime，消除 cli-app/server 重复），cli-app 通过 `apps/cli-app/src/utils/mode-detector.ts` 薄封装复用。

**检测优先级（源码 `detectMode()`，`detector.ts:52`）：**

```
环境变量 (CLI_MODE / HEADLESS / TEST_MODE)     ← 最高
AppConfig.executionMode（配置文件 fallback）    ← 已接入（见 §3.2）
硬编码默认 "interactive"                       ← 最低
```

| 环境变量 | 取值 | 效果 |
|---|---|---|
| `CLI_MODE` | `programmatic` / `headless` | 强制应用模式 |
| `HEADLESS` | `true` | 兼容旧版 → headless |
| `TEST_MODE` | `true` | 兼容旧版 → headless（集成测试用）|
| `CLI_OUTPUT_FORMAT` | `json` / `silent` / `text` | 覆盖输出格式 |
| `NO_COLOR` | 任意 | 禁用 ANSI 颜色 |

**派生结果**（`ModeDetectionResult`，`detector.ts:26`）：`mode` + `outputFormat`（headless 默认 `json`）+ `colorEnabled`（跟随 `stdout.isTTY`）+ 三个快捷布尔 `isHeadless/isProgrammatic/isInteractive`。结果带缓存（`cachedResult` + `invalidateModeCache()`），便于测试。

### 2.2 第二层：工作流执行模式

仅作用于 `modular-agent execution run`，由 flag 决定：

| 模式 | Flag | 执行位置 | 终端 | 阻塞？ | 生命周期 |
|---|---|---|---|---|---|
| **foreground**（默认）| 无 flag | CLI 同进程（node-pty 伪终端展示事件）| node-pty 前台伪终端 | 否 | 随 CLI 退出而终止 |
| **background** | `--background` | CLI 同进程，输出写日志 | 无终端，日志写文件 | 否 | OS 级分离可存活 |
| **blocking** | `-b` / `--blocking` | CLI 同进程 | 当前终端 | 是（await 完成）| 同步等待 |

所有模式走统一入口 `ExecutionService.execute()`，再分派到 `executeForeground/executeBackground/executeBlocking`。

### 2.3 组合校验矩阵（`validateModeCombination`）

```
  app mode ↓  \  exec mode → | foreground(默认) | blocking | background
  ---------------------------|-------------------|----------|------------
  interactive                | ✓                 | ✓        | ✓
  headless                   | ⚠→blocking        | ✓        | ⚠→blocking
  programmatic               | ⚠→blocking        | ✓        | ✓
```

降级均走 `output.warnLog()` 告警。设计意图合理：headless 下不创建 node-pty/日志文件，避免崩溃与输出混乱。

### 2.4 TUI 入口分发

无子命令启动时，若 `interactive + 无参 + TTY + text` 则进入全屏 TUI（`startTUI()`）；否则降级为 `--help`；headless 无参则调用 `ExitManager.exit()` 清空输出后退出。

### 2.5 正交维度（非"模式"）

以下概念与两层模型正交，不属模式范畴：
- **输出格式**（text/json/silent）、**日志级别**、**存储类型**（json/sqlite/memory）是横切维度；
- **Agent Loop 的 sync/stream/async** 是命令级便利封装（`agent run` / `--stream` / `agent start`），未抽象成"模式"对象。

---

## 三、已修复的问题

以下问题均已在本次修复中解决：

### ✅ P0-1：`executionMode` 配置文件字段接入 `getMode()`

**原问题**：schema 和类型均定义了 `executionMode` 字段，但运行时从未传入 `getMode()`，配置文件永不生效。

**修复内容**（`apps/cli-app/src/index.ts`）：
- 添加模块级变量 `configExecutionMode`
- 在 `preAction` hook 中加载 config 后赋值：`configExecutionMode = config.executionMode`
- `getMode(configExecutionMode)` 调用时传入该值作为 fallback

**效果**：配置文件的 `executionMode = "headless"` 现在可生效（环境变量仍优先）。

### ✅ P0-2：`Command.prototype.action` monkey-patch 替换为 `postAction` hook

**原问题**：全局修改 `Command.prototype.action` 的 monkey-patch，强制调用 `process.exit()` 可能截断异步输出。

**修复内容**（`apps/cli-app/src/index.ts`）：
- 移除 `Command.prototype.action` 覆写（全局副作用）
- 改用 Commander 原生 `program.hook('postAction', ...)`
- 移除强制 `process.exit()`，改为：
  - headless 模式：调用 `ExitManager.exit()` 清空输出后再退出
  - interactive 模式：仅设置 `process.exitCode`，自然结束

**效果**：无全局副作用、输出不会因 `process.exit()` 被截断、Interactive 模式下异步任务可继续执行。

### ✅ P0-3：100ms 魔法延迟移除

**原问题**：headless 无子命令时使用 `setTimeout(() => ExitManager.exit(0), 100)`。

**修复内容**：直接调用 `ExitManager.exit(0)`，利用其内部的 `ensureDrained()` 实现正确退出时机判断。

### ✅ P1-1："detached" 已全面替换为 "foreground"

**原问题**：`detached` 实际在同进程运行（非 OS 分离），命名严重误导。

**修复内容**：
- 类型 `WorkflowExecutionMode` 已彻底删除 `'detached'`，仅保留 `'foreground'`（`packages/types/src/execution/workflow-execution-mode.ts`）
- 方法 `executeDetached()` → `executeForeground()`（`execution-service.ts`）
- 默认模式从 `'detached'` 改为 `'foreground'`（`execution-service.ts`、`workflow-execution/index.ts`）
- 所有用户面消息更新为 "foreground"
- **无向后兼容**：`'detached'` 字符串在代码中已全部删除

### ✅ P2-1：background 日志路径统一

**原问题**：`execution-service.ts` 使用 `logs/workflow-${executionId}.log`，`terminal-manager.ts` 默认使用 `logs/task-${sessionId}.log`，不一致。

**修复内容**：`terminal-manager.ts:118` 默认值从 `logs/task-${sessionId}.log` 改为 `logs/workflow-${sessionId}.log`。

---

## 四、仍存在的设计张力

以下问题未在本次修复中处理，属已知设计权衡，记录以供后续评估：

| 问题 | 说明 | 建议 |
|---|---|---|
| programmatic 模式定位模糊 | `programmatic` 与 `headless` 行为接近（差异仅在 background 是否被降级）。无专用 API/返回通道。 | 若等价于 headless 则合并，若需真实编程接口则补 SDK 返回通道 |
| TEST_MODE 与 HEADLESS 等价 | 两者均映射到 headless，`TEST_MODE=true` 会触发全部 foreground/background 降级 | 明确文档标注，避免运维误用 |
| foreground/background 异步存活限制 | postAction hook 在命令 action 完成后仍会调用 `shutdown()` 销毁 SDK，同进程的 foreground/background 模式无法脱离 CLI 存活 | 仅 `--background` 的 OS 级子进程能真正存活；文档已标注"foreground 随 CLI 退出" |
| OutputManager 未实现 | `headless-mode-design.md` 中提议的 `OutputManager` 统一输出流管理器尚未实现 | 当需要统一 SDK 日志与命令输出时再实现 |
| 概念泛滥 | 多份文档把输出/日志/存储/Agent Loop 全部列为"模式" | 从"模式"分类中剥离，称为"输出维度/命令选项" |

---

## 五、文档 vs 实现一致性矩阵（修复后）

| 文档声称 | 实现情况 | 判定 |
|---|---|---|
| 应用模式 interactive/headless/programmatic | 已落地（`detector.ts`）| ✅ 一致 |
| 工作流模式 blocking/foreground/background | 已落地（`execution-service.ts`）| ✅ 一致 |
| headless 下 foreground/background 降级 blocking | 已落地（`validateModeCombination`）| ✅ 一致 |
| 配置文件 `executionMode` 可作 fallback | `configExecutionMode` 已传入 `getMode()` | ✅ 已修复 |
| 'detached' 已全部删除，'foreground' 为正式名 | 类型 + 内部代码 + 用户消息均已替换 | ✅ 已修复 |
| 退出机制使用 Commander postAction hook | postAction hook 替代了 monkey-patch | ✅ 已修复 |
| background 日志路径统一 | `terminal-manager.ts` 默认路径已改为 `logs/workflow-*` | ✅ 已修复 |
| 100ms 魔法延迟消除 | 使用 `ExitManager.exit()` 基于 drain 退出 | ✅ 已修复 |
| 不存在 `thread run` 命令 | CLI 真实命令为 `workflow-execution run` | ⚠️ 文档需更新（非代码问题） |
| programmatic 提供结构化调用接口 | 仅环境变量开关，无专用 API | ⚠️ 未实现 |
| OutputManager 统一输出流管理器 | 仅存在于设计草稿 | ⚠️ 未实现 |

---

## 六、关键文件索引

| 职责 | 路径 |
|---|---|
| 工作流执行模式类型定义（含 foreground）| `packages/types/src/execution/workflow-execution-mode.ts` |
| 应用模式检测（含缓存、configFallback）| `packages/runtime/src/mode/detector.ts`、`types.ts` |
| 模式检测薄封装 | `apps/cli-app/src/utils/mode-detector.ts` |
| CLI 入口 / postAction hook / TUI 分发 / shutdown | `apps/cli-app/src/index.ts` |
| 工作流执行模式 + 校验矩阵 + follow | `apps/cli-app/src/services/execution/execution-service.ts` |
| 终端管理（node-pty 前台 / spawn 后台）| `apps/cli-app/src/services/terminal/terminal-manager.ts` |
| 执行命令 flag（`-b` / `--background`）| `apps/cli-app/src/commands/workflow-execution/index.ts` |
| 配置层 executionMode 字段 | `packages/runtime/src/config/schema.ts:31`、`types.ts:57` |
| ExitManager（异步 drain + 退出）| `apps/cli-app/src/utils/exit-manager.ts` |
| 命令选项类型定义 | `apps/cli-app/src/types/cli-types.ts` |
| 模式与用法文档（待整合） | `apps/cli-app/docs/modes-and-usage.md` |
| 无头模式设计草稿 | `apps/cli-app/docs/headless-mode-design.md` |
| 执行模式分析（前置版本）| `apps/cli-app/docs/‎cli-app-execution-mode-analysis.md` |
| 测试模式策略 | `apps/cli-app/docs/tests/test-mode-analysis.md` |
