# SDK API层架构设计

## 一、设计原则

### 1. 模块暴露原则
- **直接暴露**: 接受外部配置的核心管理模块
- **间接暴露**: 通过主模块暴露相关功能
- **不暴露**: 内部执行逻辑和协调模块

### 2. 配置边界
- **外部可配置**: 注册表、管理器、配置类
- **内部自治**: 执行器、协调器、构建器

### 3. 依赖规则
- API层只依赖Core层的外部配置模块
- 内部模块依赖由Core层自行管理

---

## 二、模块分类

### 外部配置模块（API直接暴露）

#### 1. 主执行模块
**ThreadExecutor** - 工作流执行入口
- 功能: 执行workflow生成thread
- 配置: workflowRegistry、执行选项
- 暴露方式: 直接暴露

#### 2. 注册管理模块
**WorkflowRegistry** - 工作流管理
- 功能: 注册、查询、更新、删除工作流定义
- 配置: 版本管理策略
- 暴露方式: 直接暴露

**ThreadRegistry** - 线程管理
- 功能: 查询和管理执行中的线程
- 配置: 无
- 暴露方式: 直接暴露

**ToolRegistry** - 工具管理
- 功能: 注册和查询工具定义
- 配置: 无
- 暴露方式: 通过ToolService暴露

#### 3. 服务模块
**ToolService** - 工具执行服务
- 功能: 统一工具调用接口
- 配置: 工具定义、执行选项
- 暴露方式: 直接暴露

**LLMWrapper** - LLM调用服务
- 功能: 统一LLM调用接口
- 配置: Profile、请求参数
- 暴露方式: 直接暴露

#### 4. 配置管理模块
**ProfileManager** - LLM配置管理
- 功能: Profile注册、查询、设置
- 配置: Profile定义
- 暴露方式: 直接暴露或集成到LLMWrapper

#### 5. 状态管理模块
**CheckpointManager** - 检查点管理
- 功能: 状态快照、恢复、查询
- 配置: 存储实现、自动检查点间隔
- 暴露方式: 直接暴露

**VariableManager** - 变量管理
- 功能: 线程变量CRUD
- 配置: 无
- 暴露方式: 直接暴露

#### 6. 事件管理模块
**EventManager** - 事件监听
- 功能: 全局事件订阅和分发
- 配置: 无
- 暴露方式: 仅暴露全局事件方法

#### 7. 验证模块
**WorkflowValidator** - 工作流验证
- 功能: 验证工作流定义合法性
- 配置: 验证规则
- 暴露方式: 直接暴露

**NodeValidator** - 节点验证
- 功能: 验证节点配置合法性
- 配置: 验证规则
- 暴露方式: 直接暴露

---

### 内部模块（不暴露到API）

#### 1. 执行协调类
- ThreadCoordinator - Fork/Join/Copy协调
- ThreadBuilder - Thread构建
- ThreadLifecycleManager - 生命周期管理
- Router - 条件路由

#### 2. 执行器类
- LLMExecutor - LLM执行协调
- ConversationManager - 消息上下文管理
- NodeExecutor - 节点执行（各类）
- TriggerExecutor - 触发器执行（各类）

#### 3. 上下文类
- ExecutionContext - 依赖注入容器
- WorkflowContext - 工作流上下文
- ThreadContext - 线程上下文

---

## 三、API功能架构

### 一级API（核心功能）

#### 1. 执行API
- 工作流执行（创建并执行线程）
- 线程执行（执行已创建的线程）
- 批量执行

#### 2. 控制API
- 线程状态控制（暂停、恢复、停止、取消）
- 线程触发器操作（暂停点、恢复点、跳过节点、设置变量、发送通知）

#### 3. 注册API
- 工作流注册与查询
- 线程查询
- 工具注册与查询

#### 4. 配置API
- LLM Profile管理
- 工具配置
- SDK全局配置

#### 5. 状态API
- 检查点管理
- 变量管理
- 状态查询

#### 6. 事件API
- 事件监听
- 事件查询

#### 7. 验证API
- 工作流验证
- 节点验证

### 二级API（组合功能）

#### 1. 监控API
- 执行统计
- 性能监控
- 日志查询

#### 2. 模板API
- 工作流模板管理
- 模板实例化

#### 3. 批量API
- 批量执行
- 批量工具调用
- 批量LLM调用

---

## 四、关键设计决策

### 1. ThreadCoordinator的处理
- **定位**: 内部协调模块
- **暴露方式**: 不直接暴露
- **使用方式**: 通过ThreadExecutor内部调用
- **事件机制**: 通过内部事件与ThreadExecutor解耦

### 2. Fork/Join/Copy的API设计
- **Fork**: ThreadExecutor.executeWorkflow() 自动处理
- **Join**: ThreadExecutor.executeWorkflow() 自动处理
- **Copy**: ThreadExecutor.executeWorkflow() 自动处理
- **暴露方式**: 通过执行选项配置，不暴露独立API

### 3. ConversationManager的处理
- **定位**: 内部消息管理
- **暴露方式**: 不直接暴露
- **使用方式**: LLMExecutor内部使用
- **配置方式**: 通过LLM节点配置传递

### 4. 事件监听边界
- **全局事件**: THREAD_STARTED、NODE_COMPLETED等对外暴露
- **内部事件**: FORK_REQUEST、JOIN_REQUEST等不暴露
- **暴露方式**: EventManager仅暴露on()方法，不暴露onInternal()

---

## 五、模块依赖关系

```
API层
  ├── ThreadExecutor（主入口）
  ├── WorkflowRegistry（工作流管理）
  ├── ThreadRegistry（线程管理）
  ├── ToolService（工具服务）
  ├── LLMWrapper（LLM调用）
  ├── ProfileManager（Profile管理）
  ├── CheckpointManager（检查点）
  ├── VariableManager（变量管理）
  ├── EventManager（事件监听）
  └── WorkflowValidator（验证）

Core层（外部模块）
  ├── ThreadExecutor
  │   ├── ThreadBuilder（内部）
  │   ├── ThreadCoordinator（内部）
  │   ├── ThreadLifecycleManager（内部）
  │   └── EventManager
  ├── WorkflowRegistry
  │   └── WorkflowValidator
  ├── ThreadRegistry
  ├── ToolService
  │   ├── ToolRegistry
  │   └── 各类ToolExecutor（内部）
  ├── LLMWrapper
  │   ├── ProfileManager
  │   └── ClientFactory
  ├── ProfileManager
  ├── CheckpointManager
  │   ├── ThreadRegistry
  │   └── VariableManager
  ├── VariableManager
  ├── EventManager
  ├── WorkflowValidator
  └── NodeValidator

Core层（内部模块）
  ├── ThreadBuilder
  ├── ThreadCoordinator
  ├── ThreadLifecycleManager
  ├── Router
  ├── LLMExecutor
  ├── ConversationManager
  ├── ExecutionContext
  ├── WorkflowContext
  ├── ThreadContext
  ├── NodeExecutor（各类）
  └── TriggerExecutor（各类）
```

---

## 六、API设计规范

### 1. 命名规范
- 使用完整语义化的方法名
- 避免使用Enhanced/Unified等模糊词汇
- 采用功能导向命名

### 2. 参数规范
- 使用Options对象封装可选参数
- 必要参数作为独立参数
- 支持部分更新模式

### 3. 返回规范
- 异步方法返回Promise
- 查询方法返回null表示未找到
- 批量操作返回结果数组

### 4. 错误规范
- 使用特定Error类型
- 提供详细的错误信息
- 支持错误链追踪

---

## 七、扩展性设计

### 1. 插件机制
- 支持自定义工具类型
- 支持自定义存储实现
- 支持自定义事件处理器

### 2. 中间件机制
- 执行前后钩子
- 事件处理中间件
- 错误处理中间件

### 3. 配置扩展
- 支持自定义验证规则
- 支持自定义序列化
- 支持自定义日志

---

## 八、性能考虑

### 1. 缓存策略
- 工作流定义缓存
- Profile配置缓存
- 工具定义缓存

### 2. 连接池
- HTTP连接池
- LLM客户端连接池

### 3. 批量处理
- 批量注册
- 批量查询
- 批量执行

### 4. 异步处理
- 异步事件分发
- 异步检查点创建
- 异步日志记录
