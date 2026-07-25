# CLI App 测试模式分析与测试策略

## 概述

本文档分析了 cli-app 的测试模式配置、执行流程，以及最新重构（模式组合校验、公共事件流引擎、execution follow 等）对测试策略的影响。帮助开发者正确编写和执行单元测试、集成测试和类型测试。

---

## 1. 测试执行流程

### 1.1 测试入口

```bash
# 运行集成测试
cd apps/cli-app && pnpm test __tests__/integration/workflows/01-registration.test.ts

# 运行单元测试（与源码同目录）
cd apps/cli-app && pnpm test __tests__/services/execution/execution-service.test.ts

# 运行类型测试
cd apps/cli-app && pnpm test:type
```

### 1.2 CLIRunner 执行

`CLIRunner` 类（`__tests__/utils/cli-runner.ts`）通过 spawn 子进程运行 CLI：

```typescript
const child = spawn("node", [this.cliPath, ...args], {
  env: { ...this.defaultEnv, ...options.env },
  cwd: options.cwd,
  stdio: ["pipe", "pipe", "pipe"],
});
```

测试环境变量设置：

```typescript
this.defaultEnv = {
  NODE_ENV: "test",
  TEST_MODE: "true",            // 触发 headless 模式
  LOG_DIR: this.outputDir,
  DISABLE_LOG_TERMINAL: "true",
  DISABLE_SDK_LOGS: "true",
  SDK_LOG_LEVEL: "silent",
};
```

### 1.3 CLI 初始化流程（preAction hook）

```
CLIRunner.spawn()
  │
  ▼
子进程启动 → Commander preAction hook:
  1. 加载配置文件（ConfigLoader）
  2. 初始化 Output（CLIOutput）
  3. 初始化 Logger（含 SDK Logger）
  4. 初始化 Storage Manager（STORAGE_DIR）
  5. 初始化 Container + SDK
  │
  ▼
getMode() 检测无头模式 → TEST_MODE=true → headless
  │
  ▼
validateModeCombination() → headless 下 detached/background 自动降级为 blocking
  │
  ▼
命令执行 → shutdown() → 进程退出
```

---

## 2. 当前模式架构与测试关键点

### 2.1 两层模型回顾

```
应用运行模式（interactive / headless / programmatic）── 由环境变量或 config 决定
工作流执行模式（detached / background / blocking）   ── 由 execution run 的 flag 决定
```

**测试关键点**：测试运行在 `headless` 模式下，因此所有 `execution run` 命令如果使用 `detached` 或 `background` 模式，会被 `validateModeCombination()` 自动降级为 `blocking`。

### 2.2 新增/修改的测试面

| 变更 | 需要测试的内容 | 推荐测试类型 |
|---|---|---|
| `validateModeCombination()` | 6 种模式组合的校验逻辑 | 单元测试（直接调用私有方法） |
| `followExecution()` | 事件订阅、Promise resolve | 单元测试（mock SDK） |
| `subscribeToExecutionEvents()` | 三种事件订阅 + 清理 | 单元测试（mock createExecutionScopedSubscription） |
| `configFallback` 参数 | `detectMode(configFallback)` | 单元测试（runtime 包） |
| Server ExecutionService SDK 集成 | execute/pause/resume/stop 调用 SDK 命令 | 集成测试（需要 mock 或 real SDK） |

---

## 3. 测试策略

### 3.1 测试金字塔

```
          ╱╲
         ╱  ╲          集成测试（CLIRunner + 子进程）
        ╱    ╲         - 完整工作流执行
       ╱      ╲        - 模式降级头到尾
      ╱────────╲
     ╱          ╲     单元测试（直接调用 + mock）
    ╱            ╲    - validateModeCombination()
   ╱              ╲   - followExecution()
  ╱────────────────╲  - subscribeToExecutionEvents()
 ╱                  ╲ - detectMode() + configFallback
╱════════════════════╲
```

### 3.2 单元测试策略

#### 3.2.1 ExecutionService 的模式校验

**测试目标**：`validateModeCombination()` 方法

```typescript
// __tests__/services/execution/execution-service.test.ts
// 需要 mock getMode() 的返回值

describe("validateModeCombination", () => {
  // Interactive mode: all modes pass through
  it("should pass through all modes in interactive mode", () => {
    mockGetMode({ isInteractive: true, isHeadless: false, isProgrammatic: false });
    expect(validateModeCombination("detached")).toBe("detached");
    expect(validateModeCombination("blocking")).toBe("blocking");
    expect(validateModeCombination("background")).toBe("background");
  });

  // Headless mode: detached and background downgrade to blocking
  it("should downgrade detached to blocking in headless mode", () => {
    mockGetMode({ isInteractive: false, isHeadless: true, isProgrammatic: false });
    expect(validateModeCombination("detached")).toBe("blocking");
  });

  it("should downgrade background to blocking in headless mode", () => {
    mockGetMode({ isInteractive: false, isHeadless: true, isProgrammatic: false });
    expect(validateModeCombination("background")).toBe("blocking");
  });

  it("should keep blocking in headless mode", () => {
    mockGetMode({ isInteractive: false, isHeadless: true, isProgrammatic: false });
    expect(validateModeCombination("blocking")).toBe("blocking");
  });

  // Programmatic mode: detached downgrades to blocking
  it("should downgrade detached to blocking in programmatic mode", () => {
    mockGetMode({ isInteractive: false, isHeadless: false, isProgrammatic: true });
    expect(validateModeCombination("detached")).toBe("blocking");
  });

  it("should pass through blocking and background in programmatic mode", () => {
    mockGetMode({ isInteractive: false, isHeadless: false, isProgrammatic: true });
    expect(validateModeCombination("blocking")).toBe("blocking");
    expect(validateModeCombination("background")).toBe("background");
  });
});
```

#### 3.2.2 followExecution

**测试目标**：`followExecution()` 方法

```typescript
describe("followExecution", () => {
  it("should subscribe to 4 event types", async () => {
    // mock sdk.getFactory().getDependencies()
    // mock createExecutionScopedSubscription
    const unsubscribe = vi.fn();
    mockCreateScopedSub.mockReturnValue({ subscribe: () => unsubscribe });

    const promise = service.followExecution("exec-123");

    // 验证 4 个订阅被创建（NODE_STARTED, NODE_COMPLETED,
    //   WORKFLOW_EXECUTION_COMPLETED, WORKFLOW_EXECUTION_FAILED）
    expect(mockCreateScopedSub).toHaveBeenCalledTimes(4);

    // 触发完成事件，验证 resolve
    const completedHandler = findSubscription("WORKFLOW_EXECUTION_COMPLETED");
    completedHandler({ executionTime: 5000 });
    await expect(promise).resolves.toBeUndefined();
  });

  it("should cleanup subscriptions on completion", async () => {
    const unsubscribe = vi.fn();
    mockCreateScopedSub.mockReturnValue({ subscribe: () => unsubscribe });

    const promise = service.followExecution("exec-123");

    const completedHandler = findSubscription("WORKFLOW_EXECUTION_COMPLETED");
    completedHandler({ executionTime: 5000 });
    await promise;

    expect(unsubscribe).toHaveBeenCalled();
  });
});
```

#### 3.2.3 Runtime mode detector（configFallback）

**测试目标**：`packages/runtime/src/mode/detector.ts` 中的 `getMode(configFallback?)`

```typescript
// 测试文件：packages/runtime/src/mode/__tests__/detector.test.ts

describe("getMode with configFallback", () => {
  beforeEach(() => {
    delete process.env.CLI_MODE;
    delete process.env.HEADLESS;
    delete process.env.TEST_MODE;
    invalidateModeCache();
  });

  it("should prioritize env var over configFallback", () => {
    process.env.CLI_MODE = "headless";
    const result = getMode("interactive");
    expect(result.mode).toBe("headless");
  });

  it("should use configFallback when no env var is set", () => {
    const result = getMode("programmatic");
    expect(result.mode).toBe("programmatic");
  });

  it("should default to interactive when neither env nor configFallback is set", () => {
    const result = getMode();
    expect(result.mode).toBe("interactive");
  });

  it("should ignore configFallback 'interactive' (default)", () => {
    const result = getMode("interactive");
    expect(result.mode).toBe("interactive");
  });
});
```

### 3.3 集成测试策略

#### 3.3.1 模式降级验证（通过 CLIRunner）

```typescript
// __tests__/integration/modes/validate-mode-combination.test.ts

describe("Mode combination validation (integration)", () => {
  it("should auto-downgrade detached to blocking in headless (TEST_MODE=true)", async () => {
    const runner = new CLIRunner(undefined, testOutputDir);
    runner.setStorageDir(helper.getStorageDir());

    // 使用 TEST_MODE=true（headless）+ detached（默认）
    // 预期：CLI 自动降级为 blocking，输出 JSON 结果而非终端信息
    const { stdout, exitCode } = await runner.run(
      ["execution", "run", testWorkflowId, "-i", '{"test": true}'],
      { env: { SKIP_BLOCKING_MODE: "false" } }
    );

    expect(exitCode).toBe(0);
    // stdout 应包含 blocking 执行的 json 结果
    // 不应包含 "terminal" / "detached" 等关键词
    expect(stdout).not.toContain("terminal");
    expect(stdout).toContain("executionId");
  });

  it("should output warning when downgrading", async () => {
    const runner = new CLIRunner(undefined, testOutputDir);
    runner.setStorageDir(helper.getStorageDir());

    const { stderr, exitCode } = await runner.run(
      ["execution", "run", testWorkflowId, "--background"],
    );

    expect(exitCode).toBe(0);
    // stderr 应包含模式降级告警
    expect(stderr).toContain("Falling back to 'blocking'");
  });
});
```

#### 3.3.2 execution follow 验证

```typescript
describe("execution follow (integration)", () => {
  it("should stream events for a running execution", async () => {
    const runner = new CLIRunner(undefined, testOutputDir);
    runner.setStorageDir(helper.getStorageDir());

    // 先启动一个 blocking 执行
    const { stdout } = await runner.run(
      ["execution", "run", testWorkflowId, "-b", "-i", '{"test": true}'],
    );
    const executionId = JSON.parse(stdout).executionId;

    // 再用 follow 命令跟踪
    const { stdout: followStdout, exitCode } = await runner.run(
      ["execution", "follow", executionId],
    );

    expect(exitCode).toBe(0);
    expect(followStdout).toContain("Node started");
    expect(followStdout).toContain("Node completed");
    expect(followStdout).toContain("execution completed");
  });
});
```

### 3.4 类型测试（tsd）

类型测试应验证 `execution-service.ts` 导出的公共 API 类型正确性：

```typescript
// __tests__/test-d/services/execution-service.test-d.ts

import { expectType, expectError } from "tsd";
import { ExecutionService, ExecutionResult } from "../../../src/services/execution/index.js";
import type { WorkflowExecutionMode } from "@wf-agent/types";

// ExecutionResult.mode 应该是 WorkflowExecutionMode，不再是本地的 ExecutionMode
expectType<WorkflowExecutionMode>({} as ExecutionResult["mode"]);

// ExecutionService.execute() 接受 WorkflowExecutionMode 参数
const service = {} as ExecutionService;
expectType<Promise<ExecutionResult>>(
  service.execute("wf-1", {}, "blocking")
);
```

---

## 4. 关键测试覆盖矩阵

| 测试类别 | 测试对象 | 覆盖内容 | 优先级 |
|---|---|---|---|
| 单元 | `validateModeCombination()` | 6 种组合 + 3 种降级 | P0 |
| 单元 | `followExecution()` | 事件订阅、Promise resolve、cleanup | P0 |
| 单元 | `getMode(configFallback)` | 环境变量 vs config 优先级 | P0 |
| 单元 | `subscribeToExecutionEvents()` | 三种终端类型适配 | P1 |
| 单元 | `toExecutionDetails()` | WorkflowExecution → ExecutionDetails 转换 | P1 |
| 集成 | 模式降级 | headless + detached → blocking 全过程 | P0 |
| 集成 | `execution follow` | 完整的事件流输出 | P1 |
| 集成 | background 模式 | 日志文件写入 + pid 返回 | P1 |
| 集成 | Server SDK 命令 | execute / pause / resume / stop | P1 |
| 类型 | 公共 API 类型 | ExecutionService 导出的类型签名 | P1 |

---

## 5. 调试测试失败

### 5.1 启用调试输出

在 `CLIRunner.defaultEnv` 中：

```typescript
SDK_LOG_LEVEL: "debug",        // 关闭静默
DISABLE_SDK_LOGS: "false",     // 允许 SDK 日志
```

### 5.2 检查输出文件

测试命令输出保存到：

```
__tests__/outputs/<subdir>/<number>_<command>.log
```

### 5.3 常见失败原因

| 退出码 | 原因 | 排查方向 |
|---|---|---|
| 0（但无输出） | 模式降级改变了输出格式 | 检查 stderr 是否有告警 |
| 1 | 命令失败或未捕获异常 | 检查 error-handler.ts 日志 |
| 2 | 验证错误 | 参数格式/必填项检查 |
| 3 | 文件操作错误 | STORAGE_DIR 隔离是否成功 |
| 非零退出 | SDK 初始化失败 | 检查 TOML parser 是否可用 |

---

## 6. 测试隔离最佳实践

### 6.1 环境变量隔离

| 变量 | 用途 | 推荐测试值 | 注意 |
|---|---|---|---|
| `TEST_MODE` | 触发 headless + 模式降级 | `true` | 会使 detached/background 静默降级为 blocking |
| `CLI_MODE` | 覆盖应用运行模式 | 按需（测试降级时设 `headless`） | 比 `TEST_MODE` 优先级高 |
| `STORAGE_DIR` | 存储目录 | 每个测试唯一的临时目录 | 防 SQLite WAL 文件冲突 |
| `DISABLE_SDK_LOGS` | 关闭 SDK 日志 | `true` | 减少测试输出噪音 |
| `CLI_OUTPUT_FORMAT` | 输出格式 | 集成测试建议 `json` | 便于断言 |

### 6.2 关键注意事项

1. **模式降级对测试的影响**：如果测试在 `TEST_MODE=true` 下使用默认的 `detached` 模式执行 `execution run`，结果会是 `blocking` 而不是 `detached`。测试断言需要预期 blocking 的输出格式。

2. **follow 命令阻塞特性**：`execution follow` 返回的 Promise 会等待执行完成/失败才 resolve。集成测试需要确保被跟踪的执行最终会完成。

3. **run 的 blocking 模式下进程不退出**：`wf-agent execution run wf-1 -b` 在 headless 下会等到工作流结束后才退出进程。CLIRunner.run() 默认会等待子进程结束，如果工作流执行时间长可能导致超时。

4. **detached 模式的进程内特性**：detached 模式的工作流与 CLI 在同一个进程中。如果测试通过 CLIRunner 启动 CLI 并立即 kill 子进程，detached 的工作流也会被终止。

---

## 7. 已有的单元测试检查清单

| 已有测试 | 路径 | 状态 |
|---|---|---|
| mode detector 基础测试 | `packages/runtime/src/mode/__tests__/` | 需要补充 configFallback 测试 |
| ExecutionService | `apps/cli-app/__tests__/services/execution/` | 需要补充 validateModeCombination 测试 |
| commands/workflow-execution | `apps/cli-app/__tests__/commands/` | 需要补充 follow 命令测试 |
| CLIRunner | `apps/cli-app/__tests__/utils/cli-runner.test.ts` | 已有，需要验证模式降级 |
| Server ExecutionService | `apps/server/__tests__/services/` | 需要补充 SDK 命令测试 |

---

## 8. 参考

| 文件 | 路径 |
|---|---|
| 执行模式分析（最新） | `apps/cli-app/docs/‎cli-app-execution-mode-analysis.md` |
| CLI 入口 / shutdown | `apps/cli-app/src/index.ts` |
| 模式检测 | `packages/runtime/src/mode/detector.ts` |
| 配置层 executionMode | `packages/runtime/src/config/types.ts`、`schema.ts` |
| ExecutionService（三模式 + 校验 + follow） | `apps/cli-app/src/services/execution/execution-service.ts` |
| 公共事件流引擎 | `apps/cli-app/src/services/execution/execution-service.ts` |
| CLI 模式命令 | `apps/cli-app/src/commands/workflow-execution/index.ts` |
| CLIRunner | `apps/cli-app/__tests__/utils/cli-runner.ts` |
| WorkflowTestHelper | `apps/cli-app/__tests__/utils/workflow-test-helper.ts` |
| Server ExecutionService | `apps/server/src/services/execution-service.ts` |
