# 配置总览

本目录整理当前项目所有配置文件、配置域与作用。配置的语义与加载实现由 `wf-config` crate 提供，类型定义位于 `wf-types::config` / `wf-types::llm` / `wf-types::workflow` / `wf-types::agent` 等；本目录只描述配置本身，不重复实现细节。

## 配置分层与优先级

从低到高：

1. 代码默认值（Development / Production 两套环境默认）
2. 宿主 `config.toml` 提供的领域默认值（显式 settings 目录优先于 XDG / `~/.wf`，项目 `.wf/config.toml` 再优先）
3. 固定域文件：显式托管目录（`--infra-dir` / `--settings-dir`）替代 settings 与项目层；否则 settings 层低于项目 `.wf/infrastructure/`
4. `WF_*` 环境变量
5. CLI flags / SDK `ConfigOverrides` / 程序化覆盖

TOML 合并为字段级递归合并；JSON 家族按记录 `id` 覆盖。基础设施文件缺失时回退领域默认值，存在但损坏的文件启动失败；用户级 `config.toml` 层采用宽松策略（坏文件告警跳过），有效值进入装配器作为领域默认值。LLM 文件族声明非法时在 bootstrap 直接失败，不降级为默认配置。

## 文件系统布局

- 项目层根目录：`{project_root}/.wf`
- 用户 settings 层：`{settings_dir}/...`（通常 `~/.config/wf` 或托管目录）
- 基础设施域文件：`{layer}/infrastructure/*.toml`
- 服务传输与中间件：`{layer}/server/*.toml`
- LLM 家族：`{layer}/llm-providers/`、`{layer}/llm-profiles/`、`{layer}/llm-auth/`
- 资源索引家族：`{project}/.wf/{workflows,agent-loops,node-templates,prompt-templates,scripts}/`
- MCP / 技能：`{layer}/mcp-settings.json` 与 `{project}/.wf/mcp.json`；`{layer}/skill-settings.json` 与 `{project}/.wf/skills.json`
- 按键绑定：`{layer}/keymap-settings.toml` 与 `{project}/.wf/keymap-settings.toml`
- 宿主旋钮：`{layer}/config.toml` 与 `{project}/.wf/config.toml`
- 易变目录：`.wf/meta`、`.wf/tmp`（自动维护 `.gitignore`，不参与配置扫描）

## 域文档索引

| 文档 | 覆盖范围 |
|---|---|
| [priority-and-layout.md](./priority-and-layout.md) | 优先级、布局、易变目录、example 跳过规则 |
| [infrastructure.md](./infrastructure.md) | 基础设施九域：storage、metrics、sandbox、file-checkpoint、tool-approval、presets、tools、limits、retention |
| [server.md](./server.md) | 服务传输、中间件与后端注册表：server、auth、cors、rate-limit、backends |
| [llm.md](./llm.md) | LLM provider、profile、auth 声明与索引家族 |
| [workflow.md](./workflow.md) | 工作流定义、节点模板、提示词模板、脚本家族 |
| [agent.md](./agent.md) | 智能体循环定义 |
| [mcp-skills.md](./mcp-skills.md) | MCP 服务连接与技能路径 |
| [keymap.md](./keymap.md) | 按键绑定 |
| [tui.md](./tui.md) | TUI 前端显示配置 |
| [host.md](./host.md) | 宿主旋钮 config.toml |
| [environment.md](./environment.md) | `WF_*` 环境变量、冻结旧名、`${VAR}` 插值 |

## 通用加载规则

- `*.example.*` 样例文件永远不加载；`config-examples/` 目录从不参与加载。
- 只有 LLM 文件族（providers / profiles / auth）支持 `index.json` 的路径模式；资源定义由 API 与存储管理。
- 领域文件"缺失"回退默认值；"存在但无效"启动失败。
- 用户级全局层（`config.toml`）缺失给默认值、坏文件告警跳过而非阻断启动。
- `${VAR}` / `${VAR:default}` 在解析前展开；仅大写变量名匹配，工作流表达式 `${input.a}` 不受影响。
- `{{parameters.*}}` 替换发生在工作流执行期，不属于配置加载。
