# Component Message Output Adaptation Analysis V2

Date: 2026-05-31
Scope: 深入分析 cli-app 的输出功能如何适配 SDK component-message 系统，发现现有实现中的结构性问题并给出改进建议

## 1. 当前适配架构总览

### 1.1 SDK 侧核心组件

| 组件 | 文件 | 职责 |
|------|------|------|
| MessageBus | `sdk/api/shared/component-message/message-bus.ts` | 消息中枢：publish/subscribe、路由规则匹配、OutputHandler 注册 |
| MessagePublisher | `sdk/api/shared/component-message/publisher-api.ts` | 按分类的便捷发布 API |
| Routing Utilities | `sdk/api/shared/component-message/routing-utils.ts` | 路由规则匹配函数 |
| Type Definitions | `packages/types/src/component-message/` | 8 个消息分类、5 个输出目标、Entity 标识、路由规则定义 |

### 1.2 CLI-App 侧适配层

```
SDK MessageBus (publish)
    │
    ├─▶ [Layer 1] Routing Rules (CLI_ROUTING_RULES, 10 rules)
    │     │
    │     ├─▶ [Layer 2] OutputHandlers (注册在 MessageBus 上)
    │     │     ├── TUIOutputHandler (target=TUI)
    │     │     │   └─ ring buffer + subscriber pattern
    │     │     ├── DisplayFileHandler (target=FILE_DISPLAY)
    │     │     │   └─ batched writes → output.md
    │     │     └── FunctionalFileHandler (target=FILE_FUNCTIONAL)
    │     │         └─ writes → human-relay-output.txt
    │     │
    │     └─▶ [Layer 3] Screen 订阅者 ← 当前实现
    │           ├── AgentScreen ← 订阅 TUIOutputHandler
    │           ├── DashboardScreen ← 订阅 TUIOutputHandler
    │           └── WorkflowScreen ← 订阅 TUIOutputHandler
    │
    └─▶ [Layer 4] CLI 命令层 (并行路径，不经过 Component Message)
          (Adapters → cli-formatters → CLIOutput → stdout/stderr/log)
```

### 1.3 消息发布后的完整流程

```typescript
// MessageBus.publish() 内部逻辑
publish(input) {
  // 1. 构造消息
  const message = { ...input, id: generateId(), timestamp: Date.now() };

  // 2. 历史记录
  if (enableHistory) { history.push(message); }

  // 3. 路由决策：匹配 routing rules → 确定 OutputTargets
  const decision = this.decideOutput(message);

  // 4. 路由到 OutputHandlers
  for (const target of decision.targets) {
    const handler = this.findHandler(target, message);
    if (handler) { handler.handle(message); }
  }

  // 5. 通知直接订阅者 (subscribe/subscribeAll)
  this.notifySubscribers(message);
}
```

关键点：MessageBus.publish() 同时触发两条路径——
- **路径 A**: routing rules → OutputHandler (TUIOutputHandler → Screen 订阅者)
- **路径 B**: 直接订阅者 (notifySubscribers，当前未被 Screen 使用)

---

## 2. 各层适配细节分析

### 2.1 路由规则层 (routing-rules.ts)

10 条规则的覆盖分析：

| 规则 | 匹配条件 | 输出目标 | 优先级 |
|------|---------|---------|--------|
| agent-llm-stream | AgentMessageType.LLM_STREAM | TUI | 100 |
| agent-human-relay-request | AgentMessageType.HUMAN_RELAY_REQUEST | TUI + FILE_FUNCTIONAL + FILE_DISPLAY | 100 |
| agent-human-relay-status | HUMAN_RELAY_RESPONSE/TIMEOUT/CANCEL | TUI + FILE_DISPLAY | 100 |
| agent-tool-call | TOOL_CALL_START/END | TUI | 100 |
| agent-tool-result | TOOL_RESULT | FILE_DISPLAY | 100 |
| workflow-execution-node | NODE_START/END | TUI + FILE_DISPLAY | 100 |
| workflow-execution-fork-branch | FORK_BRANCH_START/END | FILE_DISPLAY | 100 |
| subgraph-events | category=SUBGRAPH | FILE_DISPLAY | 100 |
| error-messages | level=error/critical | TUI + FILE_DISPLAY | 50 |
| default | match all | TUI | 999 |

### 2.2 OutputHandler 层

**TUIOutputHandler** (`tui-output-handler.ts`):
- `target = TUI`，`supports()` 返回 `true` (接受所有消息)
- 内部维护 ring buffer (100 条) + Set 类型的 subscriber 集合
- 每个 Screen 通过 `subscribe()` 注册回调，只接收被路由到 TUI 的消息
- 实现了 `flush()` (空操作) 和 `close()` (清理)

**DisplayFileHandler** (`display-file-handler.ts`):
- `target = FILE_DISPLAY`，`supports()` 检查 6 种特定类型
- 按 session 缓冲 DisplaySection，每 2 秒批量写入
- 类型转换：将不同类型的 message.data 映射为 DisplaySection

**FunctionalFileHandler** (`functional-file-handler.ts`):
- `target = FILE_FUNCTIONAL`，仅处理 `agent.human_relay.request`
- 直接调用 `HumanRelayService.writeOutput()` 写入文件
- 同步操作，无缓冲

### 2.3 Screen 订阅层

三个 Screen 都订阅 TUIOutputHandler（而非 MessageBus）：

- **AgentScreen**: 按 category=AGENT + entity.id 过滤，dispatch 到 5 个子处理器
- **DashboardScreen**: 按 category=AGENT/WORKFLOW_EXECUTION 过滤，更新计数
- **WorkflowScreen**: 按 category=WORKFLOW_EXECUTION + 特定 type 过滤，更新日志面板

### 2.4 CLI 命令层 (独立路径)

完全不经过 Component Message 系统：
- 命令调用 SDK API → Adapters → Domain Objects
- cli-formatters.ts 格式化输出（含 formatComponentMessage 函数）
- CLIOutput 输出到 stdout/stderr/log

---

## 3. 问题分析

### 3.1 P1 - TUIOutputHandler 中间层冗余

**问题描述**:
当前 Screens 通过 TUIOutputHandler 订阅消息，而不是直接订阅 MessageBus。但 TUIOutputHandler 提供的全部能力 MessageBus.subscribe() 都已原生支持：

| 能力 | MessageBus.subscribe() | TUIOutputHandler |
|------|----------------------|-----------------|
| category 过滤 | ✅ filter.categories | ❌ 需要手动 if 检查 |
| type 过滤 | ✅ filter.types | ❌ 需要手动 if 检查 |
| level 过滤 | ✅ filter.levels | ❌ 需要手动 if 检查 |
| entityId 过滤 | ✅ filter.entityIds | ❌ 需要手动 if 检查 |
| ring buffer | ❌ (通过 getHistory()) | ✅ (但未被任何 Screen 使用) |
| 订阅管理 | ✅ MessageSubscription | ✅ 返回 unsubscribe 函数 |

**关键差异**:
- TUIOutputHandler 的 Screen 只收到被路由规则决策为 TUI 的消息
- MessageBus 订阅可以收到所有消息（除非使用过滤条件）

**问题实质**:
当前 TUIOutputHandler 的订阅者模式实际上是一个 **自定义的轻量 MessageBus**，功能是 MessageBus.subscribe() 的子集。它唯一提供的差异化价值——**"只接收被路由到 TUI 的消息"**——实际上是 MessageBus.subscribe() 可轻易实现的（只需要在组装时注意过滤）。

**影响**:
- 代码复杂度增加：多一个需要维护的中间层
- 开发认知成本：新开发者需要理解 TUIOutputHandler 的定位
- 测试负担：多一个需要测试的组件

**改进建议**:
将 Screens 改为直接订阅 MessageBus，移除 TUIOutputHandler 的订阅者模式。TUIOutputHandler 退化为纯 OutputHandler（只负责路由目的，不做订阅分发）。

方案对比：
```
当前: MessageBus → TUIOutputHandler(handler) → TUIOutputHandler(subscriber) → Screen
改进: MessageBus → TUIOutputHandler(handler) + MessageBus → Screen(subscriber)
```

### 3.2 P1 - HumanRelay 双重写入风险

**问题描述**:
当 `agent.human_relay.request` 消息发布时，存在两条写入路径：

```
路径 A (OutputHandler):
  MessageBus.publish()
    → decideOutput() → [TUI, FILE_FUNCTIONAL, FILE_DISPLAY]
    → FunctionalFileHandler.handle()
      → HumanRelayService.writeOutput({ content: prompt })

路径 B (TUIHumanRelayHandler):
  TUIHumanRelayHandler.handle(request)
    → HumanRelayService.writeOutput({ content: request.prompt })
    → (直接调用，不经过 MessageBus)
```

路径 A 是路由驱动的被动写入，路径 B 是 HumanRelayService 使用时的主动写入。两者写入的是**同一个文件**（同一 sessionId 的 human-relay-output.txt）。

**潜在问题**:
- 如果 SDK 先通过 MessageBus 发布消息（触发路径 A），然后 TUIHumanRelayHandler 再被调用（路径 B），会导致双重写入
- 竞态条件：异步写入时可能发生覆盖
- 内容可能不一致（路径 A 从 message.data 提取，路径 B 从 request.prompt 提取）

**根因**:
FunctionalFileHandler 的职责边界不清晰。它和 TUIHumanRelayHandler 都在做相同的事——将 Human Relay prompt 写入 functional 文件。

**改进建议**:
方案一：移除功能性重复。FunctionalFileHandler 只负责路由分发（如果有其他 FILE_FUNCTIONAL 用途），Human Relay 的文件写入完全由 TUIHumanRelayHandler 负责。
方案二：保留 FunctionalFileHandler，在 TUIHumanRelayHandler 中跳过写入（依赖路径 A 完成），但这需要保证路径 A 在路径 B 之前执行，引入时序耦合。

**建议采用方案一**，因为：
- HumanRelayHandler 是 Human Relay 交互的最终执行者，文件写入是它的职责
- FunctionalFileHandler 的存在会导致两个组件对同一文件产生竞争
- 简化架构：一个职责由一个组件完成

### 3.3 P2 - CLI 命令层绕过 Component Message 系统

**问题描述**:
CLI 命令（如 `wf workflow list`、`wf agent start`）的输出来自独立路径：

```
命令执行 → Adapter (调用 SDK API) → Domain Objects → cli-formatters → CLIOutput → stdout
```

完全不经过 MessageBus、路由规则、OutputHandler。

**后果**:
- 无法对命令输出应用统一的输出策略（例如：切换到 JSON 模式、重定向到文件）
- 格式不一致：命令输出和 component message 输出可能使用不同的样式
- 无法利用路由规则的灵活性（例如：将特定命令输出路由到文件，同时在 TUI 显示摘要）

**改进建议**:
构建 **Command Output Bridge**，将命令执行结果封装为 ComponentMessage 发布到 MessageBus，然后通过路由规则决定输出方式。

```typescript
// 改进后的流程
命令执行 → 结果封装为 ComponentMessage → MessageBus.publish()
  → 路由规则 → TUIOutputHandler / DisplayFileHandler → 统一输出
```

这需要大量重构，但能统一输出管道。短期可以先将 `formatComponentMessage` 集成到 CLIOutput 中，至少确保输出风格一致。

### 3.4 P2 - 路由规则覆盖不完整

**问题描述**:
以下 SDK 定义的消息类别在路由规则中没有完整覆盖：

| 类别 | 定义的消息类型数 | 路由规则覆盖 | 未覆盖的类型 |
|------|----------------|-------------|-------------|
| WORKFLOW_EXECUTION | 20+ | NODE_START/END, FORK_BRANCH_START/END | EXECUTION_START/END/PAUSE/RESUME/CANCEL, NODE_ERROR/SKIP, VARIABLE_SET/GET, FORK_START, JOIN_WAIT/COMPLETE, AGENT_CALL/RETURN, SUBGRAPH_CALL/RETURN |
| AGENT | 18 | LLM_STREAM, HUMAN_RELAY_*, TOOL_CALL_*, TOOL_RESULT | AGENT_START/END/PAUSE/RESUME/CANCEL, ITERATION_START/END/LIMIT, LLM_REQUEST/RESPONSE/ERROR, TOOL_ERROR, CHECKPOINT_CREATE/RESTORE, MESSAGE_ADD |
| CHECKPOINT | - | 无 | 完全未覆盖 |
| EVENT | - | 无 | 完全未覆盖 |
| TOOL | 4 | 无 (通过 AgentMessageType 间接覆盖) | TOOL_CALL_START/END, TOOL_RESULT, TOOL_ERROR |
| SUBGRAPH | - | category 匹配 | 粗粒度覆盖，无法区分具体类型 |

**影响**:
- AGENT_START/END 等生命周期消息走 default 规则（仅 TUI），无法路由到文件
- CHECKPOINT 和 EVENT 消息走 default 规则，可能被错误处理
- LLM_REQUEST/RESPONSE 等调试信息无法路由到日志文件

**改进建议**:
补充路由规则覆盖关键消息类型。至少覆盖：
- Agent 生命周期：AGENT_START/END → TUI
- WorkflowExecution 生命周期：EXECUTION_START/END → TUI + FILE_DISPLAY
- Agent 迭代：ITERATION_START/END → TUI + FILE_DISPLAY
- LLM 交互：LLM_REQUEST/RESPONSE (debug级别) → 可选路由

### 3.5 P2 - MessageBus 与 TUIOutputHandler 的双重订阅路径

**问题描述**:
MessageBus 的 `publish()` 方法同时触发 OutputHandler 和 notifySubscribers 两条路径。当 Screens 订阅 TUIOutputHandler 时，消息流是：

```
MessageBus.publish(message)
  ├── decideOutput() → [TUI, ...]
  │   └── TUIOutputHandler.handle(message)
  │       └── Screen subscribers (当前路径)
  │
  └── notifySubscribers(message)
      └── 无订阅者 (当前)
```

但如果未来有开发者误以为 Screens 应该订阅 MessageBus（正如 PRD 文档和之前架构文档所指出的），就会导致**双重消费**：

```
MessageBus.publish(message)
  ├── TUIOutputHandler.handle(message)
  │   └── Screen A (订阅 TUIOutputHandler)
  │
  └── notifySubscribers(message)
      └── Screen A (也订阅 MessageBus)
              → 同一个消息被 Screen A 处理两次
```

**根因**:
系统存在两条并行的订阅路径，文档描述与实际实现不一致（旧文档说 Screens 应订阅 MessageBus，实际代码订阅 TUIOutputHandler）。这种不一致增加了误用的风险。

**改进建议**:
统一为一条订阅路径。选择方案：
- **方案 A**: Screens 订阅 MessageBus（移除 TUIOutputHandler 的订阅者模式）——推荐
- **方案 B**: Screens 订阅 TUIOutputHandler（移除 Screens 的 MessageBus 订阅能力）

### 3.6 P3 - DisplayFileHandler 的 supports() 与路由规则不匹配

**问题描述**:
DisplayFileHandler 的 `supports()` 方法定义了 6 种支持的类型，但路由规则可能将其他类型的消息路由到 FILE_DISPLAY：

```typescript
// display-file-handler.ts
supports(message): boolean {
  const supportedTypes = [
    AgentMessageType.TOOL_RESULT,
    WorkflowExecutionMessageType.NODE_START,
    WorkflowExecutionMessageType.NODE_END,
    AgentMessageType.CHECKPOINT_CREATE,
    AgentMessageType.ITERATION_START,
    SystemMessageType.ERROR,
  ];
  return supportedTypes.includes(message.type);
}
```

路由规则会将以下消息路由到 FILE_DISPLAY，但 DisplayFileHandler 不支持：
- `FORK_BRANCH_START/END` (workflow-execution-fork-branch 规则指定 FILE_DISPLAY)
- `SUBGRAPH` 类别的所有消息 (subgraph-events 规则指定 FILE_DISPLAY)
- `HUMAN_RELAY_TIMEOUT/CANCEL` (agent-human-relay-status 规则指定 FILE_DISPLAY)

这些消息会被 `findHandler()` 过滤掉，**静默丢弃**，没有任何警告。

**影响**:
- 数据丢失：预期写入文件的消息被无声地丢弃
- 调试困难：开发者难以发现消息在 supports() 阶段被过滤
- 配置误导：路由规则看起来指向了 FILE_DISPLAY，但实际并未生效

**改进建议**:
方案一：扩展 DisplayFileHandler 的 supports() 以匹配所有路由规则中 FILE_DISPLAY 目标的消息类型。
方案二：在 MessageBus 中添加日志，当路由规则指向某个目标但找不到支持的 Handler 时记录 warning。

建议组合使用两种方案。

### 3.7 P3 - 高频消息缺少 Handler 级别的节流/批量处理

**问题描述**:
`LLM_STREAM` 消息在 LLM 流式输出时高频产生（每个 chunk 一条消息）。当前的处理是：

```
MessageBus → TUIOutputHandler → AgentScreen.handleLLMStreamMessage()
  → 每 100ms 或 buffer > 200 字符时渲染
```

但 TUIOutputHandler 层面没有任何节流——所有消息都逐个触发 subscriber 回调。如果未来有多个 Screen 订阅了 LLM_STREAM，每个都会收到全部高频消息。

**影响**:
- 当前影响有限（仅 AgentScreen 一个订阅者）
- 扩展性差：未来增加新的 LLM 消息消费者时，需要各自实现节流逻辑
- 性能风险：TUIOutputHandler 的 `subscribers.forEach()` 是同步遍历，一个慢回调会阻塞其他回调

**改进建议**:
在 TUIOutputHandler 中增加可选的批处理/节流机制：

```typescript
// 可选：支持节流配置
class TUIOutputHandler {
  constructor(config?: { throttleMs?: number }) {
    if (config?.throttleMs) {
      this.startThrottle(config.throttleMs);
    }
  }
  
  private startThrottle(ms: number) {
    setInterval(() => {
      if (this.batchBuffer.length > 0) {
        const batch = [...this.batchBuffer];
        this.batchBuffer = [];
        for (const sub of this.subscribers) {
          batch.forEach(msg => sub(msg));
        }
      }
    }, ms);
  }
}
```

### 3.8 P3 - setupMessageSystem() 死代码

**问题描述**:
[message-system.ts](file://d:\项目\agent\wf-agent\apps\cli-app\src\config\message-system.ts) 定义了 `setupMessageSystem()` 函数，但 [app.ts](file://d:\项目\agent\wf-agent\apps\cli-app\src\tui\app.ts) 直接创建 MessageBus 实例，没有使用该函数。

**后果**:
- 死代码增加维护负担
- 初始化逻辑分散在两处：message-system.ts (无人调用) 和 app.ts (实际使用)
- 未来开发者可能误以为需要调用 setupMessageSystem()

**改进建议**:
删除 message-system.ts，或将其实用逻辑合并到 app.ts 的 initializeMessageHandlers() 中。

### 3.9 P3 - OutputTarget.EVENT_BUS/LOCAL 没有对应的 Handler

**问题描述**:
`OutputTarget` 定义了 5 个目标：TUI, FILE_FUNCTIONAL, FILE_DISPLAY, EVENT_BUS, NONE。但在 cli-app 中只实现了前 3 个。

如果路由规则意外地指向 EVENT_BUS 或 NONE，消息会静默丢失（findHandler 返回 undefined）。

**改进建议**:
在 MessageBus 的 findHandler() 中添加日志，当无法找到 Handler 时记录 warning。或者添加一个默认的 "null handler" 来处理未匹配的情况。

### 3.10 P3 - Screen 的 MessageBus 参数未被使用

**问题描述**:
所有三个 Screen 的构造函数都接受 `_messageBus?: MessageBus` 参数：

```typescript
constructor(_messageBus?: MessageBus, onBack?: () => void, tuiOutputHandler?: TUIOutputHandler)
```

但参数名以 `_` 开头（TypeScript 惯例表示未使用变量），确实没有任何 Screen 使用这个参数。

**影响**:
- 造成困惑：新开发者看到这个参数会以为 Screens 和 MessageBus 有直接交互
- 增加不必要的依赖传递
- 是 TUIOutputHandler 替代 MessageBus 订阅后的遗留产物

**改进建议**:
从 Screen 构造函数中移除 `_messageBus` 参数。如果未来 Screens 需要直接访问 MessageBus，应通过其他方式（如 getter 或 context）获取。

---

## 4. 问题汇总

| # | 问题 | 严重性 | 类别 | 涉及文件 |
|---|------|--------|------|---------|
| 3.1 | TUIOutputHandler 中间层冗余 | P1 | 架构 | tui-output-handler.ts + 所有 Screen |
| 3.2 | HumanRelay 双重写入风险 | P1 | Bug | functional-file-handler.ts + tui-human-relay-handler.ts |
| 3.3 | CLI 命令层绕过 Component Message | P2 | 架构 | cli-formatters.ts + CLIOutput + Adapters |
| 3.4 | 路由规则覆盖不完整 | P2 | 功能缺失 | routing-rules.ts |
| 3.5 | MessageBus + TUIOutputHandler 双重订阅 | P2 | 架构 | message-bus.ts + tui-output-handler.ts |
| 3.6 | DisplayFileHandler supports() 与路由不匹配 | P3 | Bug | display-file-handler.ts |
| 3.7 | 高频消息缺少 Handler 级节流 | P3 | 性能 | tui-output-handler.ts |
| 3.8 | setupMessageSystem() 死代码 | P3 | 代码质量 | message-system.ts |
| 3.9 | EVENT_BUS 无对应 Handler | P3 | 功能缺失 | message-bus.ts |
| 3.10 | Screen MessageBus 参数未使用 | P3 | 代码质量 | dashboard-screen.ts, agent-screen.ts, workflow-screen.ts |

---

## 5. 改进路线图

### 短期（低风险，高收益）

1. **移除 Screen 的未使用 MessageBus 参数** (3.10)
   - 修改 3 个 Screen 的构造函数签名
   - 更新 app.ts 中的构造调用

2. **补充 DisplayFileHandler 的 supports() 匹配路由规则** (3.6)
   - 添加 FORK_BRANCH_START/END、HUMAN_RELAY_TIMEOUT/CANCEL 到支持列表
   - 或在 findHandler 失败时添加 warning 日志

3. **删除 setupMessageSystem() 死代码** (3.8)
   - 移除 message-system.ts 或标记为废弃

### 中期（需要设计决策）

4. **统一订阅路径** (3.1 + 3.5)
   - 决策：Screens 订阅 MessageBus 还是 TUIOutputHandler？
   - **推荐**：Screens 直接订阅 MessageBus，TUIOutputHandler 退化为纯路由接收器
   - 移 TUIOutputHandler 的 subscriber 模式，保留 ring buffer 用于历史查询
   - 更新所有 Screen 的 setupMessageSubscriptions()

5. **解决 HumanRelay 双重写入** (3.2)
   - 从 FunctionalFileHandler 中移除 Human Relay 写入逻辑
   - 完全由 TUIHumanRelayHandler 负责文件写入

6. **补充路由规则覆盖** (3.4)
   - 添加 Agent 生命周期、WorkflowExecution 生命周期规则
   - 考虑添加 CHECKPOINT 类别的基本路由

### 长期（架构级重构）

7. **统一 CLI 命令输出管道** (3.3)
   - 构建 Command Output Bridge
   - 将命令执行结果封装为 ComponentMessage
   - 统一输出格式和策略

8. **Handler 级节流/批量机制** (3.7)
   - 在 TUIOutputHandler 中添加可配置的节流
   - 为 LLM_STREAM 等高频消息类型启用批处理

---

## 6. 架构演进建议

### 推荐的目标架构

```
SDK Agent/Workflow (消息生产者)
    │
    ▼
MessageBus.publish()
    │
    ├── decideOutput() → [TUI, FILE_DISPLAY, FILE_FUNCTIONAL, ...]
    │
    ├── OutputHandlers (仅文件 IO/非 UI 目的)
    │   ├── DisplayFileHandler → output.md (batched writes)
    │   └── FunctionalFileHandler → human-relay-output.txt
    │
    └── notifySubscribers() (Screen 直接订阅 MessageBus)
        ├── AgentScreen (category=AGENT + entity.id filter)
        ├── DashboardScreen (category=AGENT|WORKFLOW_EXECUTION)
        └── WorkflowScreen (category=WORKFLOW_EXECUTION + type filter)

CLI 命令 (独立路径，通过 Bridge 逐步迁移)
    │
    ▼
Command → Adapter → Result → ComponentMessage → MessageBus.publish()
                                                    │
                                                    └── (统一的路由和输出)
```

### 核心原则

1. **消息总线只负责路由**：MessageBus 决定"消息去哪"，不关心"消息怎么渲染"
2. **OutputHandler 只处理非 UI 输出**：文件写入、事件总线等
3. **Screen 直接消费消息**：通过 MessageBus.subscribe() 获取感兴趣的消息
4. **统一输出管道**：CLI 命令的输出最终也通过 Component Message 系统

---

## 7. 总结

当前 cli-app 的 component-message 适配整体架构基本合理，但存在两个核心结构性问题：

1. **TUIOutputHandler 的双重身份**：既是 OutputHandler（接收路由消息）又是 EventEmitter（分发到 Screen），导致订阅路径混乱。建议将其简化为纯 OutputHandler。

2. **HumanRelay 的双重写入**：FunctionalFileHandler 和 TUIHumanRelayHandler 对同一文件有竞争写入。建议移除 FunctionalFileHandler 中的 Human Relay 逻辑。

此外，路由规则覆盖不完整、CLI 命令层独立、高频消息缺节流等问题需要逐步解决。

解决这些问题的方向是：**简化中间层、明确职责边界、统一输出管道**。