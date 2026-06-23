# Core Executors Module

Core 执行器模块负责工作流级的执行编排，包括 LLM 调用、工具执行和脚本执行的协调。

## 架构

```
core/executors/
├── llm-executor.ts          # LLM 调用执行器
├── tool-call-executor.ts    # 工具调用执行器（批量）
└── script-executor.ts       # 脚本执行执行器
```

## 关键特点

### LLMExecutor
**职责**：协调 LLM 调用流程

- 参数准备和格式化
- LLM 调用编排
- 消息历史管理
- 工具集成
- 死循环检测

**依赖**：
- `core/llm/` - LLM 基础设施
- `core/validation/` - 消息验证
- `core/hooks/` - 生命周期钩子

### ToolCallExecutor
**职责**：协调工具批量执行

- 参数验证
- 权限检查（通过 ToolApprovalCoordinator）
- 批量工具执行
- 结果收集
- 错误处理

**依赖**：
- `services/tools/` - 工具执行器
- `core/coordinators/tool-approval-coordinator` - 审批协调
- `core/registry/tool-registry` - 工具注册

### ScriptExecutor
**职责**：协调脚本执行

- 模板渲染
- 执行模式选择
- 脚本执行
- 超时管理
- 结果解析

**依赖**：
- `services/script/` - 脚本引擎
- `services/terminal/` - Terminal 服务
- `services/sandbox/` - 沙盒环境

## 设计模式

所有执行器都遵循相同的设计模式：

```typescript
export class {Executor} {
  // 1. 验证阶段 - 验证输入
  async validate(): Promise<void>

  // 2. 准备阶段 - 准备执行环境
  async prepare(): Promise<void>

  // 3. 执行阶段 - 执行核心逻辑
  async execute(): Promise<Result>

  // 4. 清理阶段 - 清理资源
  async cleanup(): Promise<void>
}
```

## 职责边界

✅ **执行器负责**：
- 高级编排逻辑
- 流程协调
- 错误处理和恢复
- 日志和监控

❌ **执行器不负责**：
- 底层执行细节（→ services/）
- 状态管理（→ state-managers/）
- 审批和权限（→ coordinators/）
- 工具注册（→ registry/）

## 工作流

```
Graph/Agent
    ↓
core/executors/{Executor}
    ├─ validate() ──→ core/validation/
    ├─ prepare() ──→ core/coordinators/
    ├─ execute() ──→ services/{specific}/
    └─ cleanup() ──→ resource cleanup

services/{specific}/
    └─ 具体实现
```
