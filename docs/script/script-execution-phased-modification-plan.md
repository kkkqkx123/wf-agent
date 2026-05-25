# 脚本执行模块 - 分阶段修改方案

## 概述

本文档基于三份分析文档，制定分阶段代码修改方案：

| 文档 | 说明 |
|------|------|
| [目录布局设计](script-execution-directory-layout.md) | 完整目录结构 + 类型定义 + 架构分层 |
| [Leaf-flow 借鉴分析](leaf-flow-script-execution-analysis.md) | 蓝图化脚本、模板变量、执行器体系 |
| [交互式脚本集成分析](interactive-script-integration-analysis.md) | 有状态节点、状态化工具、接收策略 |

## 分阶段计划

```
Phase 1 (类型定义层) ───→ Phase 2 (核心引擎层) ───→ Phase 3 (集成层)
    基础类型              ScriptEngine + Executors      Handlers + Coordinators
    接口扩展              模板渲染 + 参数解析            配置文件解析
    Schema 定义           注册表扩展                      TerminalService 增强
```

---

## Phase 1：类型定义层（packages/types）

### 1.1 新增文件

| # | 文件 | 内容 |
|---|------|------|
| 1 | `packages/types/src/script/script-executor.ts` | ScriptExecutorConfig, ShellType, ExecutorMode |
| 2 | `packages/types/src/script/script-argument.ts` | ScriptArgument, ScriptArgumentType, ArgumentValueSource |
| 3 | `packages/types/src/script/script-flow.ts` | ScriptFlow, FlowBranch, FlowModuleRef |
| 4 | `packages/types/src/script/script-interactive.ts` | InteractiveScriptConfig, ScriptInteractionPoint, InteractionMode |

### 1.2 修改文件

| # | 文件 | 修改内容 |
|---|------|----------|
| 5 | `packages/types/src/script/script.ts` | Script interface 增加 `template?`, `arguments?`, `executor?` |
| 6 | `packages/types/src/script/index.ts` | 导出新增类型 |
| 7 | `packages/types/src/node/configs/execution-configs.ts` | ScriptNodeConfig 增加 `template?`, `executor?`, `flowId?` |
| 8 | `packages/types/src/node/runtime-node-types.ts` | RuntimeNodeType 增加 `INTERACTIVE_SCRIPT` |
| 9 | `packages/types/src/node/static-node-types.ts` | StaticNodeType 增加 `INTERACTIVE_SCRIPT` |
| 10 | `packages/types/src/interaction/user-interaction.ts` | UserInteractionOperationType 增加 `SCRIPT_INTERACTION` |

### 1.3 影响范围

- 无运行时依赖，仅类型定义
- Script interface 向后兼容（新增可选字段）
- 节点类型增加需要对应 handler 注册

---

## Phase 2：核心引擎层（sdk/core）

### 2.1 新增目录结构

```
sdk/core/script/
├── engine/
│   ├── index.ts
│   ├── script-engine.ts         # 核心引擎：模板渲染→执行→结果解析
│   ├── script-template.ts       # 模板渲染（复用 renderTemplate）
│   └── script-flow-engine.ts    # Flow 执行引擎（多步骤编排）
├── executors/
│   ├── index.ts
│   ├── base-executor.ts         # 抽象基类
│   ├── direct-executor.ts       # 直接执行（当前 executeOneOff）
│   ├── shared-executor.ts       # 共享 Shell 会话
│   └── pty-executor.ts          # 伪终端执行器（交互式）
├── resolvers/
│   ├── index.ts
│   ├── argument-resolver.ts     # 参数解析：默认值、类型校验
│   └── dynamic-resolver.ts      # 动态绑定：表达式、变量引用
└── index.ts
```

### 2.2 新增/修改文件

| # | 文件 | 说明 |
|---|------|------|
| 1 | `sdk/core/script/engine/script-template.ts` | 模板渲染，复用 `packages/common-utils` 的 `renderTemplate` |
| 2 | `sdk/core/script/engine/script-engine.ts` | 核心引擎，编排解析→执行→结果 |
| 3 | `sdk/core/script/engine/script-flow-engine.ts` | Flow 引擎，拓扑序执行多步骤 |
| 4 | `sdk/core/script/executors/base-executor.ts` | 抽象基类，定义 `execute(command, options)` 接口 |
| 5 | `sdk/core/script/executors/direct-executor.ts` | 调用 `TerminalService.executeOneOff()` |
| 6 | `sdk/core/script/executors/shared-executor.ts` | 调用 `TerminalService.createSession()` + `executeInSession()` |
| 7 | `sdk/core/script/executors/pty-executor.ts` | PTY 模式（预留，Phase 3 完整实现） |
| 8 | `sdk/core/script/resolvers/argument-resolver.ts` | 参数解析：默认值、类型校验、变量注入 |
| 9 | `sdk/core/script/resolvers/dynamic-resolver.ts` | 动态绑定：表达式评估、变量引用解析 |
| 10 | `sdk/core/script/index.ts` | 统一导出 |
| 11 | `sdk/core/registry/script-registry.ts` | 扩展：支持 template/argument 注册、Flow 注册 |
| 12 | `sdk/core/executors/script-executor.ts` | 增强：支持 template 渲染 + 参数注入 |

---

## Phase 3：集成层（Workflow + API + Services）

### 3.1 新增文件

| # | 文件 | 说明 |
|---|------|------|
| 1 | `sdk/api/shared/config/processors/script-flow.ts` | Flow 蓝图 TOML 解析+验证 |
| 2 | `sdk/api/shared/config/processors/script-interactive.ts` | 交互式脚本配置解析 |
| 3 | `sdk/workflow/execution/handlers/node-handlers/interactive-script-handler.ts` | 交互式脚本节点处理器 |
| 4 | `sdk/workflow/execution/coordinators/script-interaction-coordinator.ts` | 脚本交互协调器 |

### 3.2 修改文件

| # | 文件 | 修改内容 |
|---|------|----------|
| 5 | `sdk/workflow/execution/handlers/node-handlers/script-handler.ts` | 支持 template 模式 + executor 配置 + flowId 引用 |
| 6 | `sdk/services/terminal/terminal-service.ts` | 增加 `executePtySession()`, `executeWithInput()` |
| 7 | `sdk/api/shared/config/processors/index.ts` | 导出新增处理器 |
| 8 | 节点处理器 index.ts | 注册 `INTERACTIVE_SCRIPT` 处理器 |

---

## 执行顺序

```
Phase 1 (类型) ───完全独立，可先完成───→
                                              │
Phase 2 (引擎) ───依赖 Phase 1 的类型────────→
                                              │
Phase 3 (集成) ───依赖 Phase 1 + Phase 2─────→
```

---

## 详细任务列表

### Phase 1 任务（类型定义）

- [ ] P1.1: 创建 `script-executor.ts` - 执行器配置类型
- [ ] P1.2: 创建 `script-argument.ts` - 参数声明类型
- [ ] P1.3: 创建 `script-flow.ts` - Flow 蓝图类型
- [ ] P1.4: 创建 `script-interactive.ts` - 交互式脚本类型
- [ ] P1.5: 修改 `script.ts` - 扩展 Script interface
- [ ] P1.6: 修改 `script/index.ts` - 导出新增类型
- [ ] P1.7: 修改 `execution-configs.ts` - 扩展 ScriptNodeConfig
- [ ] P1.8: 修改 `runtime-node-types.ts` - 增加 INTERACTIVE_SCRIPT
- [ ] P1.9: 修改 `static-node-types.ts` - 增加 INTERACTIVE_SCRIPT
- [ ] P1.10: 修改 `user-interaction.ts` - 增加 SCRIPT_INTERACTION

### Phase 2 任务（核心引擎）

- [ ] P2.1: 创建 `script-template.ts` - 模板渲染引擎
- [ ] P2.2: 创建 `base-executor.ts` - 执行器抽象基类  
- [ ] P2.3: 创建 `direct-executor.ts` - 直接执行器
- [ ] P2.4: 创建 `shared-executor.ts` - 共享会话执行器
- [ ] P2.5: 创建 `pty-executor.ts` - PTY 执行器（桩）
- [ ] P2.6: 创建 `argument-resolver.ts` - 参数解析器
- [ ] P2.7: 创建 `dynamic-resolver.ts` - 动态绑定解析器
- [ ] P2.8: 创建 `script-engine.ts` - 核心引擎
- [ ] P2.9: 创建 `script-flow-engine.ts` - Flow 引擎
- [ ] P2.10: 修改 `script-registry.ts` - 扩展注册表
- [ ] P2.11: 修改 `script-executor.ts` - 增强执行器

### Phase 3 任务（集成层）

- [ ] P3.1: 增强 `script-handler.ts` - 支持 template/executor/flowId
- [ ] P3.2: 创建 `interactive-script-handler.ts` - 交互式脚本处理器
- [ ] P3.3: 创建 `script-interaction-coordinator.ts` - 脚本交互协调器
- [ ] P3.4: 创建 `script-flow.ts` 配置处理器
- [ ] P3.5: 创建 `script-interactive.ts` 配置处理器
- [ ] P3.6: 增强 `terminal-service.ts` - PTY/交互方法