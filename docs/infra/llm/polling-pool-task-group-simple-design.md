# LLM轮询池和任务组简化架构设计

## 概述

本文档基于Python实现的核心概念，设计了TypeScript版本的轮询池和任务组架构。设计遵循DDD原则，与现有TypeScript项目架构保持一致，采用一次性更改策略。

## 设计原则

### 简化原则
- **一次性更改**：不采用并行运行或渐进式迁移
- **最小化变更**：只实现核心功能，避免过度设计
- **直接替换**：新系统直接替换现有LLM调用机制

### 核心功能保留
- 轮询池管理：负载均衡、故障转移、健康检查
- 任务组管理：层级降级、熔断机制
- 包装器接口：统一的LLM调用接口

## 代码目录结构

### 领域层 (Domain Layer)

```
src/domain/llm/
├── entities/
│   ├── pool.ts                  # 轮询池实体
│   ├── task-group.ts            # 任务组实体
│   └── wrapper.ts               # 包装器实体
├── value-objects/
│   ├── pool-instance.ts         # 池实例值对象
│   ├── echelon.ts              # 层级值对象
│   └── rotation-strategy.ts    # 轮询策略值对象
├── interfaces/
│   ├── pool-manager.interface.ts
│   ├── task-group-manager.interface.ts
│   └── llm-wrapper.interface.ts
└── exceptions/
    ├── pool-exceptions.ts
    └── task-group-exceptions.ts
```

### 应用层 (Application Layer)

```
src/application/llm/
├── services/
│   ├── pool-service.ts          # 轮询池服务
│   ├── task-group-service.ts   # 任务组服务
│   └── wrapper-service.ts      # 包装器服务
└── dtos/
    ├── pool-dto.ts
    └── task-group-dto.ts
```

### 基础设施层 (Infrastructure Layer)

```
src/infrastructure/llm/
├── config/
│   ├── pool-config-loader.ts
│   └── task-group-config-loader.ts
├── managers/
│   ├── pool-manager.ts
│   └── task-group-manager.ts
└── wrappers/
    ├── pool-wrapper.ts
    └── task-group-wrapper.ts
```

## 核心组件设计

### 轮询池管理器 (Pool Manager)
**职责**：管理多个LLM实例的轮询和故障转移

**核心功能**：
- 实例轮询（轮询、随机策略）
- 健康检查和故障检测
- 实例状态管理
- 统计信息收集

### 任务组管理器 (Task Group Manager)
**职责**：管理层级模型配置和降级策略

**核心功能**：
- 层级模型选择
- 降级策略执行
- 熔断器管理
- 并发控制

### 包装器服务 (Wrapper Service)
**职责**：提供统一的LLM调用接口

**核心功能**：
- 请求路由到轮询池或任务组
- 错误处理和重试机制
- 响应格式统一
- 性能监控

## 配置设计

### 轮询池配置
```toml
[pools.fast_pool]
name = "fast_pool"
task_groups = ["fast_group"]

[pools.fast_pool.rotation]
strategy = "round_robin"

[pools.fast_pool.health_check]
interval = 30
failure_threshold = 3
```

### 任务组配置
```toml
[task_groups.fast_group]
name = "fast_group"

[task_groups.fast_group.echelon1]
models = ["openai:gpt-4o", "anthropic:claude-3-5-sonnet"]
priority = 1

[task_groups.fast_group.echelon2]
models = ["openai:gpt-4o-mini", "anthropic:claude-3-haiku"]
priority = 2
```

## 实施策略

### 一次性更改方案

#### 阶段1：核心组件实现（2周）
1. **实现领域层**
   - 创建轮询池、任务组实体
   - 定义核心接口
   - 实现值对象

2. **实现基础设施层**
   - 配置加载器
   - 管理器和包装器

#### 阶段2：集成和替换（1周）
1. **替换现有LLM调用**
   - 修改LLM节点执行器
   - 更新配置加载机制
   - 测试核心功能

2. **验证和优化**
   - 功能验证
   - 性能测试
   - 错误处理验证

### 关键更改点

#### 1. LLM节点执行器修改
- 将直接LLM客户端调用改为包装器调用
- 集成轮询池和任务组路由逻辑

#### 2. 配置系统扩展
- 添加轮询池和任务组配置支持
- 保持现有配置格式兼容性

#### 3. 依赖注入调整
- 注册新的服务和管理器
- 更新服务依赖关系

## 风险控制

### 技术风险
- **功能完整性**：确保新系统功能完整
- **性能影响**：避免性能下降
- **错误处理**：保持错误处理的一致性

### 缓解措施
- **充分测试**：在切换前进行全面测试
- **性能基准**：建立性能基准进行比较
- **回滚计划**：准备快速回滚方案

## 预期收益

### 功能提升
- 实现负载均衡和故障转移
- 提供层级降级机制
- 增强系统可靠性

### 架构改进
- 统一的LLM调用接口
- 更好的配置管理
- 更强的可扩展性

## 总结

本简化设计方案采用一次性更改策略，专注于核心功能的实现。通过清晰的目录结构和组件设计，确保系统的高效实现和稳定运行。方案避免了过度设计和复杂的迁移过程，直接替换现有LLM调用机制，降低了实施风险。