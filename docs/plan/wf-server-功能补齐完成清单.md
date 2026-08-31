# wf-server 功能补齐完成清单

> 状态：实现完成，编译验证受环境依赖阻塞
> 范围：`crates/app/wf-server`（HTTP 传输层）基于 `crates/app/wf-api`（业务 API 门面层）能力差异的功能补齐
> 依据：docs/api/README.md 与 docs/api/07-wf-server-HTTP层.md 的静态分析结论

## 背景与目标

wf-api 提供 612 个公开函数，wf-server 以约 365 条路由覆盖其中大部分。经函数级比对，wf-server 完全未引用的函数有 215 个，其中约 150 个属内部机制（builder 构造器、组合根注入、持久化实现、订阅/流/任务注册表等）无需 HTTP 暴露；剩余约 25 个具备对外价值，本清单将其分组为多个阶段逐项补齐。

补齐原则：

- server 保持纯传输层定位：新 handler 全部委托 wf-api，不做业务逻辑。
- 信封、SSE、错误映射复用现有基础设施（envelope.rs / sse.rs）。
- 每个新增端点对齐现有路由命名风格与 docs/api/07 的端点清单格式。

## 阶段 2：workflow 域端点补齐

### T2-1 工作流版本自动递增

- 端点：`POST /workflows/{id}/versions/increment?level=patch|minor|major`
- 对应 wf-api：`workflow::versioning::create_versioned_update`（`VersionStrategy` / `WorkflowChanges` 已导出）
- 请求体：`{ "changes": {...}, "keep_original": bool }`，changes 支持 name / description / metadata / nodes / edges / version 可选字段
- 响应：新版本号字符串
- 涉及文件：`wf-server/src/api/workflow/versions.rs`、`wf-server/src/extract.rs`（如需新增提取器）
- 验收：level 缺省按 patch；非法 level 返回 400；工作流不存在返回 404；版本自动递增符合 semver 语义

### T2-2 工作流定义解析与节点校验

- 端点：`POST /workflows/parse`（body 为 JSON/TOML 定义文本 + format 字段）
- 对应 wf-api：`infra::config::parse_workflow` / `parse_workflow_file`
- 端点：`POST /workflows/validate/node`（body 为 node_type + node_id + config）
- 对应 wf-api：`infra::config::validate_node`
- 端点：`POST /workflows/transform`（body 为 nodes + edges 声明式配置）
- 对应 wf-api：`infra::config::transform_workflow_nodes` / `transform_workflow_edges`
- 涉及文件：`wf-server/src/api/workflow/workflows.rs`
- 验收：parse 支持 json/toml 两种格式；validate/node 返回问题清单；transform 返回转换后的节点与边

### T2-3 工作流定义 TOML 导出

- 端点：`GET /workflows/{id}/export?format=json|toml`
- 对应 wf-api：现有 `export_workflow_json` + `infra::config::export_toml`
- 涉及文件：`wf-server/src/api/workflow/workflows.rs`
- 验收：format=toml 返回 TOML 文本（Content-Type: text/toml）；缺省保持 json 行为不变

## 阶段 3：agent 域端点补齐

### T3-1 Agent 定义校验

- 端点：`POST /agents/validate`
- 对应 wf-api：`infra::config::validate_agent`
- 涉及文件：`wf-server/src/api/agent/agents.rs`（或 profiles.rs）
- 验收：合法定义返回 ok；非法定义返回 Validation 错误携带问题描述

### T3-2 Agent Loop live 状态机更新

- 端点：`POST /agent-loops/{id}/status/transition`（body 为目标状态）
- 对应 wf-api：`agent::agent_loop_registry::update_status`（live 状态机路由，运行中实体直接翻转）
- 与现有 `PATCH /agent-loops/{id}/status`（持久化改写，`agent::update_agent_loop_status`）并存：live 实体存在时走状态机，否则回退持久化
- 涉及文件：`wf-server/src/api/agent/loops.rs`
- 验收：live 运行中 loop 可 pause/resume/stop；非 live 回退持久化改写

### T3-3 live 实体清理

- 端点：`POST /agent-loops/cleanup-completed`
- 对应 wf-api：`agent::agent_loop_registry::cleanup_completed`
- 响应：清理数量
- 涉及文件：`wf-server/src/api/agent/loops.rs`
- 验收：返回清理的 terminated live 实体数量

### T3-4 循环消息去重删除

- 端点：`POST /agent-loops/{id}/messages/dedupe`
- 对应 wf-api：`agent::agent_message::dedupe_and_delete`
- 响应：删除的重复消息数量
- 涉及文件：`wf-server/src/api/agent/variables.rs`（消息端点所在文件）
- 验收：返回删除数量；无重复时返回 0

### T3-5 技能 prompt 组装

- 端点：`GET /skills/prompt`
- 对应 wf-api：`entity::skill::to_prompt`
- 响应：启用技能元数据 + 全文拼接的 prompt 文本
- 涉及文件：`wf-server/src/api/agent/skills.rs`
- 验收：返回完整 prompt；无启用技能时返回空/零值不报错

### T3-6 触发器启用状态查询

- 端点：`GET /triggers/{id}/enabled`（全局触发器）、`GET /agent-triggers/{id}/enabled`
- 对应 wf-api：`entity::trigger::is_trigger_enabled` / `workflow::execution_trigger::is_enabled`
- 涉及文件：`wf-server/src/api/resource/triggers.rs`、`wf-server/src/api/agent/triggers.rs`
- 验收：返回布尔启用状态；不存在返回 404

## 阶段 4：analysis 域端点补齐

### T4-1 错误链流式推送（SSE）

- 端点：`GET /executions/{id}/error-analysis/stream`
- 对应 wf-api：`analysis::error_analysis::stream_error_chain`（根优先，一次一条）
- 帧格式：复用 sse.rs，每条 `data: {json}`；结束发送 `data: {"done": true}` 或关闭流
- 涉及文件：`wf-server/src/api/workflow/analysis.rs`
- 验收：SSE 按根因优先顺序输出全部错误记录；无错误时立即结束

### T4-2 查询表达式独立求值

- 端点：`POST /query/evaluate`（body 为 field + operator + value + record）
- 对应 wf-api：`query::evaluate_expression` / `get_field_value`
- 响应：布尔结果
- 涉及文件：`wf-server/src/api/workflow/query.rs`
- 验收：支持点路径字段访问与全部 FilterOperator

## 阶段 5：trigger 模板双持久化修复

### T5-1 模板保存同步注册内存注册表

- 问题：`POST /templates/trigger`（`agent_trigger_template::save`）只写存储、不注册 wf-resource 内存注册表，经 HTTP 保存的 trigger 模板无法进入 TriggerEventListener 被执行
- 修复：在 `wf-api/src/template/agent_trigger_template.rs` 的 save 路径补齐注册表注册（对齐 `builder::template::TriggerTemplateBuilder::register` 的双持久化语义）
- 涉及文件：`wf-api/src/template/agent_trigger_template.rs`
- 验收：经 HTTP 保存的 trigger 模板可被事件监听器识别（新增单元测试覆盖双持久化）

## 阶段 6：传输层能力

### T6-1 OpenAPI 清单端点

- 端点：`GET /api/v1/openapi.json`
- 内容：服务信息（name/version/apiVersion）+ 全部路由清单（method + path + 描述），从各域 `routes()` 汇总
- 不引入第三方库（utopa 等），以静态汇总 JSON 形式提供，后续可平滑替换为自动生成
- 涉及文件：`wf-server/src/api/resource/health.rs` 或新增 `wf-server/src/api/resource/openapi.rs`
- 验收：返回结构完整的端点清单，与 docs/api/07 路由清单一致

## 阶段 7：验证与文档同步

### T7-1 编译与测试

- `cargo clippy --all-targets --all-features` 全量通过
- 新增端点的 handler 单元测试（参照 tests/http_integration.rs 风格）与 wf-api 侧新增测试

### T7-2 文档同步

- 更新 docs/api/07-wf-server-HTTP层.md 端点清单（新增端点）
- 本清单全部任务完成后标记状态为完成

## 完成记录

- T2、T3、T4、T5、T6 已完成。
- T7 文档同步已完成；`cargo check` 受环境缺少 LuaJIT 开发库阻塞，需补齐 `luajit.pc` 后重跑编译与测试。
