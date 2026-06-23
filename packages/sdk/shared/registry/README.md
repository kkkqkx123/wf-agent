# Core Registry Module

Registry 模块负责管理 SDK 中的各种资源（工具、技能、脚本、事件等），提供统一的注册、查询和生命周期管理接口。

## 架构

```
core/registry/
├── tool-registry.ts         # 工具注册表
├── skill-registry.ts        # 技能注册表
├── script-registry.ts       # 脚本注册表
├── event-registry.ts        # 事件注册表
└── index.ts
```

## 关键组件

### ToolRegistry
**职责**：管理所有工具和执行器

```typescript
// 注册工具
registry.registerTool(tool, { executor: 'rest' })

// 获取工具
const tool = registry.getTool('http-get')

// 获取执行器
const executor = registry.getExecutor('rest')
```

**支持的执行器**：
- REST - HTTP/HTTPS API
- Stateless - JavaScript 函数
- Stateful - 有状态 JavaScript
- Builtin - 内置工具
- MCP - MCP 服务器工具

### SkillRegistry
**职责**：管理可加载的技能资源

```typescript
// 加载技能
await registry.loadSkill('path/to/skill')

// 列出可用技能
const skills = registry.listSkills()
```

### ScriptRegistry
**职责**：管理脚本资源

```typescript
// 注册脚本引擎
registry.registerEngine('flow', scriptFlowEngine)

// 获取脚本
const script = registry.getScript('script-id')
```

### EventRegistry
**职责**：管理事件处理器

```typescript
// 注册事件处理器
registry.on('execution.started', handler)

// 发送事件
registry.emit('execution.completed', { result })
```

## 生命周期

```
初始化 (Init)
    ↓
注册资源 (Register)
    ├─ tools
    ├─ skills
    ├─ scripts
    └─ events
    ↓
查询和使用 (Query & Use)
    ├─ getTool()
    ├─ getExecutor()
    ├─ getScript()
    └─ getHandler()
    ↓
卸载和清理 (Unload & Cleanup)
    ├─ unloadSkill()
    └─ unregister()
```

## 工具注册流程

```
1. 创建 Tool 定义
   {
     id: 'http-get',
     type: 'REST',
     description: 'Fetch HTTP resource',
     parameters: { ... }
   }
   ↓
2. 选择执行器类型
   'REST' → RestExecutor
   'Stateless' → StatelessExecutor
   'MCP' → McpExecutor
   ↓
3. 注册到 ToolRegistry
   registry.registerTool(tool, { executor: 'REST' })
   ↓
4. 使用工具
   const executor = registry.getExecutor('REST')
   const result = await executor.execute(tool, params)
```

## 架构关系

```
ToolRegistry
    ├── 导入 services/tools/executors/*
    │   ├─ RestExecutor
    │   ├─ StatelessExecutor
    │   ├─ StatefulExecutor
    │   ├─ BuiltinExecutor
    │   └─ McpExecutor
    └── 导入 services/tools/core/interfaces
        └─ IToolExecutor

SkillRegistry
    └── 导入 services/skill-loader/
        └─ HostSkillLoader

ScriptRegistry
    └── 导入 services/script/engine/
        ├─ ScriptEngine
        ├─ ScriptFlowEngine
        └─ ScriptTemplateEngine

EventRegistry
    ├── 导入 core/types/
    │   └─ Event types
    └── 存储 handlers map
```

## 使用建议

### 预注册工具
```typescript
const registry = new ToolRegistry()

// 注册标准工具
registry.registerTool(
  {
    id: 'fetch',
    type: 'REST',
    description: 'Fetch HTTP resource',
    parameters: { properties: { url: { type: 'string' } } }
  },
  { executor: 'REST' }
)
```

### 动态加载技能
```typescript
const registry = new SkillRegistry()

// 从文件系统加载
await registry.loadSkill('./skills/data-processing.js')

// 查询已加载的技能
const skills = registry.listSkills()
```

### 管理事件
```typescript
const registry = new EventRegistry()

// 注册处理器
registry.on('tool.executed', (event) => {
  console.log(`Tool ${event.toolId} executed`)
})

// 发送事件
registry.emit('tool.executed', { toolId: 'fetch' })
```

## 设计原则

1. **单一真源** - Registry 是所有资源的唯一来源
2. **延迟加载** - 技能和脚本按需加载
3. **可插拔** - 新执行器可以动态注册
4. **类型安全** - 完整的 TypeScript 支持
5. **事件驱动** - 生命周期事件通知系统
