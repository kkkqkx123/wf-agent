# 模块化 Agent 框架 - 控制流与错误处理架构设计文档

## 一、架构概览

### 1.1 核心组件职责划分

本架构将控制流和错误处理划分为四个核心组件，每个组件承担明确的职责：

#### 1.1.1 Signal（中断操作）

**核心职责**：提供操作中断机制，用于取消正在进行的长时间运行操作

**关键特性**：
- 基于 Web 标准的 AbortSignal API
- 支持同步的 abort 状态检查
- 可传递给底层 API（fetch、setTimeout、HTTP 客户端等）
- 包含线程和节点上下文信息

**使用场景**：
- 用户请求暂停或停止线程执行
- LLM 调用超时或被取消
- 工具执行超时或被取消
- HTTP 请求超时或被取消

#### 1.1.2 Result（表示结果）

**核心职责**：以类型安全的方式表示操作的成功或失败结果

**关键特性**：
- 函数式编程风格的 Result 类型
- 支持链式操作（map、andThen、orElse）
- 类型守卫（isOk、isErr）
- 避免异常作为控制流

**使用场景**：
- 验证结果（工作流、节点、触发器验证）
- 业务逻辑执行结果
- API 调用结果
- 工具执行结果

#### 1.1.3 Error（存储错误信息）

**核心职责**：存储详细的错误信息和上下文

**关键特性**：
- 继承自 Error 的错误类层次
- 包含错误严重程度（ERROR、WARNING、INFO）
- 包含错误上下文（线程ID、节点ID、操作名称等）
- 支持错误链（cause 属性）

**使用场景**：
- 验证错误（配置验证、运行时验证）
- 执行错误（节点执行失败、工具执行失败）
- 系统错误（网络错误、超时错误）
- 业务逻辑错误（路由不匹配、条件不满足）

#### 1.1.4 Event（状态通知）

**核心职责**：通知状态变化，用于日志记录、sdk向应用层提供事件监听入口

**关键特性**：
- 异步、非阻塞的事件分发
- 支持优先级和过滤器
- 全局事件监听
- 事件类型枚举

**使用场景**：
- 线程生命周期事件（开始、完成、暂停、恢复、取消）
- 节点执行事件（开始、完成、失败）
- 错误事件（错误发生）
- 系统事件（Token 超限、变量变更）

### 1.2 组件协作关系

```
用户请求中断
    ↓
InterruptionManager 创建 AbortSignal（Signal）
    ↓
AbortSignal 传递给 Executor
    ↓
Executor 将 Signal 传递给底层 API
    ↓
底层 API 检测到 Signal 被中止
    ↓
底层 API 抛出 AbortError（Error）
    ↓
Executor 捕获 AbortError
    ↓
Executor 将 AbortError 转换为 Result
    ↓
Result 包含 ThreadInterruptedException（Error）
    ↓
Executor 返回 Result
    ↓
上层处理 Result
    ↓
EventManager 触发事件（Event）
    ↓
事件监听器接收通知
```

## 二、目录结构设计

### 2.1 packages/types（类型定义层）

```
packages/types/src/
├── common.ts                    # 通用类型定义
├── result.ts                    # Result 类型定义
├── errors.ts                    # Error 类型定义
├── signal.ts                    # Signal 类型定义
├── events.ts                    # Event 类型定义
├── events/
│   ├── base.ts                  # 基础事件类型
│   ├── thread-events.ts         # 线程事件
│   ├── node-events.ts           # 节点事件
│   ├── system-events.ts         # 系统事件
│   └── ...
└── index.ts                     # 统一导出
```

**文件实现方式**：
- 所有文件都是纯类型定义，无状态
- 使用 TypeScript 接口和类型别名
- 导出类型定义供其他模块使用

**文件间关系**：
- `result.ts` 依赖 `common.ts`（ID、Timestamp 等基础类型）
- `errors.ts` 依赖 `common.ts`（ID、Metadata 等基础类型）
- `signal.ts` 依赖 `common.ts` 和 `errors.ts`（ThreadInterruptedException）
- `events.ts` 依赖 `common.ts`（ID、Timestamp、Metadata）
- `events/` 下的文件依赖 `events/base.ts`

### 2.2 packages/common-utils（工具函数层）

```
packages/common-utils/src/
├── signal/                      # Signal 工具模块
│   ├── signal-utils.ts          # Signal 工具函数
│   └── index.ts
│
├── result/                      # Result 工具模块
│   ├── result-utils.ts          # Result 工具函数
│   ├── result-converters.ts     # Result 转换函数
│   └── index.ts
│
├── error/                       # Error 工具模块
│   ├── error-utils.ts           # Error 工具函数
│   └── index.ts
│
├── event/                       # Event 工具模块（可选）
│   ├── event-utils.ts           # Event 工具函数
│   └── index.ts
│
├── utils/                       # 通用工具函数
│   ├── result-utils.ts          # 临时保留，待迁移
│   ├── index.ts
│   └── ...
│
└── index.ts                     # 统一导出
```

**文件实现方式**：
- 所有工具函数都是纯函数，无状态
- 导出函数供其他模块使用
- 不包含业务逻辑，只提供工具函数

**文件间关系**：
- `signal/signal-utils.ts` 依赖 `@modular-agent/types`（Signal 类型）
- `result/result-utils.ts` 依赖 `@modular-agent/types`（Result 类型）
- `result/result-converters.ts` 依赖 `@modular-agent/types`（Result、ExecutionResult 类型）
- `error/error-utils.ts` 依赖 `@modular-agent/types`（Error 类型）
- `result/result-utils.ts` 依赖 `signal/signal-utils.ts`（检查 Signal 状态）
- `result/result-utils.ts` 依赖 `error/error-utils.ts`（检查 AbortError）

### 2.3 sdk/core（核心执行层）

```
sdk/core/
├── execution/
│   ├── managers/
│   │   ├── interruption-manager.ts    # 中断管理器
│   │   ├── thread-lifecycle-manager.ts # 线程生命周期管理器
│   │   └── ...
│   │
│   ├── executors/
│   │   ├── llm-executor.ts            # LLM 执行器
│   │   ├── tool-call-executor.ts      # 工具调用执行器
│   │   └── ...
│   │
│   ├── coordinators/
│   │   ├── node-execution-coordinator.ts  # 节点执行协调器
│   │   ├── llm-execution-coordinator.ts   # LLM 执行协调器
│   │   └── ...
│   │
│   ├── context/
│   │   ├── thread-context.ts          # 线程上下文
│   │   └── ...
│   │
│   └── thread-executor.ts             # 线程执行器
│
├── services/
│   ├── event-manager.ts               # 事件管理器
│   ├── error-service.ts               # 错误服务
│   └── ...
│
└── llm/
    ├── wrapper.ts                     # LLM 包装器
    └── ...
```

**文件实现方式**：
- `interruption-manager.ts`：有状态多实例，每个线程一个实例
- `thread-lifecycle-manager.ts`：有状态多实例，每个线程一个实例
- `event-manager.ts`：有状态全局单例，整个应用一个实例
- `error-service.ts`：有状态全局单例，整个应用一个实例
- `llm-executor.ts`：有状态多实例，每个线程一个实例
- `tool-call-executor.ts`：有状态多实例，每个线程一个实例
- `node-execution-coordinator.ts`：有状态多实例，每个线程一个实例
- `llm-execution-coordinator.ts`：有状态多实例，每个线程一个实例
- `thread-context.ts`：有状态多实例，每个线程一个实例
- `thread-executor.ts`：有状态多实例，每个线程一个实例
- `llm/wrapper.ts`：有状态多实例，每个 profile 一个实例

**文件间关系**：
- `interruption-manager.ts` 持有 `AbortController`（创建 Signal）
- `thread-context.ts` 持有 `interruption-manager`（获取 Signal）
- `llm-executor.ts` 依赖 `thread-context`（获取 Signal）
- `tool-call-executor.ts` 依赖 `thread-context`（获取 Signal）
- `llm-executor.ts` 依赖 `llm/wrapper.ts`（调用 LLM）
- `tool-call-executor.ts` 依赖 `tool-service`（调用工具）
- `node-execution-coordinator.ts` 持有 `llm-executor` 和 `tool-call-executor`
- `llm-execution-coordinator.ts` 持有 `llm-executor` 和 `tool-call-executor`
- `thread-executor.ts` 持有 `node-execution-coordinator`
- `thread-lifecycle-manager.ts` 依赖 `event-manager`（触发事件）
- `error-service.ts` 依赖 `event-manager`（触发错误事件）
- `llm/wrapper.ts` 依赖 `event-manager`（触发 LLM 事件）

### 2.4 sdk/api（API 层）

```
sdk/api/
├── types/
│   └── execution-result.ts          # ExecutionResult 类型定义
│
├── utils/
│   └── result-converters.ts         # Result 转换函数（待迁移到 common-utils）
│
├── operations/
│   └── commands/
│       ├── execution/
│       │   ├── pause-thread-command.ts
│       │   ├── stop-thread-command.ts
│       │   └── ...
│       └── ...
│
└── builders/
    └── execution-builder.ts
```

**文件实现方式**：
- `execution-result.ts`：纯类型定义，无状态
- `result-converters.ts`：纯函数，无状态（待迁移）
- `pause-thread-command.ts`：有状态多实例，每次调用一个实例
- `stop-thread-command.ts`：有状态多实例，每次调用一个实例
- `execution-builder.ts`：有状态多实例，每次构建一个实例

**文件间关系**：
- `pause-thread-command.ts` 依赖 `thread-lifecycle-coordinator`（请求暂停）
- `stop-thread-command.ts` 依赖 `thread-lifecycle-coordinator`（请求停止）
- `execution-builder.ts` 依赖 `thread-executor`（执行工作流）

## 三、核心业务逻辑设计

### 3.1 中断流程

#### 3.1.1 用户请求中断

**调用链**：
```
用户调用 API
    ↓
PauseThreadCommand 或 StopThreadCommand
    ↓
ThreadLifecycleCoordinator.pauseThread() 或 stopThread()
    ↓
ThreadLifecycleManager.pauseThread() 或 stopThread()
    ↓
InterruptionManager.requestPause() 或 requestStop()
    ↓
创建 AbortController 并设置 reason 为 ThreadInterruptedException
    ↓
触发 AbortSignal
    ↓
ThreadLifecycleManager 触发 THREAD_PAUSED 或 THREAD_CANCELLED 事件
```

**状态机**：
```
InterruptionManager 状态：
    null → PAUSE → null（恢复）
    null → STOP → null（停止）
```

**关键逻辑**：
1. 用户通过 API 请求暂停或停止线程
2. 命令对象调用生命周期协调器
3. 生命周期管理器调用中断管理器
4. 中断管理器创建 AbortController 并设置 reason
5. AbortSignal 被触发，所有监听该 signal 的操作都会收到通知
6. 生命周期管理器触发相应的事件

#### 3.1.2 Executor 检测中断

**调用链**：
```
Executor.executeLLMCall() 或 executeToolCall()
    ↓
从 ThreadContext 获取 AbortSignal
    ↓
将 Signal 传递给底层 API（LLM 客户端或工具服务）
    ↓
底层 API 检测到 Signal 被中止
    ↓
底层 API 抛出 AbortError
    ↓
Executor 捕获 AbortError
    ↓
使用 abortErrorToResult 将 AbortError 转换为 Result
    ↓
Result 包含 ThreadInterruptedException
    ↓
返回 Result 给上层
```

**关键逻辑**：
1. Executor 从 ThreadContext 获取 AbortSignal
2. Executor 将 Signal 传递给底层 API
3. 底层 API 在执行过程中检查 Signal 状态
4. 如果 Signal 被中止，底层 API 抛出 AbortError
5. Executor 捕获 AbortError 并转换为 Result
6. Result 包含 ThreadInterruptedException，上层可以识别中断原因

### 3.2 Result 处理流程

#### 3.2.1 验证流程

**调用链**：
```
WorkflowValidator.validate()
    ↓
调用各个子验证器（NodeValidator、TriggerValidator 等）
    ↓
子验证器返回 Result<ValidatedType, ValidationError[]>
    ↓
收集所有验证错误
    ↓
如果有错误，返回 Err(errors)
    ↓
如果没有错误，返回 Ok(validatedObject)
```

**关键逻辑**：
1. 验证器接收待验证的对象
2. 验证器调用各个子验证器
3. 子验证器返回 Result 类型
4. 主验证器收集所有错误
5. 如果有错误，返回 Err
6. 如果没有错误，返回 Ok

#### 3.2.2 执行流程

**调用链**：
```
Executor.executeLLMCall()
    ↓
使用 tryCatchAsyncWithSignal 包装 LLM 调用
    ↓
LLM 调用返回 Result<LLMResult, Error>
    ↓
检查 Result 是否为 Err
    ↓
如果是 Err，检查是否为 AbortError
    ↓
如果是 AbortError，转换为包含 ThreadInterruptedException 的 Result
    ↓
如果是其他错误，转换为包含 ExecutionError 的 Result
    ↓
如果是 Ok，返回包含 LLMResult 的 Result
    ↓
将 Result 转换为 ExecutionResult
    ↓
返回 ExecutionResult 给上层
```

**关键逻辑**：
1. Executor 使用 tryCatchAsyncWithSignal 包装操作
2. 操作返回 Result 类型
3. Executor 检查 Result 的状态
4. 如果是 Err，检查错误类型
5. 如果是 AbortError，转换为包含 ThreadInterruptedException 的 Result
6. 如果是其他错误，转换为包含相应 Error 的 Result
7. 如果是 Ok，返回包含成功值的 Result
8. 将 Result 转换为 ExecutionResult（包含执行时间）

### 3.3 Error 处理流程

#### 3.3.1 错误创建

**调用链**：
```
业务逻辑检测到错误
    ↓
创建相应的 Error 子类实例（ValidationError、ExecutionError 等）
    ↓
设置错误上下文（线程ID、节点ID、操作名称等）
    ↓
设置错误严重程度（ERROR、WARNING、INFO）
    ↓
设置错误链（cause 属性）
    ↓
抛出 Error 或返回包含 Error 的 Result
```

**关键逻辑**：
1. 业务逻辑检测到错误
2. 创建相应的 Error 子类实例
3. 设置错误上下文信息
4. 设置错误严重程度
5. 设置错误链（如果有原始错误）
6. 抛出 Error 或返回包含 Error 的 Result

#### 3.3.2 错误传播

**调用链**：
```
底层操作抛出 Error
    ↓
Executor 捕获 Error
    ↓
使用 getErrorMessage 提取错误消息
    ↓
创建新的 Error（如果需要）
    ↓
设置原始错误为 cause
    ↓
抛出新的 Error 或返回包含 Error 的 Result
    ↓
上层捕获 Error 或处理 Result
    ↓
ErrorService 记录错误
    ↓
EventManager 触发 ERROR 事件
```

**关键逻辑**：
1. 底层操作抛出 Error
2. Executor 捕获 Error
3. Executor 提取错误信息
4. Executor 创建新的 Error（如果需要）
5. Executor 设置错误链
6. Executor 抛出新的 Error 或返回 Result
7. 上层处理 Error 或 Result
8. ErrorService 记录错误
9. EventManager 触发错误事件

### 3.4 Event 触发流程

#### 3.4.1 线程生命周期事件

**调用链**：
```
ThreadLifecycleManager.startThread()
    ↓
触发 THREAD_STARTED 事件
    ↓
触发 THREAD_STATE_CHANGED 事件（RUNNING）
    ↓
ThreadLifecycleManager.pauseThread()
    ↓
触发 THREAD_PAUSED 事件
    ↓
触发 THREAD_STATE_CHANGED 事件（PAUSED）
    ↓
ThreadLifecycleManager.resumeThread()
    ↓
触发 THREAD_RESUMED 事件
    ↓
触发 THREAD_STATE_CHANGED 事件（RUNNING）
    ↓
ThreadLifecycleManager.stopThread()
    ↓
触发 THREAD_CANCELLED 事件
    ↓
触发 THREAD_STATE_CHANGED 事件（CANCELLED）
```

**关键逻辑**：
1. 生命周期管理器执行状态转换
2. 生命周期管理器触发相应的事件
3. 事件管理器分发事件给所有监听器
4. 监听器处理事件（日志记录、监控、UI 更新等）

#### 3.4.2 节点执行事件

**调用链**：
```
NodeExecutionCoordinator.executeNode()
    ↓
触发 NODE_STARTED 事件
    ↓
执行节点逻辑
    ↓
如果成功，触发 NODE_COMPLETED 事件
    ↓
如果失败，触发 NODE_FAILED 事件
    ↓
EventManager 分发事件给所有监听器
```

**关键逻辑**：
1. 节点执行协调器开始执行节点
2. 触发 NODE_STARTED 事件
3. 执行节点逻辑
4. 如果成功，触发 NODE_COMPLETED 事件
5. 如果失败，触发 NODE_FAILED 事件
6. 事件管理器分发事件给所有监听器

## 四、模块集成设计

### 4.1 Signal 模块集成

#### 4.1.1 与 InterruptionManager 集成

**集成方式**：
- InterruptionManager 持有 AbortController
- InterruptionManager 提供 getAbortSignal() 方法
- ThreadContext 持有 InterruptionManager
- Executor 从 ThreadContext 获取 AbortSignal

**调用链**：
```
InterruptionManager.requestPause()
    ↓
创建 AbortController
    ↓
设置 reason 为 ThreadInterruptedException
    ↓
调用 abortController.abort(reason)
    ↓
ThreadContext.getAbortSignal()
    ↓
返回 abortController.signal
    ↓
Executor 获取 Signal
    ↓
将 Signal 传递给底层 API
```

#### 4.1.2 与 Executor 集成

**集成方式**：
- Executor 从 ThreadContext 获取 AbortSignal
- Executor 将 Signal 传递给底层 API
- Executor 使用 tryCatchAsyncWithSignal 包装操作
- Executor 使用 abortErrorToResult 转换 AbortError

**调用链**：
```
Executor.executeLLMCall()
    ↓
从 ThreadContext 获取 AbortSignal
    ↓
使用 tryCatchAsyncWithSignal 包装 LLM 调用
    ↓
将 Signal 传递给 LLM 客户端
    ↓
LLM 客户端检查 Signal 状态
    ↓
如果 Signal 被中止，抛出 AbortError
    ↓
Executor 捕获 AbortError
    ↓
使用 abortErrorToResult 转换为 Result
    ↓
返回 Result
```

### 4.2 Result 模块集成

#### 4.2.1 与 Validator 集成

**集成方式**：
- Validator 使用 Result 类型表示验证结果
- Validator 使用 ok() 和 err() 创建 Result
- Validator 使用 all() 组合多个 Result
- 上层使用 isOk() 和 isErr() 检查结果

**调用链**：
```
WorkflowValidator.validate()
    ↓
调用 NodeValidator.validate()
    ↓
NodeValidator 返回 Result<Node, ValidationError[]>
    ↓
调用 TriggerValidator.validate()
    ↓
TriggerValidator 返回 Result<Trigger, ValidationError[]>
    ↓
收集所有错误
    ↓
如果有错误，返回 Err(errors)
    ↓
如果没有错误，返回 Ok(workflow)
    ↓
上层使用 isOk() 检查结果
    ↓
如果成功，使用 unwrap() 获取值
    ↓
如果失败，使用 unwrapOrElse() 处理错误
```

#### 4.2.2 与 Executor 集成

**集成方式**：
- Executor 使用 Result 类型表示执行结果
- Executor 使用 tryCatchAsyncWithSignal 包装操作
- Executor 使用 abortErrorToResult 转换 AbortError
- Executor 将 Result 转换为 ExecutionResult

**调用链**：
```
Executor.executeLLMCall()
    ↓
使用 tryCatchAsyncWithSignal 包装 LLM 调用
    ↓
LLM 调用返回 Result<LLMResult, Error>
    ↓
检查 Result 是否为 Err
    ↓
如果是 Err，检查错误类型
    ↓
如果是 AbortError，使用 abortErrorToResult 转换
    ↓
如果是其他错误，创建包含 ExecutionError 的 Result
    ↓
如果是 Ok，返回包含 LLMResult 的 Result
    ↓
使用 resultToExecutionResult 转换为 ExecutionResult
    ↓
返回 ExecutionResult
```

### 4.3 Error 模块集成

#### 4.3.1 与 Executor 集成

**集成方式**：
- Executor 创建相应的 Error 子类实例
- Executor 设置错误上下文和严重程度
- Executor 设置错误链（cause 属性）
- Executor 抛出 Error 或返回包含 Error 的 Result

**调用链**：
```
Executor.executeLLMCall()
    ↓
LLM 调用失败
    ↓
捕获原始错误
    ↓
创建 ExecutionError 实例
    ↓
设置错误上下文（线程ID、节点ID、profileId）
    ↓
设置原始错误为 cause
    ↓
抛出 ExecutionError 或返回包含 ExecutionError 的 Result
    ↓
上层捕获 Error 或处理 Result
```

#### 4.3.2 与 ErrorService 集成

**集成方式**：
- ErrorService 接收 Error 实例
- ErrorService 提取错误信息
- ErrorService 记录错误日志
- ErrorService 触发 ERROR 事件

**调用链**：
```
业务逻辑抛出 Error
    ↓
ErrorService.handleError(error)
    ↓
提取错误信息（消息、堆栈、上下文）
    ↓
记录错误日志
    ↓
触发 ERROR 事件
    ↓
EventManager 分发事件给所有监听器
    ↓
监听器处理错误事件
```

### 4.4 Event 模块集成

#### 4.4.1 与 ThreadLifecycleManager 集成

**集成方式**：
- ThreadLifecycleManager 在状态转换时触发事件
- ThreadLifecycleManager 使用 EventManager.emit() 触发事件
- EventManager 分发事件给所有监听器

**调用链**：
```
ThreadLifecycleManager.startThread()
    ↓
创建 ThreadStartedEvent
    ↓
调用 EventManager.emit(event)
    ↓
EventManager 分发事件给所有监听器
    ↓
监听器处理事件
    ↓
创建 ThreadStateChangedEvent
    ↓
调用 EventManager.emit(event)
    ↓
EventManager 分发事件给所有监听器
    ↓
监听器处理事件
```

#### 4.4.2 与 NodeExecutionCoordinator 集成

**集成方式**：
- NodeExecutionCoordinator 在节点执行时触发事件
- NodeExecutionCoordinator 使用 EventManager.emit() 触发事件
- EventManager 分发事件给所有监听器

**调用链**：
```
NodeExecutionCoordinator.executeNode()
    ↓
创建 NodeStartedEvent
    ↓
调用 EventManager.emit(event)
    ↓
EventManager 分发事件给所有监听器
    ↓
监听器处理事件
    ↓
执行节点逻辑
    ↓
如果成功，创建 NodeCompletedEvent
    ↓
调用 EventManager.emit(event)
    ↓
EventManager 分发事件给所有监听器
    ↓
监听器处理事件
```

## 五、关键设计决策

### 5.1 为什么需要 Signal？

**原因**：
1. **中断正在进行的操作**：Result 只能表示操作完成后的结果，无法在操作过程中取消
2. **与 Web 标准 API 集成**：fetch、setTimeout 等原生 API 只支持 AbortSignal
3. **同步检查机制**：在循环或长时间运行的操作中，需要同步检查是否需要中断
4. **传递给底层 API**：LLM 客户端、HTTP 客户端等底层 API 需要接收 AbortSignal

**场景示例**：
- LLM 调用可能需要几十秒，用户可能在调用过程中请求取消
- 工具执行可能需要更长时间，需要能够中断
- HTTP 请求可能超时，需要能够取消

### 5.2 为什么需要 Result？

**原因**：
1. **类型安全的错误处理**：Result 类型强制处理错误，避免未捕获的异常
2. **函数式编程风格**：支持链式操作，代码更简洁
3. **避免异常作为控制流**：异常应该用于真正的错误，不应该用于控制流
4. **统一的错误处理方式**：所有操作都返回 Result，错误处理方式一致

**场景示例**：
- 验证结果：验证成功返回 Ok，验证失败返回 Err
- 业务逻辑：操作成功返回 Ok，操作失败返回 Err
- API 调用：调用成功返回 Ok，调用失败返回 Err

### 5.3 为什么需要 Error？

**原因**：
1. **存储详细的错误信息**：Error 类包含错误消息、堆栈、上下文等信息
2. **错误分类**：不同类型的错误有不同的处理方式
3. **错误链**：通过 cause 属性保留原始错误，便于追踪根本原因
4. **错误严重程度**：区分 ERROR、WARNING、INFO，不同的严重程度有不同的处理方式

**场景示例**：
- ValidationError：配置验证失败，需要修复配置
- ExecutionError：节点执行失败，需要重试或跳过
- NetworkError：网络错误，需要重试
- TimeoutError：超时错误，需要增加超时时间或重试

### 5.4 为什么需要 Event？

**原因**：
1. **状态通知**：通知状态变化，用于日志记录、监控、UI 更新
2. **异步非阻塞**：事件触发后立即返回，不等待处理完成
3. **解耦**：事件发布者和订阅者解耦，易于扩展
4. **全局监听**：可以全局监听所有事件，用于日志记录和监控

**场景示例**：
- 线程生命周期事件：记录线程开始、完成、暂停、恢复、取消
- 节点执行事件：记录节点开始、完成、失败
- 错误事件：记录错误发生，用于错误监控
- 系统事件：记录 Token 超限、变量变更等

### 5.5 四个组件如何协同工作？

**协同流程**：

1. **中断流程**：
   - 用户请求中断 → InterruptionManager 创建 AbortSignal → Signal 传递给 Executor → Executor 将 Signal 传递给底层 API → 底层 API 检测到 Signal 被中止 → 抛出 AbortError → Executor 捕获 AbortError → 转换为 Result → Result 包含 ThreadInterruptedException → 上层处理 Result → EventManager 触发事件

2. **执行流程**：
   - Executor 执行操作 → 使用 tryCatchAsyncWithSignal 包装 → 操作返回 Result → 检查 Result 状态 → 如果是 Err，检查错误类型 → 如果是 AbortError，转换为包含 ThreadInterruptedException 的 Result → 如果是其他错误，转换为包含相应 Error 的 Result → 如果是 Ok，返回包含成功值的 Result → 将 Result 转换为 ExecutionResult → 返回 ExecutionResult → 上层处理 ExecutionResult → EventManager 触发事件

3. **错误处理流程**：
   - 业务逻辑检测到错误 → 创建相应的 Error 子类实例 → 设置错误上下文和严重程度 → 设置错误链 → 抛出 Error 或返回包含 Error 的 Result → 上层捕获 Error 或处理 Result → ErrorService 记录错误 → EventManager 触发错误事件 → 监听器处理错误事件

## 六、总结

### 6.1 架构优势

1. **职责清晰**：Signal、Result、Error、Event 四个组件职责明确，互不干扰
2. **类型安全**：所有组件都提供类型安全的接口
3. **易于维护**：逻辑集中，职责明确，易于理解和扩展
4. **符合标准**：使用 Web 标准的 AbortSignal API
5. **向后兼容**：保持现有 API 不变，逐步迁移到新架构

### 6.2 关键改进

1. **消除重复代码**：统一的 Signal 处理逻辑和 Result 转换逻辑
2. **提高类型安全**：ThreadAbortSignal 提供完整的类型信息
3. **统一错误处理**：Result 模块提供一致的错误处理方式
4. **清晰的职责分离**：Signal 用于中断，Result 用于表示结果，Error 用于存储错误信息，Event 用于状态通知

### 6.3 实施路径

1. **阶段一**：创建 Signal 和 Result 模块
2. **阶段二**：迁移 Result 转换逻辑到 common-utils
3. **阶段三**：重构 Executor 使用新的 Signal 和 Result 模块
4. **阶段四**：清理和优化，删除未使用的代码

这个架构设计为长期演进提供了坚实的基础，同时保持了向后兼容性和清晰的职责分离。