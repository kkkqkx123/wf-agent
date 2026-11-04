# MCP 与技能配置

## MCP

- 全局：`{settings_dir}/mcp-settings.json`
- 项目：`{project}/.wf/mcp.json`（优先）
- 结构：`{ "mcpServers": { "<name>": { ... } } }`
- 单个服务字段：`type`（`stdio` / `streamable-http`）、`command`、`args`、`cwd`、`env`、`url`、`disabled`、`timeout`、`alwaysAllow`、`disabledTools`、`lifecycle`、`idle_timeout`、`health_check_interval`
- 项目层与全局层按服务名合并，项目覆盖同名服务

## 技能

- 全局：`{settings_dir}/skill-settings.json`
- 项目：`{project}/.wf/skills.json`（优先）
- 结构：`{ "paths": [...], "autoScan": true }`
- 合并规则：`paths` 并集去重（项目优先），`autoScan` 项目优先，缺省 `true`
