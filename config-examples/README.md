# config-examples/ — 配置文档样例（从不加载）

本目录是纯文档，说明各项配置长什么样、落到哪里。
所有加载器统一跳过文件名含 `.example.` 的文件，
样例目录本身从不参与任何加载，因此样例不可能泄漏进运行配置。

## 用法

- 原样可用的模板（`infrastructure/`、`server/`）：直接复制到目标位置再按需改值。
- 草图样例（`*.example.*`）：去掉 `.example` 后缀后作为起点手写真实定义。

## 目录

| 目录 | 样例用途，复制目标 |
|---|---|
| `infrastructure/` | 运行时九域模板，落到显式管理目录、`{settings_dir}/infrastructure/` 或项目 `.wf/infrastructure/` |
| `server/` | 传输与中间件四文件模板，落到 `{settings_dir}/server/` 或项目 `.wf/server/` |
| `mcp/` | 全局 MCP 设置，落到显式用户设置目录或项目 `.wf/mcp.json` |
| `skills/` | 全局技能设置，落到显式用户设置目录或项目 `.wf/skills.json` |
| `workflows/` | 工作流定义样例与索引格式 |
| `agent-loops/` | 智能体循环定义样例与索引格式 |
| `node-templates/` | 节点模板样例与索引格式 |
| `prompt-templates/` | 提示词模板样例 |
| `scripts/` | 预留脚本家族索引格式 |

本目录不参与任何优先级链，不随发布物分发到生产环境；
生产真实值只由 `deploy/` 机制在目标机器上渲染（见 `deploy/README.md`）。
