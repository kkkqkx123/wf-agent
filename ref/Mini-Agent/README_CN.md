# Mini Agent

[English](./README.md) | 中文

**Mini Agent** 是一个极简但专业的演示项目，旨在展示使用 MiniMax M2 模型构建 Agent 的最佳实践。项目通过兼容 Anthropic 的 API，完全支持交错思维（interleaved thinking），从而解锁 M2 模型在处理长而复杂的任务时强大的推理能力。

该项目具备一系列为稳健、智能的 Agent 开发而设计的特性：

*   ✅ **完整的 Agent 执行循环**：一个完整可靠的执行框架，配备了文件系统和 Shell 操作的基础工具集。
*   ✅ **持久化记忆**：通过内置的 **Session Note Tool**，Agent 能够在多个会话中保留关键信息。
*   ✅ **智能上下文管理**：自动对会话历史进行摘要，可处理长达可配置 Token 上限的上下文，从而支持无限长的任务。
*   ✅ **集成 Claude Skills**：内置 15 种专业技能，涵盖文档处理、设计、测试和开发等领域。
*   ✅ **集成 MCP 工具**：原生支持 MCP 协议，可轻松接入知识图谱、网页搜索等工具。
*   ✅ **全面的日志记录**：为每个请求、响应和工具执行提供详细日志，便于调试。
*   ✅ **简洁明了的设计**：美观的命令行界面和易于理解的代码库，使其成为构建高级 Agent 的理想起点。

## 目录

- [Mini Agent](#mini-agent)
  - [目录](#目录)
  - [快速开始](#快速开始)
    - [1. 获取 API Key](#1-获取-api-key)
    - [2. 选择使用模式](#2-选择使用模式)
      - [🚀 快速上手模式（推荐新手）](#-快速上手模式推荐新手)
      - [🔧 开发模式](#-开发模式)
  - [使用示例](#使用示例)
    - [任务执行](#任务执行)
    - [使用 Claude Skill（例如：PDF 生成）](#使用-claude-skill例如pdf-生成)
    - [网页搜索与摘要（MCP 工具）](#网页搜索与摘要mcp-工具)
  - [测试](#测试)
    - [快速运行](#快速运行)
    - [测试覆盖范围](#测试覆盖范围)
  - [相关文档](#相关文档)
  - [贡献](#贡献)
  - [许可证](#许可证)
  - [参考资源](#参考资源)

## 快速开始

### 1. 获取 API Key

MiniMax 提供国内和海外两个平台，请根据您的网络环境选择：

| 版本       | 平台地址                                                       | API Base                             |
| ---------- | -------------------------------------------------------------- | ------------------------------------ |
| **国内版** | [https://platform.minimaxi.com](https://platform.minimaxi.com) | `https://api.minimaxi.com/anthropic` |
| **海外版** | [https://platform.minimax.io](https://platform.minimax.io)     | `https://api.minimax.io/anthropic`   |

**获取步骤：**
1. 访问相应平台注册并登录
2. 进入 **账户管理 > API 密钥**
3. 点击 **"创建新密钥"**
4. 复制并妥善保存（密钥仅显示一次）

> 💡 **提示**：请记住您所选平台对应的 API Base 地址，后续配置时会用到。

**前置要求：安装 pipx**

两种使用模式都需要 pipx。如果您尚未安装：

```bash
# macOS
brew install pipx
pipx ensurepath

# Linux
sudo apt install pipx  # Debian/Ubuntu
pipx ensurepath

# 安装完成后，重启终端或运行：
source ~/.bashrc  # 或 ~/.zshrc
```

### 2. 选择使用模式

我们提供两种使用模式，请根据您的需求选择：

#### 🚀 快速上手模式（推荐新手）

此模式适合希望快速体验 Mini Agent，而无需克隆代码仓库或修改代码的用户。

**安装步骤：**

```bash
# 1. 直接从 GitHub 安装
pipx install git+https://github.com/MiniMax-AI/Mini-Agent.git

# 2. 运行配置脚本（自动创建配置文件）
curl -fsSL https://raw.githubusercontent.com/MiniMax-AI/Mini-Agent/main/scripts/setup-config.sh | bash
```

> 💡 **提示**：如果您希望在本地进行开发或修改代码，请使用下方的“开发模式”。

**配置步骤：**

配置脚本会在 `~/.mini-agent/config/` 目录下创建配置文件，请编辑该文件：

```bash
# 编辑配置文件
nano ~/.mini-agent/config/config.yaml
```

填入您的 API Key 和对应的 API Base：

```yaml
api_key: "YOUR_API_KEY_HERE"          # 填入第 1 步获取的 API Key
api_base: "https://api.minimaxi.com/anthropic"  # 国内版
# api_base: "https://api.minimax.io/anthropic"  # 海外版（如使用海外平台，请取消本行注释）
model: "MiniMax-M2"
```

**开始使用：**

```bash
mini-agent                                    # 使用当前目录作为工作空间
mini-agent --workspace /path/to/your/project  # 指定工作空间目录
mini-agent --version                          # 查看版本信息
```

#### 🔧 开发模式

此模式适合需要修改代码、添加功能或进行调试的开发者。

**安装与配置步骤：**

```bash
# 1. 克隆仓库
git clone https://github.com/MiniMax-AI/Mini-Agent.git
cd Mini-Agent

# 2. 安装 uv（如果尚未安装）
curl -LsSf https://astral.sh/uv/install.sh | sh

# 3. 同步依赖
uv sync

# 4. 初始化 Claude Skills（可选）
git submodule update --init --recursive

# 5. 复制配置模板
cp mini_agent/config/config-example.yaml mini_agent/config/config.yaml

# 6. 编辑配置文件
vim mini_agent/config/config.yaml  # 或使用您偏好的编辑器
```

填入您的 API Key 和对应的 API Base：

```yaml
api_key: "YOUR_API_KEY_HERE"          # 填入第 1 步获取的 API Key
api_base: "https://api.minimaxi.com/anthropic"  # 国内版
# api_base: "https://api.minimax.io/anthropic"  # 海外版（如使用海外平台，请修改此行）
model: "MiniMax-M2"
max_steps: 100
workspace_dir: "./workspace"
```

> 📖 完整的配置指南，请参阅 [config-example.yaml](mini_agent/config/config-example.yaml)

**运行方式：**

选择您偏好的方式运行：

```bash
# 方式 1：作为模块直接运行（适合调试）
uv run python -m mini_agent.cli

# 方式 2：以可编辑模式安装（推荐）
pipx install -e .
# 安装后，您可以在任何路径下运行，且代码更改会立即生效
mini-agent
mini-agent --workspace /path/to/your/project
```

> 📖 更多开发指引，请参阅 [开发指南](docs/DEVELOPMENT_GUIDE_CN.md)

> 📖 更多生产部署指引，请参阅 [生产指南](docs/PRODUCTION_GUIDE_CN.md)

## 使用示例

这里有几个 Mini Agent 能力的演示。

### 任务执行

*在这个演示中，我们要求 Agent 创建一个简洁美观的网页并在浏览器中显示它，以此展示基础的工具使用循环。*

![演示动图 1: 基础任务执行](docs/assets/demo1-task-execution.gif "基础任务执行演示")

### 使用 Claude Skill（例如：PDF 生成）

*这里，Agent 利用 Claude Skill 根据用户请求创建专业文档（如 PDF 或 DOCX），展示了其强大的高级能力。*

![演示动图 2: Claude Skill 使用](docs/assets/demo2-claude-skill.gif "Claude Skill 使用演示")

### 网页搜索与摘要（MCP 工具）

*此演示展示了 Agent 如何使用其网页搜索工具在线查找最新信息，并为用户进行总结。*

![演示动图 3: 网页搜索](docs/assets/demo3-web-search.gif "网页搜索演示")


## 测试

项目包含了覆盖单元测试、功能测试和集成测试的全面测试用例。

### 快速运行

```bash
# 运行所有测试
pytest tests/ -v

# 仅运行核心功能测试
pytest tests/test_agent.py tests/test_note_tool.py -v
```

### 测试覆盖范围

- ✅ **单元测试** - 工具类、LLM 客户端
- ✅ **功能测试** - Session Note Tool、MCP 加载
- ✅ **集成测试** - Agent 端到端执行
- ✅ **外部服务** - Git MCP 服务器加载


## 相关文档

- [开发指南](docs/DEVELOPMENT_GUIDE_CN.md) - 详细的开发和配置指引
- [生产环境指南](docs/PRODUCTION_DEPLOYMENT_GUIDE_CN.md) - 生产部署最佳实践

## 贡献

我们欢迎并鼓励您提交 Issue 和 Pull Request！

- [贡献指南](CONTRIBUTING.md) - 如何为项目做贡献
- [行为准则](CODE_OF_CONDUCT.md) - 社区行为准则

## 许可证

本项目采用 [MIT 许可证](LICENSE) 授权。

## 参考资源

- MiniMax API: https://platform.minimaxi.com/document
- MiniMax-M2: https://github.com/MiniMax-AI/MiniMax-M2
- Anthropic API: https://docs.anthropic.com/claude/reference
- Claude Skills: https://github.com/anthropics/skills
- MCP Servers: https://github.com/modelcontextprotocol/servers

---

**⭐ 如果这个项目对您有帮助，请给它一个 Star！**
