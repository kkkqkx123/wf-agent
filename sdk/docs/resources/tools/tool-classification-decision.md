# 工具分类架构决策文档

## 决策概述

本文档记录了对 `sdk/resources/predefined/tools` 目录下工具分类的分析结论与决策。

## 分析背景

针对以下问题进行了代码结构分析：

1. `use-mcp` 是否应该从 `stateless/interaction` 迁移到 `stateful` 目录？
2. `skill` 和 `run-slash-command` 是否应该视为 builtin 工具？

## 核心原则

工具分类的核心标准是：**工具自身是否维护状态**，而非它调用的服务是否有状态。

| 分类 | 判定标准 | 典型特征 |
|------|----------|----------|
| **stateless** | 工具本身不维护状态 | 纯函数、无实例管理、无生命周期 |
| **stateful** | 工具自身维护状态 | 有 Manager/Instance 类、维护 Map/状态对象 |
| **builtin** | 工作流引擎内部控制工具 | 需要 `BuiltinToolExecutionContext`、依赖 DI 容器 |

## 分析结论

### 1. use-mcp 保持 stateless 分类

**决策：不迁移，保持现状**

**理由：**
- `use-mcp` 是一个纯转发层工具，本身不维护任何状态
- MCP 连接的生命周期管理在 SDK 核心层处理，不在工具内部
- 对比 stateful 工具（如 `backend-shell`、`session-note`），它们都有明确的 Manager 类维护状态：
  - `BackendShellManager` 维护 `shells: Map<string, BackendShell>`
  - `SessionNoteInstance` 维护 `notes: NoteEntry[]` 和 `loaded` 状态
- `use-mcp` 只是将调用转发给 MCP 服务器，无自身状态管理

**代码参考：**
- `sdk/resources/predefined/tools/stateless/interaction/use-mcp/handler.ts`
- `sdk/resources/predefined/tools/stateful/shell/backend-shell/handler.ts`

### 2. skill 和 run-slash-command 保持 interaction 分类

**决策：不移动到 builtin，保持现状**

**理由：**

| 特性 | Builtin 工具 | Interaction 工具 |
|------|-------------|-----------------|
| 执行上下文 | 需要 `BuiltinToolExecutionContext` | 纯参数处理 |
| 依赖关系 | 依赖 DI 容器访问内部服务 | 无特殊依赖 |
| 功能定位 | 工作流引擎内部控制 | 用户交互功能 |
| 实现复杂度 | 高，直接调用内部服务 | 低，返回标记结果 |

- `skill` 和 `run-slash-command` 都是简单的参数校验和字符串返回
- 它们返回特殊标记供工作流引擎识别，但本身不直接访问内部服务
- 与 `execute-workflow` 等 builtin 工具有本质区别

**代码参考：**
- `sdk/resources/predefined/tools/stateless/interaction/skill/handler.ts`
- `sdk/resources/predefined/tools/stateless/interaction/run-slash-command/handler.ts`
- `sdk/resources/predefined/tools/builtin/workflow/execute-workflow/handler.ts`

## 当前目录结构

```
sdk/resources/predefined/tools/
├── builtin/                    # 工作流引擎内部控制工具
│   └── workflow/
│       ├── cancel-workflow/
│       ├── execute-workflow/
│       └── query-workflow-status/
│
├── stateful/                   # 有状态工具（工具自身维护状态）
│   ├── memory/
│   │   └── session-note/       # 维护会话笔记状态
│   └── shell/
│       └── backend-shell/      # 维护后台 shell 进程状态
│
└── stateless/                  # 无状态工具
    └── interaction/            # 用户交互类工具
        ├── ask-followup-question/
        ├── run-slash-command/  # 纯转发，返回标记
        ├── skill/              # 纯转发，返回标记
        ├── update-todo-list/
        └── use-mcp/            # 纯转发，无自身状态
```

## 决策总结

| 工具 | 原位置 | 建议 | 最终决策 |
|------|--------|------|----------|
| use-mcp | stateless/interaction/use-mcp | 保持 | **保持现状** |
| skill | stateless/interaction/skill | 保持 | **保持现状** |
| run-slash-command | stateless/interaction/run-slash-command | 保持 | **保持现状** |

所有工具保持当前目录位置，不进行迁移。

## 补充说明

如需调整，可考虑以下替代方案：

1. **创建 `stateless/internal` 子目录**：存放与内部实现相关但非 builtin 的工具
2. **扩展分类体系**：在 interaction 下创建 `internal` 子分类

但当前结构已足够清晰，暂不需要调整。

---

*文档创建时间：2026-04-26*
*相关分析：tools-architecture-design.md*
