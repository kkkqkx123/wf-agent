# 交互式脚本集成分析 - 有状态节点设计 / 状态化工具 / 接收策略

## 1. 问题域分析

交互式脚本的核心挑战：**脚本执行过程中需要在任意时刻暂停并等待外部输入**，这个输入可能来自人类（通过 UserInteraction），也可能来自 LLM（自动决策），或者混合（LLM 推荐+用户确认）。

### 1.1 交互式脚本的典型场景

| 场景 | 说明 | 输入来源 | 状态需求 |
|------|------|---------|---------|
| 部署确认 | `继续部署到生产环境？(y/n)` | 用户 | 轻量，单次交互 |
| 参数调整 | `请输入数据库连接字符串：` | 用户 | 轻量，单次交互 |
| LLM 辅助决策 | 分析日志后，LLM 决定下一步操作 | LLM | 轻量，单次交互 |
| 混合审批 | LLM 建议参数值，用户确认或修改 | LLM + 用户 | 中量，2~3轮 |
| 多轮交互 | 交互式 CLI 工具（`npm init`, `ssh-keygen` 等） | 用户 | 重量，多轮循环 |
| 长期运行脚本 | `docker build`, `npm run test -- --watch` | 流式监控 | 重量，持续状态 |
| 分步脚本 | 部署流水线：build→test→deploy，每步都需要确认 | 用户/LLM | 中量，步骤间状态 |

### 1.2 现有系统能力分析

| 现有组件 | 能力 | 局限 |
|---------|------|------|
| **USER_INTERACTION 节点** | 支持 `UPDATE_VARIABLES` / `ADD_MESSAGE` | 纯 UI 交互，无脚本执行上下文 |
| **UserInteractionHandler** | 通用交互处理接口，支持超时/取消 | 操作类型固定，无法嵌入脚本执行流程 |
| **HumanRelayClient** | 替代 LLM API，从人类获取响应 | 只用于 LLM 替换，不关联脚本 |
| **TerminalService** | 支持 session 管理和命令执行 | 无交互式输入管理，无 PTY 支持 |
| **ScriptExecutor** | 执行一次性脚本 | 无状态，不支持交互式输入 |
| **backend-shell 工具** | 后台 Shell 执行 + 输出轮询 | 只支持后台执行，无 PTY 交互 |
| **AGENT_LOOP 节点** | 有状态循环 + Entity 模式 | 专为 LLM 对话设计，不匹配脚本场景 |
| **SUBGRAPH 节点** | 占位符 + 独立执行实体 | 运行时创建子实体，无状态保持需求 |
| **LOOP 节点** | 循环执行子图 | 无状态，每次迭代重新执行 |

---

## 2. 有状态节点设计深度分析

本章分析三种有状态节点设计路线的优缺点，以及各自适合的场景。

### 2.1 现有模式对比：AGENT_LOOP vs SUBGRAPH 的设计哲学

| 维度 | AGENT_LOOP | SUBGRAPH |
|------|-----------|----------|
| **设计模式** | 运行时创建 `AgentLoopEntity`（有状态实体） | 预编译展开为子图 OR 运行时创建子执行实体 |
| **状态存储** | `AgentLoopState`（序列化到 Checkpoint） | 子 `WorkflowExecutionEntity` 的 VariableManager |
| **生命周期** | `CREATED→RUNNING→PAUSED→RUNNING→COMPLETED` | 子工作流执行完成即销毁 |
| **暂停/恢复** | 支持（通过 Checkpoint + Interruption） | 不支持（执行完毕后清理） |
| **状态粒度** | 细粒度（iteration, toolCall, streaming 等） | 粗粒度（仅变量状态） |
| **执行模型** | 迭代型（LLM 驱动的循环） | 一次性执行 |
| **与父级关系** | 独立实体，通过 HierarchyManager 注册 | 独立实体，但有变量映射契约 |
| **Checkpoint 策略** | 状态 + 消息分离序列化 | 父工作流整体 Checkpoint 时包含子实体状态 |

**关键发现**：
- AGENT_LOOP 的 Entity 模式是"**自包含的状态容器**"：Entity 内聚了 config（不可变）+ state（可变，可序列化）+ managers（运行时）
- SUBGRAPH 的模式是"**接口隔离的执行单元**"：通过 variableInputs/variableOutputs 定义清晰边界，执行完毕后结果通过映射返回
- 两者都使用 HierarchyManager 统一管理父子关系，这与交互式脚本的需求一致

### 2.2 交互式脚本的有状态节点设计路线

#### 路线 A：专用有状态节点（Dedicated Stateful Node）

**设计**：类似于 AGENT_LOOP 的 Entity 模式，创建一个 `InteractiveScriptSessionEntity`

```
INTERACTIVE_SCRIPT entity
  ├── Config (不可变):
  │   ├── scriptName / template
  │   ├── interactionMode (blocking / llm-assisted / hybrid)
  │   ├── executorType (shared / pty)
  │   ├── shell, cwd, env
  │   └── promptPatterns, maxRounds
  │
  ├── State (可变, 可序列化到 Checkpoint):
  │   ├── sessionStatus: pending → running → waiting_input → running → ... → completed
  │   ├── currentCommand: 当前正在执行的命令
  │   ├── executedCommands: 已执行的命令列表
  │   ├── accumulatedStdout / accumulatedStderr
  │   ├── interactionHistory: 交互记录列表
  │   ├── completedRounds, maxRounds
  │   └── waitingForInput: boolean, currentPrompt: string | null
  │
  └── Runtime Managers (不可序列化):
      ├── terminalSession: 关联的 TerminalService session
      ├── outputBuffer: 输出缓冲（用于增量读取）
      └── promptDetector: 交互提示检测器
```

**优点**：
- 完整的状态管理（暂停/恢复/Checkpoint）
- 嵌套在 LOOP 内部时可跨迭代保持 Shell 状态
- 生命周期清晰，与现有 AGENT_LOOP 架构一致

**缺点**：
- 引入新的节点类型 + Entity 类 + Handler + Coordinator
- 需要新的 Checkpoint 序列化逻辑
- Shell 会话的序列化/恢复较复杂（无法序列化进程，只能重建）

#### 路线 B：内嵌节点 + 运行时展开为 LOOP（Embedded + Loop Expansion）

**设计**：在 Workflow 配置中使用嵌入式节点组合，预处理器将其展开为一个 LOOP 结构

```
# 配置中的嵌入式节点定义
[nodes.step-by-step-deploy]
type = "INTERACTIVE_SCRIPT"  # 静态配置中的占位符

# 预处理器展开为:
# LOOP_START (iterations: 1, condition: "interactive_script.status !== 'completed'")
#   ├── [路由] 检测 run_or_wait 状态
#   │   ├── 需要执行命令 → SCRIPT 节点 (execute command)
#   │   └── 正在等待输入 → USER_INTERACTION 节点 (get input)
#   └── [条件判断] 是否完成
# LOOP_END
```

**核心问题**：展开为 LOOP 后，**每次迭代执行的是独立的 SCRIPT 命令**，无法保持 Shell 会话状态。比如：
- 第 1 次迭代执行 `cd /var/www`
- 第 2 次迭代执行 `./deploy.sh` → 工作目录已丢失
- 无法处理 `ssh` 等需要多轮交互的会话式命令

**因此该方案需要配合状态化工具使用**（见第 3 章）：

```
# 改进方案：Embedded + stateful backend-shell tool
LOOP_START
  ├── TOOL_CALL: backend_shell (command: "cd /var/www")
  │   └── 返回 shell_id
  │
  ├── TOOL_CALL: backend_shell (command: "./deploy.sh")
  │   └── 复用同一个 shell_id
  │
  ├── [检测到需要输入]
  │   ├── TOOL_CALL: shell_output (获取当前终端输出)
  │   ├── USER_INTERACTION: 显示输出 + 获取用户输入
  │   └── TOOL_CALL: backend_shell (发送输入)
  │
  └── [条件判断] 是否完成
LOOP_END
```

**优点**：
- 不引入新节点类型，复用已有 SCRIPT / USER_INTERACTION / TOOL 节点
- 配置灵活，用户可以自由编排交互逻辑
- 复用已有的 backend-shell 状态化工具

**缺点**：
- 编排复杂度高，用户需要手动管理 shell_id 在循环中的传递
- 无法进行 Checkpoint 暂停/恢复（循环没有持久化 Shell 状态）
- 对多轮交互式 CLI 工具不友好（需要用户手动检测提示 + 发送输入）
- 长期运行脚本的输出累积在 TerminalService 的内存中，无状态管理

#### 路线 C：混合方案（推荐）

**设计**：核心交互式脚本使用**专用有状态节点**（路线 A），简单的分步执行使用**嵌入式展开**（路线 B）

```
# 场景 1：交互式 CLI（ssh-keygen, npm init 等）
# → 使用 INTERACTIVE_SCRIPT 有状态节点
[nodes.generate-ssh]
type = "INTERACTIVE_SCRIPT"
config.template = "ssh-keygen -t ed25519"
config.interactionMode = "blocking"
config.executorType = "pty"
config.maxRounds = 10

# 场景 2：分步部署（每步独立的脚本，步骤间需要确认）
# → 使用内嵌节点展开为 LOOP（无需新节点类型）
[nodes.deploy-pipeline]
type = "INTERACTIVE_SCRIPT"  # 静态占位符
config.steps = [
  { command = "npm run build", requiresInput = false },
  { command = "npm test", requiresInput = false },
  { command = "npm run deploy", confirmMessage = "确认部署到生产环境？" },
  { command = "npm run verify", requiresInput = false },
]
```

**关键原则**：
- **PTY + 多轮交互场景** → 需要真正的有状态节点（Entity 模式）
- **分步无状态脚本 + 步骤间确认** → 嵌入式展开为 LOOP 即可
- **长期运行脚本 + 流式日志监控** → 需要状态化工具 + 接收策略（见第 4 章）

### 2.3 决策矩阵

| 场景 | 路线 A（专用节点） | 路线 B（展开为 LOOP） | 路线 C（混合） |
|------|:---:|:---:|:---:|
| 多轮交互式 CLI（ssh-keygen, passwd） | ✅ 必需 | ❌ 无法处理 | ✅ 专用节点处理 |
| 单次交互（确认部署） | ✅ 可用 | ✅ 可用 | ✅ 嵌入式展开 |
| 分步流水线（build→test→deploy） | ⚠️ 过度设计 | ✅ 天然适合 | ✅ 嵌入式展开 |
| 长期运行脚本 + 流式监控 | ✅ 可管理 | ❌ 无输出管理 | ✅ 专用节点处理 |
| 嵌套在 LOOP 内部分批处理 | ✅ 跨迭代保持状态 | ❌ 重新执行 | ✅ 专用节点处理 |
| Checkpoint 暂停/恢复 | ✅ 完整支持 | ❌ 不支持 | ✅ 专用节点支持 |

**最终推荐**：采用路线 C。以路线 A（专用有状态节点）为核心，对简单分步执行场景支持嵌入式展开模式。

---

## 3. 状态化脚本工具分析

### 3.1 现有 backend-shell 工具架构

```
backend_shell tool           → 启动后台命令，返回 shell_id
shell_output tool             → 轮询获取 shell 输出
shell_kill tool               → 终止 shell 会话
                    ↓
          TerminalService
          ├── getOrCreateSession()  → 会话复用
          ├── startBackgroundCommand() → 后台执行
          ├── getOutput()           → 增量读取输出
          ├── killBackgroundCommand() → 终止
          └── terminateSession()    → 销毁会话
```

**工具形态**：三个独立的工具函数，共同构成一个"状态化 Shell 工具组"

**LLM 调用模式**（在 AGENT_LOOP 中使用）：

```
Agent Loop Iteration
  │
  ├── LLM 决定调用 backend_shell("npm run build")
  │   └── 得到 shell_id = "session-1"
  │
  ├── LLM 决定调用 shell_output("session-1")
  │   └── 得到增量输出
  │
  ├── LLM 分析输出，决定下一步
  │
  ├── LLM 决定调用 backend_shell("npm test")
  │   └── 复用 shell_id = "session-1"（状态保持）
  │
  └── ...
```

### 3.2 方案对比：复用 vs 新建

| 维度 | 复用 backend-shell | 新建 interactive-shell 工具 |
|------|:---:|:---:|
| **实现成本** | 无需改动 | 需要完整实现 |
| **PTY 支持** | ❌ 仅后台 spawn | ✅ 支持 PTY 交互 |
| **交互式输入** | ❌ stdin 只能预先提供 | ✅ 支持运行时输入 |
| **状态保持** | ✅ 通过 session 复用 | ✅ 通过 session 复用 |
| **输出轮询** | ✅ 已有增量机制 | ✅ 同样需要实现 |
| **现有兼容性** | ✅ AGENT_LOOP 已集成 | ❌ 需注册新工具 |
| **LLM 调用** | ✅ 已集成 | ✅ 同样可集成 |

### 3.3 分析结论

**结论：应当复用 backend-shell 模式，但需要补充 PTY 和交互式输入能力。**

原因如下：

1. **backend-shell 的工具组模式（后 台执行 + 输出获取 + 终止）是合理的抽象**，三个工具的职责分离清晰，与 LLM 工具调用模式（Function Calling）天然匹配

2. **当前缺失的关键能力是 PTY 和交互式输入**：
   - `startBackgroundCommand` 只支持 spawn（管道模式），不支持 PTY
   - 没有 `sendInput()` 方法用于向 shell 写入输入
   - 没有交互提示检测能力

3. **增强方案**：扩展 backend-shell 工具组，新增 PTY 交互能力

```
# 现有工具（增强）
backend_shell tool
  └── 新增参数: executor_mode: "background" | "interactive"
      当 executor_mode = "interactive" 时使用 PTY

# 新增工具
shell_send_input tool
  ├── shell_id: string (必需)
  ├── input: string (必需，要发送的输入)
  └── wait_for_prompt: boolean (可选，发送后是否等待下一个提示)

shell_wait tool
  ├── shell_id: string (必需)
  ├── timeout: number (可选，等待超时)
  └── expected_pattern: string (可选，等待指定输出模式)
```

4. **对于分步脚本执行**：backend-shell 的模式天然适合 LLM 编排，每个命令作为一次工具调用，LLM 可以分析输出后决定下一步

```
# LLM 编排的分步脚本：
1. backend_shell("cd /project && npm run build") → shell_id: "s1", 输出构建日志
2. LLM 分析日志，发现构建成功
3. backend_shell("npm test", shell_id: "s1") → 输出测试结果
4. LLM 分析结果，发现测试失败
5. backend_shell("cat test-results.xml | grep failure") → 查看失败详情
```

这种模式下不需要专用的 SCRIPT 节点，AGENT_LOOP + backend-shell 工具已经可以满足需求。

### 3.4 有状态节点与状态化工具的关系

```
                    ┌──────────────────────────────────────┐
                    │        INTERACTIVE_SCRIPT 节点        │
                    │        (高层抽象，面向工作流编辑)       │
                    │                                      │
                    │  内部封装:                             │
                    │  ├── 命令执行                          │
                    │  ├── 交互提示检测                      │
                    │  ├── UserInteraction 协调              │
                    │  └── LLM 集成                         │
                    └──────────┬───────────────────────────┘
                               │ 底层使用
                               ▼
┌──────────────────────────────────────────────────────────┐
│                    TerminalService                        │
│                                                          │
│  ┌──────────────────────────────────────────────────────┐ │
│  │           backend-shell 工具组（面向 LLM/Agent）      │ │
│  │                                                      │ │
│  │  backend_shell → start/continue session              │ │
│  │  shell_output  → poll output                         │ │
│  │  shell_send_input → send input to PTY（新增）        │ │
│  │  shell_wait → wait for pattern（新增）               │ │
│  │  shell_kill → terminate session                      │ │
│  └──────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────┘
```

**两者将共用同一个 TerminalService + Session 体系**：
- INTERACTIVE_SCRIPT 节点内部使用 TerminalService 的 PTY 会话
- backend-shell 工具也使用 TerminalService 的后台执行会话
- 共享 Shell 会话、环境变量、工作目录管理

---

## 4. 多种接收策略分析

### 4.1 问题域

交互式脚本/长期运行脚本的输出接收有不同场景需求：

| 场景 | 特点 | 接收策略需求 |
|------|------|-------------|
| 快速脚本 | 秒级完成，输出少 | 一次性获取全部输出 |
| 长期运行脚本 | 分钟级，持续输出日志 | 增量获取，不阻塞 |
| 交互式 CLI | 输出→等待输入→输出 | 实时输出，检测暂停点 |
| 流式处理 | 持续输出，直到终止 | 持续推送，防抖聚合 |
| 分步流水线 | 多步骤，步骤间有边界 | 分步获取，步骤隔离 |

### 4.2 策略设计

#### 策略 1：一次性获取（One-shot）

**当前**：`executeOneOff` 等待进程结束后一次性返回全部输出

```
适用场景: 快速脚本，不需要中间输出
实现: await child_process -> resolve(stdout)
优点: 简单，调用方无需管理
缺点: 不能用于长期运行脚本
```

#### 策略 2：轮询获取增量输出（Polling）

**当前**：`startBackgroundCommand` + `getOutput`（采用增量 `lastReadIndex` 机制）

```
适用场景: 长期运行脚本（docker build, npm install）
实现:
  1. startBackgroundCommand → 后台执行
  2. 定时调用 getOutput(sessionId) → 获取自上次读取后的新行
  3. 流程完成后调用 terminateSession

增量机制 (TerminalRegistry):
  outputLines: string[]  ← 所有输出行
  lastReadIndex: number  ← 上次读取到的位置
  getOutput() → outputLines.slice(lastReadIndex)
  markOutputRead() → lastReadIndex = outputLines.length

优点: 适合 AI 编排场景（LLM 每次工具调用时顺便获取最新输出）
缺点: 
  - 频繁轮询有性能开销
  - 实时性依赖于轮询频率
  - 输出持续累积可能导致内存问题（需要 maxOutputLines 限制）
```

#### 策略 3：防抖增量推送（Debounced Push）

**适用场景**：流式处理且需要中间获取信息，但不需要实时推送

```
实现:
  1. 后台进程持续输出
  2. OutputBuffer 接收所有输出
  3. 设置 debounce 定时器（如 500ms）
  4. 定时器触发时，推送自上次推送后的增量输出
  5. 如果输出频率高，debounce 自动聚合

OutputBuffer 设计:
  ┌─────────────┐     ┌──────────────────┐     ┌──────────────┐
  │ PTY/stdout  │────▶│  OutputBuffer    │────▶│ EventEmitter │
  │             │     │                  │     │              │
  │ 持续写入    │     │ buffer: string[] │     │ 'output'     │
  │             │     │ debounceTimer    │     │ 'completed'  │
  └─────────────┘     │ flush()          │     └──────────────┘
                      └──────────────────┘

优点:
  - 降低事件频率，减少调用开销
  - 适合 UI 更新、日志展示
  - 防抖自动聚合高频输出

缺点:
  - debounce 延迟导致输出不是完全实时的
  - 仍需要消费者监听事件或轮询
```

#### 策略 4：实时流式推送（Live Streaming）

**适用场景**：需要实时查看脚本输出（如交互式 CLI 的回显）

```
实现:
  1. PTY 模式的 stdout 通过 EventEmitter 实时推送
  2. 每行输出触发 'line' 事件
  3. 每块输出触发 'data' 事件
  4. 交互提示检测器在流上实时匹配

事件流:
  PTY.onData → 行分割 → 触发 'line' 事件
                        ├── 输出累积到 buffer
                        ├── 传递给 promptDetector 匹配
                        └── 通过 MessageBus 推送到 UI

优点:
  - 实时性最好
  - 交互提示可以即时检测

缺点:
  - 消费者必须在线监听
  - 无法用于 LLM 工具调用模式（需等待工具返回）
```

#### 策略 5：步骤级边界输出（Step-based）

**适用场景**：分步流水线（build→test→deploy），需要区分每个步骤的输出

```
实现:
  1. 每个步骤作为一个独立的命令执行
  2. 步骤执行完成后，该步骤输出被标记为"已关闭"
  3. 下一步骤在新的上下文中开始

数据模型:
  StepOutput {
    stepIndex: number,
    command: string,
    stdout: string[],
    stderr: string[],
    exitCode: number,
    startTime: number,
    endTime: number,
  }

优点:
  - 输出组织清晰，便于 LLM 分析每个步骤的结果
  - 步骤隔离，下一步不会混淆到上一步的输出

缺点:
  - 需要明确知道步骤边界（不是所有脚本都有清晰分步）
```

### 4.3 统一策略框架

```typescript
// 统一的 OutputStrategy 配置
type OutputStrategy =
  | { type: "one-shot" }
  | { type: "polling"; intervalMs: number; maxOutputLines?: number }
  | { type: "debounced-push"; debounceMs: number; maxOutputLines?: number }
  | { type: "live-streaming" }
  | { type: "step-based"; steps: string[] };

// 策略选择建议
const strategySelector = {
  "fast-script":        { type: "one-shot" },
  "long-running":       { type: "polling", intervalMs: 2000, maxOutputLines: 10000 },
  "interactive-cli":    { type: "live-streaming" },
  "streaming-log":      { type: "debounced-push", debounceMs: 500, maxOutputLines: 50000 },
  "step-pipeline":      { type: "step-based" },
};
```

### 4.4 策略与现有 TerminalService 的集成

```typescript
// TerminalService 增强
class TerminalService {
  // [已有] 一次性执行
  async executeOneOff(command, options): Promise<ExecuteResult>;
  
  // [已有] 后台执行 + 轮询
  async startBackgroundCommand(sessionId, command): Promise<ExecuteResult>;
  async getOutput(sessionId, options): Promise<string>;
  
  // [新增] 流式执行（PTY 模式）
  async executeStreaming(sessionId, command, options: {
    onData?: (chunk: string) => void;
    onLine?: (line: string) => void;
    onExit?: (exitCode: number) => void;
  }): Promise<void>;
  
  // [新增] 带防抖的输出推送
  async startDebouncedOutput(sessionId, options: {
    debounceMs: number;
    onOutput: (output: string) => void;
    onError?: (error: Error) => void;
  }): Promise<DebouncedSubscription>;
  
  // [新增] 发送输入到交互式会话
  async sendInput(sessionId, input: string): Promise<void>;
  
  // [新增] 等待指定输出模式
  async waitForPattern(sessionId, pattern: RegExp, timeout?: number): Promise<{
    matched: boolean;
    matchedLine: string;
    accumulatedOutput: string;
  }>;
}
```

### 4.5 策略与 LLM 交互的适配

**场景 A：LLM 驱动的长期运行脚本监控**

```
1. backend_shell("docker build -t myapp .") → shell_id: "s1"
2. shell_output("s1") → "[1/5] FROM node:18..."
   LLM: "构建正在进行中，继续监控"
3. shell_output("s1") → "[3/5] RUN npm install..."
   LLM: "安装依赖中..."
4. shell_output("s1") → "Successfully built myapp"
   LLM: "构建完成，可以部署了"
   
→ 使用策略 2（轮询），每次工具调用获取增量
```

**场景 B：流式日志 + 防抖**

```
1. backend_shell("tail -f /var/log/app.log") → shell_id: "s1"
2. 输出以高频率持续产生
3. 防抖 500ms 后推送一次聚合输出
4. LLM 每 30 秒分析一次聚合日志

→ 使用策略 3（防抖推送），减少 LLM 调用次数
```

**场景 C：交互式 CLI 工具**

```
1. backend_shell("ssh-keygen -t ed25519", executor_mode: "interactive")
     → 实时捕获输出
     → 检测到 "Enter file in which to save the key:"
     → 暂停等待输入
2. 调用 shell_send_input("s1", "/home/user/.ssh/id_ed25519\n")
     → 继续执行
     → 检测到 "Enter passphrase:"
     → 暂停等待输入
3. 调用 shell_send_input("s1", "my-secure-passphrase\n")

→ 使用策略 4（实时流式）+ 交互提示检测
```

---

## 5. 统一架构总览

### 5.1 三种交互模式的关系

```
┌─────────────────────────────────────────────────────────────────┐
│                     Workflow Graph Layer                         │
│                                                                  │
│  ┌─────────────────────┐  ┌──────────────────┐  ┌─────────────┐ │
│  │  INTERACTIVE_SCRIPT │  │  SCRIPT + LOOP   │  │ AGENT_LOOP  │ │
│  │  (有状态 Entity)     │  │  (嵌入式展开)     │  │ +           │ │
│  │                     │  │                  │  │ backend-    │ │
│  │  适用: 多轮交互     │  │  适用: 分步执行   │  │ shell 工具  │ │
│  │  交互式 CLI         │  │  流水线部署       │  │             │ │
│  │  长期运行脚本       │  │  单次确认         │  │ LLM 编排    │ │
│  └────────┬────────────┘  └────────┬─────────┘  └──────┬──────┘ │
└───────────┼────────────────────────┼───────────────────┼────────┘
            │                        │                   │
            ▼                        ▼                   ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Script Engine Layer                            │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │              TerminalService                                 │ │
│  │                                                              │ │
│  │  executeOneOff()  startBackgroundCommand()  executeStreaming│ │
│  │  sendInput()      waitForPattern()         startDebounced() │ │
│  │  getOutput()      terminateSession()       cleanup()        │ │
│  └────────────────────────┬────────────────────────────────────┘ │
└───────────────────────────┼──────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Executor Adapter Layer                         │
│                                                                  │
│  ┌──────────┐  ┌──────────────┐  ┌──────────┐  ┌─────────────┐ │
│  │ Direct   │  │ Background   │  │   PTY    │  │  Debounced  │ │
│  │ Executor │  │ Executor     │  │ Executor │  │  Output     │ │
│  │          │  │              │  │          │  │  Buffer     │ │
│  │ 一次性   │  │ 后台执行     │  │ 交互式   │  │  防抖推送   │ │
│  │ 执行     │  │ + 轮询输出   │  │ 执行     │  │  聚合       │ │
│  └────┬─────┘  └──────┬───────┘  └────┬─────┘  └──────┬──────┘ │
└───────┼───────────────┼───────────────┼────────────────┼────────┘
        │               │               │                │
        ▼               ▼               ▼                │
┌──────────────────────────────────────────────────────┐ │
│                  操作系统子进程                        │ │
│  (child_process.spawn / node-pty)                     │ │
└──────────────────────────────────────────────────────┘ │
        │                                               │
        └───────────────────────────────────────────────┘
                         输出流通过 EventEmitter/MessageBus 推送
```

### 5.2 策略选择指南

```
脚本类型                   推荐策略              节点类型
──────────────────────────────────────────────────────────
快速命令 (ls, git status)    one-shot              SCRIPT
构建脚本 (npm run build)     polling               SCRIPT + LOOP
长期服务 (npm run dev)       debounced-push        AGENT_LOOP + tool
交互式 CLI (ssh-keygen)      live-streaming        INTERACTIVE_SCRIPT
分步流水线                    step-based            INTERACTIVE_SCRIPT (embedded)
日志监控 (tail -f)           debounced-push        AGENT_LOOP + tool
混合场景 (部署+确认)          live-streaming        INTERACTIVE_SCRIPT
                            + step-based
```

### 5.3 核心决策汇总

| 决策 | 结论 |
|------|------|
| **有状态节点设计路线** | 混合方案（路线 C）：核心交互用专用有状态节点 Entity，简单分步用嵌入式展开为 LOOP |
| **Entity 模式** | 仿照 AGENT_LOOP Entity 模式，创建 `InteractiveScriptSessionEntity`，包含 Config + State + Runtime Managers |
| **状态化工具** | 复用并增强 backend-shell 工具组：新增 `shell_send_input`、`shell_wait`，扩展 `backend_shell` 支持 PTY 模式 |
| **接收策略** | 提供 5 种策略：one-shot / polling / debounced-push / live-streaming / step-based，通过 OutputStrategy 统一配置 |
| **与 LOOP 嵌套** | 有状态 Entity 跨迭代保持 Shell 会话状态，嵌入式展开每次迭代独立执行但通过 backend-shell session 保持共享状态 |
| **与 UserInteraction 集成** | 扩展 `UserInteractionOperationType`，新增 `SCRIPT_INTERACTION`，复用事件系统和超时/取消机制 |
| **与 LLM 集成** | 三种模式：blocking / llm-assisted / hybrid，通过节点配置中的 `interactionMode` 和 `llmProfileId` 控制 |
| **Checkpoint 支持** | Entity 模式完整支持（State 序列化 + Shell 会话重建），嵌入式模式由父 LOOP 的 Checkpoint 覆盖 |