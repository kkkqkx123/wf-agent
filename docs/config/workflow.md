# 工作流配置（workflows / node-templates / prompt-templates / scripts）

工作流相关配置位于项目 `.wf/`（或 settings）下的资源索引家族。启动时只作为 `import` 起点；网关与注册表的真实来源是 API 与存储。

## workflows/

文件：`{project}/.wf/workflows/*.workflow.toml`，索引：`index.json`（`type: workflows`，路径 `./*.toml`）。

关键字段（`WorkflowDefinition`）：

- `id`、`name`、`type`（如 `STANDALONE`）、`description`、`version`、`created_at`、`updated_at`
- `[[variables]]`：`name`、`type`、`required`、`description`
- `[[nodes]]`：`id`、`node_type`（`START`/`END`/`LLM`/`CUSTOM`/`SCRIPT`/...）、`[nodes.config]`
- `[[edges]]`：`source_node_id`、`target_node_id`、`source_port`、`target_port`
- `[config]`：`timeout`、`max_steps`、`checkpoint`、`retry_policy`、`tool_approval`、`available_tools`、`initial_messages`、`static_contexts`
- `[metadata]`：`author`、`tags`、`category`

节点配置按节点类型校验（LLM / Script / Variable / Route / Fork / Join / Subgraph / Interaction / AgentLoop 等）；插件贡献的未知类型免检。issue 携带点分路径，如 `nodes.n1.config.profile_id`。

引用语法：`${path}` / `{{path}}`（如 `$ref:node.output`、`$var:varName`）。

## node-templates/

文件：`{project}/.wf/node-templates/*.template.toml`，多模板以 `---` 分隔。

关键字段（`NodeTemplate`）：`name`、`node_type`、`description`、`[nodes.config]`、`[metadata]`。

## prompt-templates/

两种形式：

- TOML：`{project}/.wf/prompt-templates/*.prompt.toml`（`Template` + `TemplateVariableDefinition`）
- JSON：`custom.prompts.json`（`prompts` 数组）

关键字段：`id`、`name`、`description`、`category`、`content`、`[[variables]]`（`name`、`type`、`required`、`description`、`default_value`）、`fragments`、`metadata`。

同名模板与片段以模板优先，与渲染期并集一致。

## scripts/

目录预留：`{project}/.wf/scripts/index.json`（`type: scripts`），暂无自动加载的脚本家族文件。
