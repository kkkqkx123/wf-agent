# Script Execution Module - Directory Layout & Architecture Design

## 1. Design Principles

### 1.1 分层原则

遵循当前项目已有的三层架构：

```
types/           → 类型定义层（纯接口/类型，零依赖）
sdk/core/        → 核心逻辑层（无状态引擎、执行器）
sdk/services/    → 服务层（有状态资源管理、外部系统对接）
```

### 1.2 扩展原则

- 新增类型定义放在 `packages/types/` 下的现有目录结构中
- 核心执行逻辑放在 `sdk/core/executors/` 和 `sdk/core/script/`
- 执行器服务放在 `sdk/services/terminal/`（已存在）
- 蓝图配置解析放在 `sdk/api/shared/config/processors/`（已存在）
- Workflow 节点处理器放在 `sdk/workflow/execution/handlers/node-handlers/`（已存在）

---

## 2. 类型定义层（packages/types）

### 2.1 现有结构

```
packages/types/src/
├── node/configs/
│   ├── execution-configs.ts      # SCRIPT/LLM/TOOL_VISIBILITY 配置
│   ├── interaction-configs.ts    # USER_INTERACTION 配置
│   ├── loop-configs.ts           # LOOP_START/LOOP_END 配置
│   └── agent-loop-configs.ts     # AGENT_LOOP 配置
├── script/
│   ├── script.ts                 # Script 接口定义
│   └── script-security.ts        # 安全相关类型
└── interaction/
    ├── user-interaction.ts       # UserInteractionHandler 通用协议
    ├── tool-approval.ts          # 工具审批类型
    └── followup-question.ts      # 追问类型
```

### 2.2 新增/修改的类型

```
packages/types/src/
├── script/
│   ├── script.ts                     # [修改] 扩展 Script interface
│   │   ├── 增加 template?: string    # 命令模板（含 {{var}} 占位符）
│   │   ├── 增加 arguments?: ScriptArgument[]  # 参数声明
│   │   └── 增加 executor?: ScriptExecutorConfig  # 执行器配置
│   │
│   ├── script-executor.ts            # [新增] 执行器配置类型
│   │   ├── ShellType: "powershell" | "bash" | "cmd" | "auto"
│   │   ├── ExecutorMode: "direct" | "shared" | "pty"
│   │   └── ScriptExecutorConfig interface
│   │
│   ├── script-argument.ts            # [新增] 参数声明类型
│   │   ├── ScriptArgumentType: "string" | "number" | "boolean" | "file"
│   │   ├── ScriptArgument interface (key, type, default, required, etc.)
│   │   └── ArgumentValueSource: "static" | "variable" | "expression"
│   │
│   ├── script-flow.ts                # [新增] Flow 蓝图类型
│   │   ├── ScriptFlow interface (name, branches, modules)
│   │   ├── FlowBranch interface (key, modules, depends_on)
│   │   └── FlowModuleRef interface (key, args override)
│   │
│   └── script-interactive.ts         # [新增] 交互式脚本相关类型
│       ├── InteractiveScriptConfig interface
│       ├── ScriptInteractionPoint (pause point with prompt)
│       └── InteractionMode: "blocking" | "llm-assisted" | "hybrid"
│
├── node/configs/
│   └── execution-configs.ts          # [修改] 扩展 ScriptNodeConfig
│       ├── 增加 template?: string     # 内联模板（替代 scriptName）
│       ├── 增加 executor?: ScriptExecutorConfig
│       └── 增加 flowId?: string       # 引用 Flow 蓝图
│
└── interaction/
    └── user-interaction.ts           # [修改] 扩展 UserInteractionOperationType
        └── 增加 "SCRIPT_INTERACTION"  # 脚本交互类型
```

---

## 3. 核心逻辑层（sdk/core）

### 3.1 新增目录结构

```
sdk/core/
├── script/                           # [新增] 脚本引擎模块
│   ├── __tests__/
│   │   ├── script-engine.test.ts
│   │   ├── script-template.test.ts
│   │   └── script-flow-engine.test.ts
│   │
│   ├── engine/                       # 脚本执行引擎
│   │   ├── index.ts
│   │   ├── script-engine.ts          # 核心引擎：模板渲染→执行→结果解析
│   │   ├── script-template.ts        # 模板渲染（利用 renderTemplate）
│   │   └── script-flow-engine.ts     # Flow 执行引擎（多步骤编排）
│   │
│   ├── executors/                    # 脚本执行器适配层
│   │   ├── index.ts
│   │   ├── base-executor.ts          # 抽象基类
│   │   ├── direct-executor.ts        # 直接执行（当前 executeOneOff）
│   │   ├── shared-executor.ts        # 共享 Shell 会话
│   │   └── pty-executor.ts           # 伪终端执行器（交互式）
│   │
│   ├── resolvers/                    # 参数解析
│   │   ├── index.ts
│   │   ├── argument-resolver.ts      # 参数解析：默认值、类型校验
│   │   └── dynamic-resolver.ts       # 动态绑定：表达式、变量引用
│   │
│   └── index.ts
│
├── script/                           # [修改] 增强现有 ScriptRegistry
│   ├── index.ts
│   └── script-registry.ts            # 增加 Flow 注册、模板注册
│
└── executors/
    └── script-executor.ts            # [修改] 增强现有 ScriptExecutor
```

### 3.2 核心模块职责

#### 3.2.1 ScriptEngine

```
ScriptEngine
  ├── resolve(): 解析脚本定义（参数绑定、模板渲染）
  ├── execute(): 调用 executor 执行最终命令
  └── parseOutput(): 解析执行结果（stdout → 结构化数据）

执行流程：
1. 接收 Script 定义 + 运行时变量 context
2. 调用 ArgumentResolver 解析参数（注入变量、计算默认值）
3. 调用 ScriptTemplate 渲染命令模板（{{var}} → 实际值）
4. 选择合适的 Executor（direct/shared/pty）
5. 执行命令，等待结果
6. 解析 stdout/stderr，返回结构化结果
```

#### 3.2.2 Shell Executors

```
BaseExecutor (abstract)
  ├── execute(command: string, options): Promise<ExecuteResult>
  │
  ├── DirectExecutor
  │   └── 使用 TerminalService.executeOneOff()
  │       └── 当前已有实现，无状态模式
  │
  ├── SharedExecutor
  │   └── 使用 TerminalService.createSession() + executeInSession()
  │       └── 共享 Shell 进程，保持 cd/环境变量
  │       └── 对应 Leaf-flow SharedExecutor
  │
  └── PtyExecutor
      └── 使用 node-pty 创建伪终端
          └── 支持交互式命令（需要用户输入）
          └── 对应 Leaf-flow PtyExecutor
```

#### 3.2.3 ScriptFlowEngine

```
ScriptFlowEngine
  ├── execute(flow: ScriptFlow, context): Promise<FlowResult>
  │
  ├── 按拓扑序执行 branches
  │   ├── 无依赖的 branches 并行执行
  │   └── 有依赖的 branches 串行
  │
  ├── 每个 branch 内顺序执行 modules
  │   ├── 查找 Script 定义（从注册表）
  │   ├── 应用参数覆盖（args override）
  │   └── 调用 ScriptEngine 执行
  │
  └── 收集结果，返回结构化输出
```

---

## 4. 配置处理层（sdk/api/shared/config）

### 4.1 新增处理器

```
sdk/api/shared/config/
├── processors/
│   ├── script-flow.ts                # [新增] Flow 蓝图 TOML 解析+验证
│   └── script-interactive.ts         # [新增] 交互式脚本配置解析
│
└── parsers.ts                        # [修改] 增加 parseScriptFlow 导出
```

### 4.2 配置类型

```
sdk/api/shared/config/types.ts        # [修改] 增加解析后的 Flow/Interactive 类型
```

---

## 5. Workflow 节点处理器（sdk/workflow）

### 5.1 新增节点处理器

```
sdk/workflow/execution/handlers/
├── node-handlers/
│   ├── script-handler.ts             # [修改] 增强现有 script-handler
│   │   ├── 支持 template 模式（内联模板 + 变量注入）
│   │   ├── 支持 executor 配置（direct/shared/pty）
│   │   └── 支持 flowId 引用（委派给 ScriptFlowEngine）
│   │
│   ├── interactive-script-handler.ts # [新增] 交互式脚本节点处理器
│   │   ├── 处理有状态交互式脚本节点
│   │   ├── 集成 UserInteractionHandler 获取用户输入
│   │   └── 支持挂起/恢复执行（内部状态保持）
│   │
│   └── index.ts                      # [修改] 注册新处理器
│
└── coordinators/
    └── script-interaction-coordinator.ts # [新增] 脚本交互协调器
        ├── 管理交互式脚本的生命周期
        ├── 协调 UserInteractionHandler + LLM 调用
        └── 支持断点续执行
```

### 5.2 现有文件的修改

```
sdk/workflow/evaluation/
├── expression-evaluator.ts           # [使用] 已有表达式引擎，无需修改
├── condition-evaluator.ts            # [使用] 已有条件评估器
└── path-resolver.ts                  # [使用] 已有路径解析器
```

---

## 6. 服务层（sdk/services）

### 6.1 现有 TerminalService 增强

```
sdk/services/terminal/
├── terminal-service.ts               # [修改] 增强 TerminalService
│   ├── 增加 executeWithInput()        # 交互式执行（发送输入+接收输出）
│   ├── 增加 executePtySession()       # PTY 会话模式
│   └── 增加 getSessionOutput()        # 增量读取输出
│
├── shell-detector.ts                 # [使用] 已有 Shell 检测
├── terminal-registry.ts              # [使用] 已有会话注册表
└── types.ts                          # [使用] 已有类型定义
```

---

## 7. 完整目录结构一览

```
packages/types/src/
├── script/
│   ├── script.ts                          # Script interface 扩展
│   ├── script-executor.ts                 # [NEW] 执行器配置
│   ├── script-argument.ts                 # [NEW] 参数声明
│   ├── script-flow.ts                     # [NEW] Flow 蓝图
│   └── script-interactive.ts              # [NEW] 交互式脚本类型
│
├── node/configs/
│   └── execution-configs.ts               # [MOD] ScriptNodeConfig 扩展
│
└── interaction/
    └── user-interaction.ts                # [MOD] 增加 SCRIPT_INTERACTION

sdk/core/script/                           # [NEW] 脚本引擎模块
├── engine/
│   ├── script-engine.ts                   # 核心引擎
│   ├── script-template.ts                 # 模板渲染
│   └── script-flow-engine.ts              # Flow 引擎
├── executors/
│   ├── base-executor.ts                   # 抽象基类
│   ├── direct-executor.ts                 # 直接执行
│   ├── shared-executor.ts                 # 共享 Shell
│   └── pty-executor.ts                    # 伪终端
├── resolvers/
│   ├── argument-resolver.ts               # 参数解析
│   └── dynamic-resolver.ts                # 动态绑定
└── index.ts

sdk/core/registry/
└── script-registry.ts                     # [MOD] 扩展注册表

sdk/core/executors/
└── script-executor.ts                     # [MOD] 增强执行器

sdk/api/shared/config/processors/
├── script-flow.ts                         # [NEW] Flow 解析
└── script-interactive.ts                  # [NEW] 交互脚本解析

sdk/workflow/execution/handlers/node-handlers/
├── script-handler.ts                      # [MOD] 增强脚本处理器
├── interactive-script-handler.ts          # [NEW] 交互脚本处理器
└── index.ts                               # [MOD] 注册新类型

sdk/workflow/execution/coordinators/
└── script-interaction-coordinator.ts      # [NEW] 脚本交互协调器

sdk/services/terminal/
├── terminal-service.ts                    # [MOD] 增加 PTY/交互方法
├── shell-detector.ts                      # [USE]
├── terminal-registry.ts                   # [USE]
└── types.ts                               # [USE]
```

---

## 8. 架构设计图

### 8.1 整体分层

```
┌─────────────────────────────────────────────────────────────┐
│                  Workflow Graph Layer                        │
│  (sdk/workflow/execution/handlers/node-handlers/)           │
│                                                              │
│  SCRIPT handler ──→ script-handler.ts                        │
│  INTERACTIVE_SCRIPT handler ──→ interactive-script-handler   │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│              Script Engine Layer                             │
│  (sdk/core/script/)                                         │
│                                                              │
│  ScriptEngine ──→ 模板渲染 → 参数注入 → 选择执行器 → 执行    │
│  ScriptFlowEngine ──→ 多步骤编排（Flow 蓝图）                 │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│           Executor Adapter Layer                             │
│  (sdk/core/script/executors/)                               │
│                                                              │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                   │
│  │ Direct   │  │ Shared   │  │  PTY     │                   │
│  │ Executor │  │ Executor │  │ Executor │                   │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘                   │
│       │              │              │                        │
└───────┼──────────────┼──────────────┼────────────────────────┘
        │              │              │
        ▼              ▼              ▼
┌─────────────────────────────────────────────────────────────┐
│           Terminal Service Layer                             │
│  (sdk/services/terminal/)                                    │
│                                                              │
│  executeOneOff()  createSession()  executePtySession()       │
│                    executeInSession()                        │
└───────────────────────────┬─────────────────────────────────┘
                            │
                            ▼
                    操作系统子进程
```

### 8.2 节点类型扩展

```
RuntimeNodeType 新增:
  "INTERACTIVE_SCRIPT"      # 交互式脚本节点（有状态）

已有相关类型:
  "SCRIPT"                  # 普通脚本节点（无状态）
  "USER_INTERACTION"        # 用户交互节点（无状态）
  "LOOP_START" / "LOOP_END" # 循环节点
  "AGENT_LOOP"              # Agent 循环节点（有状态）
```

### 8.3 数据流

```
Workflow 变量上下文
        │
        ▼
ScriptNodeConfig
  ├── scriptName     → ScriptRegistry → ScriptEngine
  ├── template       → ScriptTemplate → 渲染后命令 → Executor
  ├── arguments      → ArgumentResolver → 变量注入
  └── executor       → 选择 Direct|Shared|PTY
        │
        ▼
TerminalService
  ├── executeOneOff()     → DirectExecutor
  ├── executeInSession()  → SharedExecutor
  └── executePtySession() → PtyExecutor
        │
        ▼
ExecuteResult
  ├── stdout / stderr
  ├── exitCode
  └── parsed output (结构化)
        │
        ▼
Workflow 变量更新 + 节点输出
```

---

## 9. 文件改动汇总

| 改动类型 | 文件 | 说明 |
|---------|------|------|
| **新增** | `packages/types/src/script/script-executor.ts` | 执行器配置类型 |
| **新增** | `packages/types/src/script/script-argument.ts` | 参数声明类型 |
| **新增** | `packages/types/src/script/script-flow.ts` | Flow 蓝图类型 |
| **新增** | `packages/types/src/script/script-interactive.ts` | 交互式脚本类型 |
| **修改** | `packages/types/src/script/script.ts` | 扩展 Script interface |
| **修改** | `packages/types/src/node/configs/execution-configs.ts` | 扩展 ScriptNodeConfig |
| **修改** | `packages/types/src/interaction/user-interaction.ts` | 增加 SCRIPT_INTERACTION |
| **修改** | `packages/types/src/node/runtime-node-types.ts` | 增加 INTERACTIVE_SCRIPT |
| **修改** | `packages/types/src/node/static-node-types.ts` | 增加 INTERACTIVE_SCRIPT |
| **新增** | `sdk/core/script/engine/script-engine.ts` | 脚本执行引擎 |
| **新增** | `sdk/core/script/engine/script-template.ts` | 模板渲染 |
| **新增** | `sdk/core/script/engine/script-flow-engine.ts` | Flow 执行引擎 |
| **新增** | `sdk/core/script/executors/base-executor.ts` | 执行器抽象基类 |
| **新增** | `sdk/core/script/executors/direct-executor.ts` | 直接执行器 |
| **新增** | `sdk/core/script/executors/shared-executor.ts` | 共享会话执行器 |
| **新增** | `sdk/core/script/executors/pty-executor.ts` | 伪终端执行器 |
| **新增** | `sdk/core/script/resolvers/argument-resolver.ts` | 参数解析器 |
| **新增** | `sdk/core/script/resolvers/dynamic-resolver.ts` | 动态绑定解析器 |
| **修改** | `sdk/core/registry/script-registry.ts` | 扩展注册表 |
| **修改** | `sdk/core/executors/script-executor.ts` | 增强执行器 |
| **新增** | `sdk/api/shared/config/processors/script-flow.ts` | Flow 配置解析 |
| **新增** | `sdk/api/shared/config/processors/script-interactive.ts` | 交互脚本解析 |
| **修改** | `sdk/workflow/execution/handlers/node-handlers/script-handler.ts` | 增强脚本处理器 |
| **新增** | `sdk/workflow/execution/handlers/node-handlers/interactive-script-handler.ts` | 交互脚本处理器 |
| **新增** | `sdk/workflow/execution/coordinators/script-interaction-coordinator.ts` | 脚本交互协调器 |
| **修改** | `sdk/services/terminal/terminal-service.ts` | 增加 PTY/交互方法 |