# Mini-Agent 与 CLI-App 功能对比分析

## 一、架构对比

| 维度 | Mini-Agent (Python) | CLI-App (TypeScript) |
|------|---------------------|----------------------|
| **定位** | 单一 Agent 框架 | 多工作流管理 CLI |
| **架构** | 单体 Agent 循环 | 三层架构 (CLI → Adapter → SDK) |
| **执行模型** | 单线程对话循环 | 多线程工作流执行 |
| **状态管理** | 内存消息历史 | 持久化检查点 |

---

## 二、Mini-Agent 核心功能概览

### 2.1 目录结构

```
mini_agent/
├── __init__.py              # 模块入口，导出核心类
├── agent.py                 # 核心 Agent 实现
├── llm.py                   # LLM 客户端
├── config.py                # 配置管理
├── logger.py                # 运行日志记录
├── retry.py                 # 重试机制
├── cli.py                   # 命令行交互入口
├── config/                  # 配置文件目录
│   ├── config-example.yaml  # 配置示例
│   ├── mcp.json             # MCP 服务器配置
│   └── system_prompt.md     # 系统提示词
├── schema/                  # 数据模型定义
│   └── schema.py            # Pydantic 模型
├── skills/                  # 技能目录 (15个技能)
│   ├── document-skills/     # 文档处理技能
│   │   ├── pdf/
│   │   ├── pptx/
│   │   ├── docx/
│   │   └── xlsx/
│   └── ...                  # 其他技能
└── tools/                   # 工具实现
    ├── base.py              # 工具基类
    ├── file_tools.py        # 文件操作工具
    ├── bash_tool.py         # Bash 命令工具
    ├── note_tool.py         # 会话笔记工具
    ├── mcp_loader.py        # MCP 工具加载器
    ├── skill_loader.py      # 技能加载器
    └── skill_tool.py        # 技能工具
```

### 2.2 核心特性

#### Agent 核心类
- 单一 Agent 实现，支持基本工具和 MCP 支持
- 消息历史管理和 Token 估算
- 自动消息摘要机制（防止上下文溢出）
- 工具调用循环执行

#### LLM 客户端
- MiniMax M2 模型的 Anthropic 兼容 API 客户端
- 支持扩展思维 (Extended Thinking) 和工具调用
- 内置重试机制

#### 工具系统
- **文件工具**: ReadTool, WriteTool, EditTool
- **Bash 工具**: 前台/后台命令执行，进程管理
- **笔记工具**: SessionNoteTool, RecallNoteTool
- **MCP 加载器**: MCP 工具包装器和连接管理
- **技能系统**: 按需加载技能内容

#### 配置管理
- 多路径配置搜索（开发模式 → 用户配置 → 安装目录）
- YAML 格式配置文件
- 环境变量覆盖支持

#### CLI 入口
- 交互式多轮对话
- 内置命令: `/help`, `/clear`, `/history`, `/stats`, `/exit`
- 使用 prompt_toolkit 提供增强的输入体验
- 支持历史记录、自动补全、快捷键

---

## 三、CLI-App 可补充的功能

### 3.1 消息历史管理增强 ⭐⭐⭐

**Mini-Agent 特性：**
- 使用 `tiktoken` 精确计算 Token 数量
- 超过阈值时自动触发消息摘要
- 保留用户消息，总结 Agent 执行过程

**CLI-App 现状：**
- Agent Loop 有消息历史管理，但缺少 Token 估算
- 没有自动摘要机制

**建议补充：**
```
src/tools/stateful/message-summarizer.ts  # 消息摘要工具
src/utils/token-counter.ts                # Token 计数器
```

---

### 3.2 交互式对话模式 ⭐⭐⭐

**Mini-Agent 特性：**
- 使用 `prompt_toolkit` 提供增强输入体验
- 支持历史记录、自动补全、快捷键
- 内置命令：`/help`, `/clear`, `/history`, `/stats`, `/exit`

**CLI-App 现状：**
- 主要是命令式操作，缺少交互式对话
- Agent Loop 执行是单次调用，不支持多轮对话

**建议补充：**
```
src/commands/chat/                        # 交互式对话命令
  ├── index.ts                            # 对话入口
  ├── repl.ts                             # REPL 循环
  └── commands.ts                         # 内置命令处理
```

---

### 3.3 技能系统 ⭐⭐⭐

**Mini-Agent 特性：**
- 15 个预置技能（PDF、PPTX、DOCX、XLSX 等）
- 渐进式技能披露（元数据 → 按需加载 → 完整内容）
- YAML frontmatter 格式的技能定义

**CLI-App 现状：**
- 有模板系统，但不是技能系统
- 缺少按需加载机制

**建议补充：**
```
src/commands/skill/                       # 技能管理命令
  ├── index.ts
  ├── register.ts
  └── list.ts
src/skills/                               # 技能目录
  ├── skill-loader.ts                     # 技能加载器
  └── skill-tool.ts                       # 技能工具
```

---

### 3.4 MCP 工具集成 ⭐⭐

**Mini-Agent 特性：**
- 支持标准 MCP 协议
- 自动发现和加载 MCP 服务器工具
- 正确的异步上下文管理

**CLI-App 现状：**
- SDK 层有 MCP 支持，但 CLI 层未暴露
- 缺少 MCP 配置管理命令

**建议补充：**
```
src/commands/mcp/                         # MCP 管理命令
  ├── index.ts
  ├── connect.ts                          # 连接 MCP 服务器
  ├── list.ts                             # 列出可用工具
  └── disconnect.ts                       # 断开连接
```

---

### 3.5 运行日志记录 ⭐⭐

**Mini-Agent 特性：**
- `AgentLogger` 记录完整运行过程
- 日志存储在 `~/.mini-agent/log/`
- 记录 LLM 请求/响应、工具调用结果

**CLI-App 现状：**
- 有日志系统，但主要是 CLI 输出
- 缺少结构化的运行日志

**建议补充：**
```
src/utils/run-logger.ts                   # 运行日志记录器
src/commands/log/                         # 日志管理命令
  ├── index.ts
  ├── show.ts                             # 查看日志
  └── clean.ts                            # 清理日志
```

---

### 3.6 重试机制增强 ⭐

**Mini-Agent 特性：**
- 指数退避策略
- 可配置重试次数和间隔
- 装饰器模式，完全解耦

**CLI-App 现状：**
- 工具有重试，但不是全局机制
- 缺少统一的 LLM 调用重试

**建议补充：**
```
src/utils/retry.ts                        # 统一重试机制
```

---

### 3.7 配置文件搜索优先级 ⭐

**Mini-Agent 特性：**
- 多路径配置搜索（开发模式 → 用户配置 → 安装目录）
- 支持环境变量覆盖

**CLI-App 现状：**
- 配置管理较简单
- 缺少多路径搜索

**建议补充：**
```
src/config/config-search.ts               # 配置搜索逻辑
```

---

## 四、优先级建议

| 优先级 | 功能 | 理由 |
|--------|------|------|
| **P0** | 交互式对话模式 | 提升用户体验，支持多轮对话 |
| **P0** | 消息历史管理增强 | 防止上下文溢出，降低成本 |
| **P1** | 技能系统 | 扩展能力，复用 Mini-Agent 技能 |
| **P1** | MCP 工具集成 | 暴露 SDK 能力，增强工具生态 |
| **P2** | 运行日志记录 | 便于调试和审计 |
| **P2** | 重试机制增强 | 提高稳定性 |
| **P3** | 配置文件搜索优先级 | 提升配置灵活性 |

---

## 五、功能映射表

| Mini-Agent 功能 | CLI-App 对应 | 差距 |
|-----------------|--------------|------|
| Agent 循环 | Agent Loop 命令 | 已有 |
| 工具系统 | Tool 命令 + 工具注册 | 已有 |
| 后台进程管理 | BackgroundShellManager | 已有 |
| 笔记工具 | SessionNoteTool | 已有 |
| Token 估算 | - | 缺失 |
| 消息摘要 | - | 缺失 |
| 交互式对话 | - | 缺失 |
| 技能系统 | 模板系统（不同） | 缺失 |
| MCP 集成 | SDK 有，CLI 无 | 部分缺失 |
| 运行日志 | 日志系统（简化） | 部分缺失 |
| 重试机制 | 工具级重试 | 部分缺失 |

---

## 六、总结

CLI-App 作为一个工作流管理 CLI，已经具备了完善的基础设施。相比 Mini-Agent，主要缺失的是：

1. **交互式对话体验** - 这是 Mini-Agent 的核心优势
2. **智能消息管理** - Token 估算和自动摘要
3. **技能系统** - 渐进式技能披露机制

建议优先实现交互式对话模式和消息历史管理增强，这将显著提升 CLI-App 的用户体验。

---

## 七、参考文件

### Mini-Agent 核心文件

| 文件路径 | 主要功能 |
|---------|---------|
| `ref/Mini-Agent/mini_agent/agent.py` | Agent 核心实现，消息管理，工具执行循环 |
| `ref/Mini-Agent/mini_agent/llm.py` | MiniMax M2 LLM 客户端，Anthropic 兼容 API |
| `ref/Mini-Agent/mini_agent/config.py` | 配置管理，YAML 加载，优先级搜索 |
| `ref/Mini-Agent/mini_agent/logger.py` | 运行日志记录 |
| `ref/Mini-Agent/mini_agent/retry.py` | 异步重试机制，指数退避 |
| `ref/Mini-Agent/mini_agent/cli.py` | 命令行交互入口 |
| `ref/Mini-Agent/mini_agent/schema/schema.py` | Pydantic 数据模型定义 |
| `ref/Mini-Agent/mini_agent/tools/base.py` | 工具基类和结果模型 |
| `ref/Mini-Agent/mini_agent/tools/file_tools.py` | 文件读写编辑工具 |
| `ref/Mini-Agent/mini_agent/tools/bash_tool.py` | Bash 命令执行，后台进程管理 |
| `ref/Mini-Agent/mini_agent/tools/note_tool.py` | 会话笔记记录和回忆 |
| `ref/Mini-Agent/mini_agent/tools/mcp_loader.py` | MCP 工具加载和连接管理 |
| `ref/Mini-Agent/mini_agent/tools/skill_loader.py` | 技能加载器，SKILL.md 解析 |
| `ref/Mini-Agent/mini_agent/tools/skill_tool.py` | 技能工具，按需加载技能 |

### CLI-App 核心文件

| 文件路径 | 主要功能 |
|---------|---------|
| `apps/cli-app/src/index.ts` | CLI 入口 |
| `apps/cli-app/src/commands/` | 13个命令组 |
| `apps/cli-app/src/adapters/` | 15个适配器 |
| `apps/cli-app/src/terminal/` | 终端管理模块 |
| `apps/cli-app/src/tools/` | 工具模块 |
| `apps/cli-app/src/utils/` | 工具函数 |
| `apps/cli-app/src/config/` | 配置管理 |
