# 工作流配置（workflows / node-templates / prompt-templates / scripts）

工作流相关定义由 API 与存储管理；仓库中的样例文件只描述结构，不会被运行时自动加载。`wf-config::processor` 提供定义校验，供 API 保存与资源注册复用。当前 `index.json` 仅用于 LLM 文件族，资源家族的索引清单不作为启动入口。

## workflows/

样例文件：`config-examples/workflows/*.workflow.example.toml`（加载器跳过 `.example.` 文件）。

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

样例文件：`config-examples/node-templates/*.template.example.toml`，多模板以 `---` 分隔。

关键字段（`NodeTemplate`）：`name`、`node_type`、`description`、`[nodes.config]`、`[metadata]`。

## prompt-templates/

两种形式：

- TOML：`config-examples/prompt-templates/*.prompt.example.toml`（`Template` + `TemplateVariableDefinition`）
- JSON：`config-examples/prompt-templates/custom.prompts.example.json`（`prompts` 数组）

关键字段：`id`、`name`、`description`、`category`、`content`、`[[variables]]`（`name`、`type`、`required`、`description`、`default_value`）、`fragments`、`metadata`。

同名模板与片段以模板优先，与渲染期并集一致。

## scripts/

脚本定义由 API 与存储管理，没有自动加载的脚本家族文件。
