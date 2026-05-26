# AgentFS 模块划分分析

## 项目概述

AgentFS 是一个专为 AI 代理设计的文件系统。它提供了一种存储抽象，使 AI 代理能够管理其状态、文件和工具调用历史记录。整个项目采用多语言 SDK 和命令行工具的方式实现，核心基于 SQLite 数据库。

## 模块划分

### 1. 核心规范模块 (Specification)

**位置**: SPEC.md

**功能**:
- 定义了 AgentFS 的 SQLite 数据库模式
- 包含三个主要组件：
  - 工具调用审计跟踪 (Tool Call Audit Trail)
  - 虚拟文件系统 (Virtual Filesystem)
  - 键值存储 (Key-Value Store)
- 定义了 Overlay 文件系统用于复制写入 (copy-on-write) 语义

**重要性**: 这是整个项目的基础，定义了数据如何在底层存储和组织。

### 2. SDK 模块 (Software Development Kit)

**位置**: sdk/

**子模块**:
- **TypeScript SDK** (sdk/typescript): 为 JavaScript/TypeScript 环境提供访问接口
- **Python SDK** (sdk/python): 为 Python 环境提供访问接口
- **Rust SDK** (sdk/rust): 为 Rust 环境提供访问接口

**功能**:
- 提供编程方式访问 AgentFS 的接口
- 实现三个核心接口：文件系统、键值存储和工具调用跟踪
- 支持持久化存储和临时内存数据库选项

**特点**:
- 多语言支持，便于集成到不同的 AI 框架中
- 统一的 API 设计，跨语言一致性

### 3. 命令行界面模块 (CLI)

**位置**: cli/

**功能**:
- 提供命令行工具来管理 AgentFS
- 支持挂载 AgentFS 到主机文件系统（Linux 上使用 FUSE，macOS 上使用 NFS）
- 提供文件访问的命令行工具
- 支持沙盒执行环境

**主要命令**:
- `agentfs init`: 初始化新的 AgentFS
- `agentfs run`: 在沙盒环境中运行程序
- `agentfs mount`: 挂载 AgentFS
- `agentfs fs`: 文件系统操作
- `agentfs timeline`: 显示工具调用时间线

### 4. 沙盒模块 (Sandbox)

**位置**: sandbox/

**功能**:
- 提供安全的执行环境
- 使用复制写入 (copy-on-write) 文件系统隔离
- 支持 Linux 命名空间和 macOS 沙盒机制

**特点**:
- 保证代理执行的安全性
- 防止对主机系统的意外修改
- 支持会话持久化

### 5. 示例模块 (Examples)

**位置**: examples/

**子模块**:
- **Mastra** (examples/mastra/research-assistant): 使用 Mastra AI 框架的研究助手
- **Claude Agent SDK** (examples/claude-agent/research-assistant): 使用 Anthropic Claude Agent SDK 的研究助手
- **OpenAI Agents** (examples/openai-agents/research-assistant): 使用 OpenAI Agents SDK 的研究助手
- **Firecracker** (examples/firecracker): 使用 Firecracker VM 的最小实现
- **AI SDK + just-bash** (examples/ai-sdk-just-bash): 使用 Vercel AI SDK 的交互式 AI 代理
- **Cloudflare Workers** (examples/cloudflare): 在 Cloudflare Workers 上运行的 AI 代理

**功能**:
- 展示如何将 AgentFS 集成到流行的 AI 框架中
- 提供实际应用案例和最佳实践

### 6. 脚本模块 (Scripts)

**位置**: scripts/

**功能**:
- 包含构建和维护脚本
- 版本更新脚本
- 依赖安装脚本

### 7. 文档模块 (Documentation)

**位置**: 
- README.md: 项目概述和入门指南
- SPEC.md: 技术规范
- MANUAL.md: 用户手册
- CHANGELOG.md: 版本变更记录
- TESTING.md: 测试说明

## 架构关系

```
┌─────────────────┐    ┌─────────────────┐
│   SDK模块       │    │   CLI模块       │
│  (多语言支持)   │◄──►│  (命令行工具)   │
└─────────────────┘    └─────────────────┘
         │                       │
         ▼                       ▼
┌─────────────────────────────────────────────┐
│              核心规范模块                   │
│         (SQLite数据库模式)                  │
└─────────────────────────────────────────────┘
                        │
         ┌──────────────┼──────────────┐
         ▼              ▼              ▼
   沙盒模块        示例模块        文档模块
 (安全执行环境)   (集成示例)      (用户指导)
```

## 关键特性

1. **可审计性**: 每个文件操作、工具调用和状态更改都记录在 SQLite 数据库中
2. **可重现性**: 可以快照代理状态并在以后恢复
3. **可移植性**: 整个代理运行时存储在单个 SQLite 文件中
4. **安全性**: 通过沙盒和复制写入机制提供执行隔离
5. **多语言支持**: 提供 TypeScript、Python 和 Rust SDK

## 技术栈

- **后端/核心**: Rust (CLI、沙盒、Rust SDK)
- **前端/客户端**: TypeScript/JavaScript (TypeScript SDK)
- **服务端脚本**: Python (部分脚本)
- **数据库**: SQLite (Turso 兼容)
- **协议**: NFS、FUSE (用于文件系统挂载)
- **加密**: libSQL 加密功能

## 扩展点

1. **新语言 SDK**: 可以轻松添加其他语言的 SDK
2. **新协议支持**: 可以添加更多网络协议支持
3. **扩展文件系统功能**: 可以添加更多 POSIX 功能
4. **同步功能**: 支持与远程 Turso 数据库同步