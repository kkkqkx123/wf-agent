# 动态线程创建与回调机制设计方案

## 1. 设计概述

### 1.1 设计目标

本方案旨在实现主线程在运行时动态创建子线程并接收回调的功能，支持以下场景：
- 主线程执行过程中，通过工具调用创建基于其他工作流的子线程
- 子线程执行完成后，将结果回调给主线程
- 支持同步等待和异步回调两种模式
- 复用现有的TaskQueue和ThreadPool基础设施

### 1.2 核心设计原则

**扩展现有Thread概念，不引入新的复杂概念**
- 新增DYNAMIC_CHILD线程类型
- 新增DynamicThreadManager管理动态子线程
- 新增CallbackRegistry管理回调函数
- 充分利用现有的TaskQueue、ThreadPool、ThreadRegistry等组件

## 2. 目录结构设计

```
sdk/core/execution/
├── managers/
│   ├── dynamic-thread-manager.ts                    [新增]
│   ├── callback-registry.ts                         [新增]
│   ├── task-queue-manager.ts                        [已存在]
│   ├── thread-pool-manager.ts                       [已存在]
│   └── task-registry.ts                             [已存在]
├── types/
│   ├── task.types.ts                                [已存在]
│   └── dynamic-thread.types.ts                      [新增]
├── handlers/
│   └── tool-handlers/
│       └── create-thread-handler.ts                 [新增]
└── utils/
    └── callback-utils.ts                            [新增]
```

## 3. 核心组件设计

### 3.1 DynamicThreadManager（动态线程管理器）

**文件路径**：`sdk/core/execution/managers/dynamic-thread-manager.ts`

**实现方式**：有状态多实例，由ExecutionContext持有或由Handler创建

**文件关系**：
- 依赖：ThreadBuilder、TaskQueueManager、ThreadPoolManager、CallbackRegistry、ThreadRegistry、EventManager
- 被依赖：create-thread-handler、工具调用方
- 持有：CallbackRegistry实例、动态线程映射

**业务逻辑职责**：

动态线程管理器是动态子线程创建和管理的总协调器，负责：
- 创建动态子线程
- 管理子线程生命周期
- 处理子线程完成回调
- 提供同步和异步执行模式

**主要状态**：
- 动态线程映射：Map<threadId, DynamicThreadInfo>
- CallbackRegistry实例

**核心调用链**：

**创建动态子线程（同步模式）**：
```
工具调用create-thread-handler
  → create-thread-handler调用DynamicThreadManager.createDynamicThread()
    → 验证参数（workflowId是否存在）
    → 准备输入数据（从主线程ThreadContext获取）
    → 创建子线程ThreadContext
      → 调用ThreadBuilder.build()
      → 设置threadType为DYNAMIC_CHILD
      → 设置parentThreadId
    → 注册ThreadContext到ThreadRegistry
    → 建立父子线程关系
      → 调用mainThreadContext.registerChildThread(childId)
    → 创建Promise(resolve, reject)
    → 注册回调到CallbackRegistry
      → 调用CallbackRegistry.registerCallback(threadId, resolve, reject)
    → 提交到TaskQueueManager
      → 调用TaskQueueManager.submitSync(threadContext)
    → 等待Promise完成
    → 获取ThreadResult
    → 注销父子关系
    → 清理CallbackRegistry中的回调
    → 返回{ threadContext, threadResult, executionTime }
```

**创建动态子线程（异步模式）**：
```
工具调用create-thread-handler
  → create-thread-handler调用DynamicThreadManager.createDynamicThread()
    → 验证参数（workflowId是否存在）
    → 准备输入数据
    → 创建子线程ThreadContext
    → 注册ThreadContext到ThreadRegistry
    → 建立父子线程关系
    → 注册回调到CallbackRegistry
      → 调用CallbackRegistry.registerCallback(threadId, resolve, reject)
    → 提交到TaskQueueManager
      → 调用TaskQueueManager.submitAsync(threadContext)
    → 立即返回{ threadId, status: 'queued', message: 'Thread submitted' }
    → (后台执行完成)
    → TaskQueueManager触发handleTaskCompleted()
    → DynamicThreadManager接收回调
      → 调用CallbackRegistry.triggerCallback(threadId, result)
      → 触发DYNAMIC_THREAD_COMPLETED事件
    → 注销父子关系
    → 清理CallbackRegistry中的回调
```

**处理子线程完成**：
```
TaskQueueManager.handleTaskCompleted()
  → 调用DynamicThreadManager.handleThreadCompleted(threadId, result)
    → 从CallbackRegistry获取回调函数
    → 调用resolve(result)
    → 更新动态线程映射状态
    → 触发DYNAMIC_THREAD_COMPLETED事件
    → 注销父子关系
    → 清理CallbackRegistry
```

**处理子线程失败**：
```
TaskQueueManager.handleTaskFailed()
  → 调用DynamicThreadManager.handleThreadFailed(threadId, error)
    → 从CallbackRegistry获取回调函数
    → 调用reject(error)
    → 更新动态线程映射状态
    → 触发DYNAMIC_THREAD_FAILED事件
    → 注销父子关系
    → 清理CallbackRegistry
```

**取消动态线程**：
```
工具调用cancel-thread-handler
  → 调用DynamicThreadManager.cancelDynamicThread(threadId)
    → 调用TaskQueueManager.cancelTask(threadId)
    → 如果成功
      → 更新动态线程映射状态为CANCELLED
      → 触发DYNAMIC_THREAD_CANCELLED事件
      → 注销父子关系
      → 清理CallbackRegistry
    → 返回是否成功
```

**查询动态线程状态**：
```
工具调用get-thread-status-handler
  → 调用DynamicThreadManager.getThreadStatus(threadId)
    → 从动态线程映射获取DynamicThreadInfo
    → 返回线程状态信息
```

**关闭管理器**：
```
shutdown()
  → 调用CallbackRegistry.cleanup()
  → 清理动态线程映射
  → 取消所有运行中的线程
```

### 3.2 CallbackRegistry（回调注册表）

**文件路径**：`sdk/core/execution/managers/callback-registry.ts`

**实现方式**：有状态多实例，由DynamicThreadManager持有

**文件关系**：
- 依赖：无
- 被依赖：DynamicThreadManager
- 持有：回调函数映射

**业务逻辑职责**：

回调注册表负责管理动态子线程的回调函数，支持Promise-based回调和事件监听回调。

**主要状态**：
- 回调映射：Map<threadId, CallbackInfo>

**CallbackInfo结构**：
- threadId：线程ID
- resolve：Promise resolve函数
- reject：Promise reject函数
- eventListeners：事件监听器数组
- registeredAt：注册时间

**核心调用链**：

**注册回调**：
```
DynamicThreadManager.createDynamicThread()
  → CallbackRegistry.registerCallback(threadId, resolve, reject)
    → 创建CallbackInfo对象
    → 存入回调映射
    → 返回注册成功
```

**触发回调（成功）**：
```
DynamicThreadManager.handleThreadCompleted()
  → CallbackRegistry.triggerCallback(threadId, result)
    → 从回调映射获取CallbackInfo
    → 调用resolve(result)
    → 通知所有事件监听器
    → 从回调映射移除
```

**触发回调（失败）**：
```
DynamicThreadManager.handleThreadFailed()
  → CallbackRegistry.triggerErrorCallback(threadId, error)
    → 从回调映射获取CallbackInfo
    → 调用reject(error)
    → 通知所有事件监听器
    → 从回调映射移除
```

**添加事件监听器**：
```
DynamicThreadManager.addEventListener(threadId, eventType, listener)
  → CallbackRegistry.addEventListener(threadId, eventType, listener)
    → 从回调映射获取CallbackInfo
    → 添加监听器到eventListeners数组
```

**清理回调**：
```
DynamicThreadManager.shutdown()
  → CallbackRegistry.cleanup()
    → 遍历回调映射
    → 调用所有reject函数
    → 清空回调映射
```

### 3.3 create-thread-handler（创建线程处理器）

**文件路径**：`sdk/core/execution/handlers/tool-handlers/create-thread-handler.ts`

**实现方式**：纯函数导出

**文件关系**：
- 依赖：DynamicThreadManager、ThreadBuilder
- 被依赖：工具系统
- 持有：无

**业务逻辑职责**：

创建线程处理器负责接收工具调用请求，创建DynamicThreadManager，调用执行，处理返回结果。

**核心调用链**：

**处理创建线程请求**：
```
工具系统调用Handler
  → create-thread-handler(action, triggerId, executionContext)
    → 解析参数（workflowId、input、waitForCompletion等）
    → 获取主线程ThreadContext
      → 从ExecutionContext获取ThreadRegistry
      → 获取当前线程ID
      → 获取mainThreadContext
    → 准备输入数据
      → 构建input对象（包含triggerId、output、input）
    → 创建DynamicThreadManager
      → 实例化ThreadBuilder
      → 实例化CallbackRegistry
      → 实例化DynamicThreadManager（传入配置）
    → 创建线程对象
      → workflowId = workflowId
      → input = 准备好的输入数据
      → triggerId = triggerId
      → mainThreadContext = mainThreadContext
      → config = { waitForCompletion, timeout, recordHistory }
    → 调用manager.createDynamicThread(thread)
    → 处理返回结果
      → 如果是同步执行（返回ExecutedThreadResult）
        → 构建成功结果
          → message = "Dynamic thread execution completed"
          → workflowId = workflowId
          → input = 输入数据
          → output = result.threadContext.getOutput()
          → waitForCompletion = true
          → executed = true
          → completed = true
          → executionTime = result.executionTime
        → 返回ToolExecutionResult
      → 如果是异步执行（返回ThreadSubmissionResult）
        → 构建成功结果
          → message = "Dynamic thread submitted"
          → workflowId = workflowId
          → threadId = result.threadId
          → status = result.status
          → waitForCompletion = false
          → executed = true
          → completed = false
          → executionTime = result.executionTime
        → 返回ToolExecutionResult
    → 捕获异常
      → 构建失败结果
        → success = false
        → error = 错误信息
      → 返回ToolExecutionResult
```

### 3.4 callback-utils（回调工具函数）

**文件路径**：`sdk/core/execution/utils/callback-utils.ts`

**实现方式**：无状态，纯函数导出

**文件关系**：
- 依赖：无
- 被依赖：DynamicThreadManager、CallbackRegistry
- 持有：无

**业务逻辑职责**：

回调工具函数提供辅助功能，包括回调包装、错误处理、超时控制等。

**主要功能**：
- wrapCallback：包装回调函数，添加错误处理
- createTimeoutPromise：创建超时Promise
- mergeResults：合并多个子线程结果
- validateCallback：验证回调函数有效性

## 4. 类型定义

**文件路径**：`sdk/core/execution/types/dynamic-thread.types.ts`

**实现方式**：无状态，纯类型导出

**主要类型**：

- **DynamicThreadInfo接口**：动态线程信息
  - id：线程ID
  - threadContext：线程上下文
  - status：线程状态（QUEUED、RUNNING、COMPLETED、FAILED、CANCELLED、TIMEOUT）
  - submitTime：提交时间
  - startTime：开始执行时间
  - completeTime：完成时间
  - result：执行结果（成功时）
  - error：错误信息（失败时）
  - parentThreadId：父线程ID

- **CallbackInfo接口**：回调信息
  - threadId：线程ID
  - resolve：Promise resolve函数
  - reject：Promise reject函数
  - eventListeners：事件监听器数组
  - registeredAt：注册时间

- **ExecutedThreadResult接口**：同步执行结果
  - threadContext：线程上下文
  - threadResult：线程执行结果
  - executionTime：执行时间

- **ThreadSubmissionResult接口**：异步执行结果
  - threadId：线程ID
  - status：线程状态
  - message：状态消息
  - executionTime：执行时间

- **DynamicThreadConfig接口**：动态线程配置
  - waitForCompletion：是否等待完成
  - timeout：超时时间
  - recordHistory：是否记录历史

## 5. 与现有模块集成

### 5.1 TaskQueueManager集成

**集成方式**：
- DynamicThreadManager通过构造函数接收TaskQueueManager实例
- 调用TaskQueueManager.submitSync()提交同步任务
- 调用TaskQueueManager.submitAsync()提交异步任务
- 监听TaskQueueManager的任务完成和失败事件

**调用链**：
```
DynamicThreadManager.createDynamicThread()
  → TaskQueueManager.submitSync(threadContext)
    → 加入待执行队列
    → 触发processQueue()
    → 返回Promise
```

### 5.2 ThreadPoolManager集成

**集成方式**：
- 通过TaskQueueManager间接使用ThreadPoolManager
- TaskQueueManager调用ThreadPoolManager.allocateExecutor()获取执行器
- TaskQueueManager调用ThreadPoolManager.releaseExecutor()释放执行器

**调用链**：
```
TaskQueueManager.processQueue()
  → ThreadPoolManager.allocateExecutor()
    → 返回ThreadExecutor
  → ThreadExecutor.executeThread(threadContext)
  → ThreadPoolManager.releaseExecutor(executor)
```

### 5.3 ThreadRegistry集成

**集成方式**：
- DynamicThreadManager从ExecutionContext获取ThreadRegistry
- 子线程ThreadContext注册到ThreadRegistry
- 父子线程关系由ThreadContext管理

**调用链**：
```
DynamicThreadManager.createDynamicThread()
  → ThreadRegistry.register(childThreadContext)
  → mainThreadContext.registerChildThread(childThreadId)
```

### 5.4 EventManager集成

**集成方式**：
- 所有组件通过ExecutionContext获取EventManager
- 触发动态线程生命周期事件

**事件类型**：
- DYNAMIC_THREAD_SUBMITTED：线程已提交
- DYNAMIC_THREAD_STARTED：线程已开始
- DYNAMIC_THREAD_COMPLETED：线程已完成
- DYNAMIC_THREAD_FAILED：线程失败
- DYNAMIC_THREAD_CANCELLED：线程已取消
- DYNAMIC_THREAD_TIMEOUT：线程超时

### 5.5 ThreadCascadeManager集成

**集成方式**：
- DynamicThreadManager创建的子线程自动建立父子关系
- ThreadCascadeManager可以级联取消动态子线程
- ThreadCascadeManager可以等待动态子线程完成

**调用链**：
```
ThreadCascadeManager.cascadeCancel(parentThreadId)
  → 获取所有子线程ID
  → 遍历子线程
    → 调用DynamicThreadManager.cancelDynamicThread(childThreadId)
```

## 6. 数据流设计

### 6.1 同步执行数据流

```
1. 工具调用create-thread-handler
2. 创建DynamicThreadManager
3. 创建子线程ThreadContext
4. 注册到ThreadRegistry
5. 建立父子关系
6. 注册回调到CallbackRegistry
7. 提交到TaskQueueManager (submitSync)
8. TaskQueueManager加入待执行队列
9. TaskQueueManager调用processQueue()
10. TaskQueueManager调用ThreadPoolManager.allocateExecutor()
11. ThreadPoolManager返回ThreadExecutor
12. TaskQueueManager调用ThreadExecutor.executeThread(threadContext)
13. ThreadExecutor执行子工作流
14. 返回ThreadResult
15. TaskQueueManager调用handleTaskCompleted()
16. DynamicThreadManager接收回调
17. CallbackRegistry触发resolve(result)
18. 更新动态线程映射状态
19. 触发DYNAMIC_THREAD_COMPLETED事件
20. 注销父子关系
21. 清理CallbackRegistry
22. 返回结果给Handler
23. Handler返回结果给工具调用方
```

### 6.2 异步执行数据流

```
1. 工具调用create-thread-handler
2. 创建DynamicThreadManager
3. 创建子线程ThreadContext
4. 注册到ThreadRegistry
5. 建立父子关系
6. 注册回调到CallbackRegistry
7. 提交到TaskQueueManager (submitAsync)
8. TaskQueueManager加入待执行队列
9. TaskQueueManager返回threadId
10. Handler返回ThreadSubmissionResult
11. 工具调用方获得threadId
12. (后台) TaskQueueManager调用processQueue()
13. TaskQueueManager调用ThreadPoolManager.allocateExecutor()
14. ThreadPoolManager返回ThreadExecutor
15. TaskQueueManager调用ThreadExecutor.executeThread(threadContext)
16. ThreadExecutor执行子工作流
17. 返回ThreadResult
18. TaskQueueManager调用handleTaskCompleted()
19. DynamicThreadManager接收回调
20. CallbackRegistry触发resolve(result)
21. 更新动态线程映射状态
22. 触发DYNAMIC_THREAD_COMPLETED事件
23. 注销父子关系
24. 清理CallbackRegistry
25. (可选) 工具调用方通过事件或轮询获取结果
```

## 7. 错误处理

### 7.1 错误类型

- **DynamicThreadError**：动态线程创建失败、线程不存在
- **CallbackError**：回调注册失败、回调执行失败
- **TimeoutError**：线程执行超时
- **ThreadExecutionError**：子工作流不存在、执行失败

### 7.2 错误处理流程

```
线程执行失败
  → ThreadExecutor抛出异常
  → TaskQueueManager捕获异常
  → TaskQueueManager调用handleTaskFailed()
  → DynamicThreadManager接收失败回调
  → CallbackRegistry触发reject(error)
  → 更新动态线程映射状态为FAILED
  → 触发DYNAMIC_THREAD_FAILED事件
  → 同步执行：reject(error)传递给调用方
  → 异步执行：仅触发事件，调用方可通过事件或查询获取错误
```

## 8. 性能优化

### 8.1 线程池优化

- **动态扩缩容**：复用ThreadPoolManager的动态扩缩容机制
- **执行器复用**：执行完任务后不销毁，放回空闲队列供后续使用

### 8.2 回调优化

- **回调清理**：异步任务完成后立即清理CallbackRegistry中的回调
- **内存管理**：定期清理过期的回调记录

### 8.3 并发控制

- **队列限制**：复用TaskQueueManager的队列限制机制
- **超时控制**：支持设置线程执行超时时间

## 9. 向后兼容性

### 9.1 接口兼容

- DynamicThreadManager.createDynamicThread()保持现有接口风格
- 返回类型与TriggeredSubworkflowManager保持一致
- Handler返回值保持ToolExecutionResult结构

### 9.2 配置兼容

- waitForCompletion配置与TriggeredSubworkflowManager保持一致
- timeout配置与TriggeredSubworkflowManager保持一致
- 新增配置项有默认值

### 9.3 事件兼容

- 现有事件（THREAD_COMPLETED、THREAD_FAILED）保持不变
- 新增事件（DYNAMIC_THREAD_*）专门用于动态线程

## 10. 使用场景示例

### 10.1 场景一：主线程调用子工作流并等待结果

**场景描述**：
主线程执行过程中，需要调用一个子工作流处理数据，并等待子工作流返回结果后继续执行。

**调用链**：
```
主线程执行到工具节点
  → 调用create-thread工具
  → 传入workflowId和input
  → 设置waitForCompletion=true
  → DynamicThreadManager创建子线程
  → 提交到TaskQueueManager（同步模式）
  → 等待子线程完成
  → 获取子线程输出
  → 主线程继续执行
```

### 10.2 场景二：主线程调用子工作流并异步处理结果

**场景描述**：
主线程执行过程中，需要调用一个子工作流处理数据，但不等待结果，主线程继续执行，子线程完成后通过事件通知。

**调用链**：
```
主线程执行到工具节点
  → 调用create-thread工具
  → 传入workflowId和input
  → 设置waitForCompletion=false
  → DynamicThreadManager创建子线程
  → 提交到TaskQueueManager（异步模式）
  → 立即返回threadId
  → 主线程继续执行
  → (后台) 子线程执行完成
  → 触发DYNAMIC_THREAD_COMPLETED事件
  → 主线程监听事件并处理结果
```

### 10.3 场景三：主线程调用多个子工作流并合并结果

**场景描述**：
主线程执行过程中，需要调用多个子工作流并行处理数据，等待所有子线程完成后合并结果。

**调用链**：
```
主线程执行到工具节点
  → 调用create-thread工具（多次）
  → 每次传入不同的workflowId和input
  → 设置waitForCompletion=true
  → DynamicThreadManager创建多个子线程
  → 提交到TaskQueueManager（同步模式）
  → 等待所有子线程完成
  → 合并所有子线程输出
  → 主线程继续执行
```

## 11. 实施建议

### 11.1 实施顺序

**阶段一：基础设施**
1. 创建类型定义文件
2. 实现CallbackRegistry
3. 编写单元测试

**阶段二：核心组件**
1. 实现DynamicThreadManager
2. 实现create-thread-handler
3. 编写单元测试

**阶段三：集成**
1. 更新ThreadType枚举
2. 更新ThreadBuilder支持DYNAMIC_CHILD类型
3. 编写集成测试

**阶段四：优化**
1. 性能优化
2. 错误处理完善
3. 文档更新
4. 全面测试

### 11.2 风险控制

- 渐进式实施，先实现同步执行确保功能正确
- 再实现异步执行确保不破坏现有功能
- 保持现有测试通过，添加新的测试覆盖
- 分支开发，主分支保持稳定，出现问题可快速回滚

### 11.3 测试策略

**单元测试**：
- CallbackRegistry：测试回调注册、触发、清理
- DynamicThreadManager：测试线程创建、完成、失败、取消
- create-thread-handler：测试参数解析、结果处理

**集成测试**：
- 测试与TaskQueueManager的集成
- 测试与ThreadPoolManager的集成
- 测试与ThreadRegistry的集成
- 测试与EventManager的集成

**端到端测试**：
- 测试同步执行场景
- 测试异步执行场景
- 测试多线程并发场景
- 测试错误处理场景