# wf-workflow 变量系统与消息上下文

> 范围：`src/variable.rs`、`src/config_parse.rs`、`src/message_context.rs`，辅以 `src/handler/variable_mapping.rs`、`src/handler/output_mapping.rs`。
> 所有 `file:line` 相对 `crates/engine/wf-workflow/src/`。

## 1. 双层语法：`${...}` 与 `{{...}}`（`variable.rs:1-13`）

- `${...}` 是**图装配层的变量传输**：在节点间搬移值并做表达式求值；`{{...}}` 属于**文本渲染层**（`wf_common::template` 经资源模板引擎）。
- 两层刻意按序运行：先 `${...}`、后 `{{...}}`。同时携带两种语法的模板（如脚本节点模板）是合法的，因此不存在字段级的跨语法拒绝（`variable.rs:6-9`）。
- 字段归属速查：用户交互 `prompt` 字段在装配层解析（`${...}`）；消息 `content_template` 稍后经 `{{input}}` 替换渲染。

## 2. 表达式求值 `ExprEvaluator`（`variable.rs:50-619`）

面向 VARIABLE 节点的递归下降表达式引擎：

- 支持：变量引用（`${path}`）、算术 `+ - * / %`、比较 `== != > < >= <=`、逻辑 `&& || !`、三元、一元负号、括号；不带引号的标识符按变量解析。
- 解析链：`evaluate`（`variable.rs:79-89`，拒绝尾部残留 token）→ `parse_ternary` → `parse_or` → `parse_and` → `parse_equality` → `parse_relational` → `parse_additive` → `parse_multiplicative` → `parse_unary` → `parse_primary`（`variable.rs:293-385`）。
- 运算语义：`truthy`（`variable.rs:417`）、`compare_values`（`variable.rs:437`）、四则与取模（`variable.rs:490-597`）。

## 3. 变量解析 `VariableResolver`（`variable.rs:621-743`）

- 递归解析字符串、对象与数组；**整串单一 `${path}` 引用保留值类型**，嵌入文本中的引用退化为展示字符串（`variable.rs:624-629`）。
- 可选 `SystemContext` 解析只读的 `execution.*` / `node.*` 路径。
- `resolve_str`（`variable.rs:655-700`）：完整 span 引用返回类型化值；未闭合 span 原样保留；缺失变量记日志并原样保留，不置空。
- `lookup_variable`（`variable.rs:702-743`）：系统路径优先；`variables.x` 是**作用域前缀路径**——读扁平变量 `x`，不是嵌套键；点路径遍历对象属性与数组下标。

## 4. `evaluate_expression` 与语义错误策略（`variable.rs:745-858`）

- `looks_like_expression`（`variable.rs:801-852`）判定是否按表达式求值：`${...}` span 内部的运算符不计数；紧贴 span 边缘的运算符（如 `${a}-done`）视为模板文本。
- `${...}` 模板字面量的语法失败退化为插值（`variable.rs:760-771`）。
- **语义错误必须浮出**（`is_semantic_evaluation_error`，`variable.rs:785-794`）：缺失变量、类型不匹配、除零/取模零、溢出、非有限值不得被插值字符串掩盖。
- `convert_variable_type`（`variable.rs:859+`）：按声明的 `variableType`（number/string/boolean/array/object）转换，转换失败是显式错误而非静默强转；空文本不充当缺失值。

## 5. 变量存储

- `VariableStore = Arc<DashMap<String, Value>>`（`variable.rs:20`），`create_variable_store()` 分配共享并发映射。
- `ExpressionError`（`variable.rs:27-48`）携带表达式原文与错误信息。

## 6. 显式配置解析（`config_parse.rs`）

取代散落的 `from_value(...).ok()` / `unwrap_or_default()` 静默降级点：

- `parse_node_config`（`config_parse.rs:17-27`）：严格解析，失败返回结构化 `ConfigError { node_id, field, detail }`。
- `parse_node_config_or_warn`（`config_parse.rs:31-49`）：可选增强配置降级到默认值，但记录节点 id + 字段路径的告警。

## 7. 命名消息数组（`message_context.rs`）

供 LLM / AGENT_LOOP / TOOL_VISIBILITY 节点共享的命名上下文，存储在工作流变量映射的保留前缀下，因此天然参与 checkpoint：

- 保留键：`CONTEXT_PREFIX = "__msg_ctx__"`（`message_context.rs:17`）、历史前缀 `__msg_ctx_hist__`（`message_context.rs:23`）、每数组台账 `__msg_ledger__`（`message_context.rs:25`，只记录版本 + 发射守卫，预算决策时重扫投影）、默认上下文 `DEFAULT_CONTEXT_ID = "current"`、`COMPRESSION_DEPTH_KEY`（深度 > 0 阻止递归压缩发射，`message_context.rs:28-32`）。
- `get_context`（`message_context.rs:163-179`）：纯读；损坏条目降级为空并告警。
- `append_context`（`message_context.rs:183-201`）：追加，台账按 O(新增消息) 增量记账。
- `register_context`（`message_context.rs:209-240`）：**替换活跃视图但归档**——旧消息进入只增、按 message-id 去重的历史；台账重算。
- `get_context_history`（`message_context.rs:246-255`）：归档 + 活跃，去重合并（checkpoint 持久化的无损记录）；`restore_full_history`（`message_context.rs:272-286`）撤销最近一次视图切换——历史从未删除，撤销零成本。
- 辅助：`snapshot_branch_messages`（`message_context.rs:312`）、`merge_archived_history`（`message_context.rs:332`）、`seed_message_inputs`（`message_context.rs:364`）、压缩发射守卫 `should_emit_compression` / `mark_compression_emitted`（`message_context.rs:439-452`）、数组版本 `array_version`（`message_context.rs:427`，供压缩回写的版本检查）。

## 8. 变量映射与输出映射

### `handler/variable_mapping.rs`

- `inherit_all_variables`（`variable_mapping.rs:17-27`）：父→子全量复制，剔除引擎内部键（触发器子工作流使用；SUBGRAPH 保持隔离）。
- `apply_variable_inputs`（`variable_mapping.rs:34-88`）：`source_path → internal_name` 映射；必填缺失即失败，可缺省有默认值则回填，否则静默跳过。
- `apply_variable_outputs`（`variable_mapping.rs:92-118`）：子→父按 `internal_name → target_path` 复制。
- `set_variable_path`（`variable_mapping.rs:123-172`）：点路径写入，SYNC 节点复用。

### `handler/output_mapping.rs`

- `apply_output_mappings`（`output_mapping.rs:7-33`）：解析 `Vec<ScriptOutputMapping>`（畸形映射是结构化 `ConfigError`，不跳过），按点路径提取值写入 `variable`/`output` 目标；`extract_path_value` / `deep_clone_mapped`（`output_mapping.rs:35-73`）。

## 9. 变量注入（与主循环的衔接）

节点边界的变量注入（steering 输入）由 `coordinator/workflow/variable_injection.rs` 排水（见 [01-architecture.md](01-architecture.md)），实体侧缓冲与上限见 `entity.rs` 的 `pending_injections`（容量上限 `MAX_PENDING_VARIABLE_INJECTIONS`，`entity.rs:69`）；注入缓冲不参与快照，崩溃即丢弃未消费注入。
