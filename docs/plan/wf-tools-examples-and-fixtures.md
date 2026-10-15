# wf-tools 工具示例与 Fixtures 调试方案

> 目的：为 `crates/engine/wf-tools` 中的每个预定义工具提供可复现的 example 与 fixtures，把每次调用的结果（含 schema、错误、产物）落盘到该 crate 内部的 `outputs/` 目录，供人工审查与调试。
> 约定：本文遵循 AGENTS.md——中文文档、以自然语言描述为主、避免完整代码段；事实声明均附 `file:line` 引用。

## 一、背景与动机

`wf-tools` 目前有 43 个预定义工具，全部由单一定义源描述：

- 每个工具是一个 `ToolDefinition` 常量，同时派生出「注册侧 schema」与「LLM 侧描述」，保证两者不漂移（`crates/engine/wf-tools/src/predefined/schema.rs:107`、`:122`、`:178`）。
- 全部定义按类别聚合，并由 `all_definitions()` 统一枚举（`crates/engine/wf-tools/src/predefined.rs:39`、`:74`）。
- 执行统一走 `ToolRegistry::execute_tool`（`crates/engine/wf-tools/src/registry.rs:257`），按 `ToolType` 分派到 Stateless / Stateful / BuiltIn / Mcp / Rest 五个执行器（`crates/engine/wf-tools/src/registry.rs:152`）。

现状的调试能力存在明显缺口：

- 单元测试集中在各自模块（如 `predefined/memory/tests.rs`、`predefined/shell/tests.rs`），断言内联、无落盘产物，无法「人眼审查一次真实调用的输入输出」；
- 集成测试只有 `tests/mcp_integration.rs` 一个，覆盖 MCP 一条链路；
- 没有任何机制保证「新增工具时同步补齐示例」，工具 schema、参数默认值、错误分支容易长期无人验证。

项目里已有成熟范例可参照：`wf-cli-demo` 用 `examples/component_output.rs` 把渲染结果写进 crate 内部的 `outputs/`，形成可审查、可 diff 的黄金产物（`crates/app/cli/wf-cli-demo/examples/component_output.rs:24`、`:92`）。本方案把这套「example + 黄金输出」模式迁移到 `wf-tools`，但输入侧改为 **fixtures 驱动**。

## 二、目标与非目标

目标：

1. 每个预定义工具至少有一条可执行 example（fixture 用例），输入参数、种子文件、期望断言全部外置为 fixtures；
2. 每次运行把结果写入 `crates/engine/wf-tools/outputs/`，包含调用结果、工具 schema 快照、文档示例，便于人工审查与回归 diff；
3. 能在无网络、无外部服务的默认模式下跑通「纯本地工具」，并可在 CI 做黄金输出校验；
4. 新增工具时若缺少 fixture，测试直接失败，避免覆盖面随工具增长而腐化。

非目标：

- 不替代现有单元测试；本方案面向「端到端、可观察、可落盘」的调用审查；
- 不引入真实 LLM、真实 workflow/agent 引擎；需要引擎回调的工具使用确定性 stub；
- 不改动工具的运行时语义，示例只消费公开 API。

## 三、工具清单与可执行性分级

按 `all_definitions()` 的注册顺序（`crates/engine/wf-tools/src/predefined.rs:41`），共 43 个工具。`Tier` 表示运行所需的外部依赖，决定默认是否纳入 CI 黄金校验：

| Tier | 含义 | 是否默认执行 |
|------|------|--------------|
| `offline` | 仅本地文件系统 / 内存 / 临时 SQLite，确定性 | 是 |
| `process` | 需本地子进程 / PTY | 否（可选） |
| `stub` | 需注入 engine 回调 / skill 加载器，用确定性 stub 替代 | 是（用 stub） |
| `network` | 需真实或 mock HTTP 服务 | 否（可选） |
| `mcp` | 需 MCP 服务端（可复用测试脚本 stub） | 否（可选） |

### 3.1 filesystem（8）

| id | ToolType | 风险 | 执行来源 | Tier |
|----|----------|------|----------|------|
| read_file | Stateless | ReadOnly | `FsToolHandlers::read_file` | offline |
| write_file | Stateless | Write（checkpoint Before） | `FsToolHandlers` | offline |
| edit_file | Stateless | Write（Before） | `FsToolHandlers` | offline |
| apply_patch | Stateless | Write（Both） | `FsToolHandlers` | offline |
| apply_diff | Stateless | Write（Before） | `FsToolHandlers` | offline |
| list_files | Stateless | ReadOnly | `FsToolHandlers` | offline |
| grep_search | Stateless | ReadOnly | `FsToolHandlers` | offline |
| glob_search | Stateless | ReadOnly | `FsToolHandlers` | offline |

聚合与注册见 `crates/engine/wf-tools/src/predefined/filesystem.rs:32`、`:45`；配置类型 `FsToolConfig` 见 `crates/engine/wf-tools/src/filesystem.rs:20`。

### 3.2 shell（10）

| id | ToolType | 风险 | Tier |
|----|----------|------|------|
| execute_command | Stateless（async） | Execute | process |
| backend_shell | Stateful | Execute | process |
| shell_output | Stateful | ReadOnly | process |
| shell_wait | Stateful | ReadOnly | process |
| shell_kill | Stateful | Execute | process |
| shell_send_input | Stateful | Execute | process |
| shell_resize | Stateful | ReadOnly | process |
| get_or_create_shell | Stateful | Execute | process |
| execute_in_session | Stateful | Execute | process |
| release_sessions_for_task | Stateful | Execute | process |

聚合与注册见 `crates/engine/wf-tools/src/predefined/shell.rs:42`、`:63`。该组依赖 `BackgroundShellStore` 与 `ShellToolConfig`，并有 checkpoint/session 观察链路（`crates/engine/wf-tools/src/predefined/shell/checkpoint_handle.rs:16`）。

### 3.3 memory（7）

| id | ToolType | 风险 | Tier |
|----|----------|------|------|
| record_note | Stateful | Write | offline |
| recall_notes | Stateful | ReadOnly | offline |
| list_categories | Stateful | ReadOnly | offline |
| session_note | Stateful | Write | offline（临时 SQLite） |
| memory_remember | Stateful | Write | offline |
| memory_forget | Stateful | Write | offline |
| memory_list | Stateful | ReadOnly | offline |

注册见 `crates/engine/wf-tools/src/predefined/memory.rs:46`；`session_note` 的 db 路径可在注册时注入（`crates/engine/wf-tools/src/predefined/memory/session_note.rs:274`），测试已示范用临时库（`crates/engine/wf-tools/src/predefined/memory/tests.rs:104`）。

### 3.4 utility（1）

| id | ToolType | 风险 | Tier |
|----|----------|------|------|
| update_todo_list | Stateless | ReadOnly | offline |

### 3.5 web（2）

| id | ToolType | 风险 | Tier |
|----|----------|------|------|
| web_search | Stateless（async） | Network | network |
| web_fetch | Stateless（async） | Network | network |

配置类型 `WebToolConfig` 见 `crates/engine/wf-tools/src/predefined/web.rs:25`；`web_search` 的 DuckDuckGo HTML 解析是纯函数，可用固定 HTML fixture 离线验证（`crates/engine/wf-tools/src/predefined/web/web_search.rs:119`）。

### 3.6 code_context（6）

| id | ToolType | 风险 | Tier |
|----|----------|------|------|
| code_search | Stateless（async） | ReadOnly | network |
| code_keyword_search | Stateless（async） | ReadOnly | network |
| read_file_folded | Stateless（async） | ReadOnly | offline（无服务时静默降级） |
| code_symbols | Stateless（async） | ReadOnly | network |
| code_references | Stateless（async） | ReadOnly | network |
| code_definition | Stateless（async） | ReadOnly | network |

注册见 `crates/engine/wf-tools/src/predefined/code_context.rs:432`；`read_file_folded` 在服务未配置时回退为普通读取（`crates/engine/wf-tools/src/predefined/code_context.rs:375`），其「未配置」分支已被现有测试覆盖（`:518`）。

### 3.7 workflow / agent / interaction / integration / knowledge / general（9）

| id | ToolType | 风险 | 执行来源 | Tier |
|----|----------|------|----------|------|
| execute_workflow | BuiltIn | System | ExecutionCallback | stub |
| query_workflow_status | BuiltIn | ReadOnly | ExecutionCallback | stub |
| cancel_workflow | BuiltIn | System | ExecutionCallback | stub |
| call_agent | BuiltIn | System | ExecutionCallback | stub |
| skill | BuiltIn | ReadOnly | SkillLoader | stub |
| general | BuiltIn | ReadOnly | GeneralToolInvoker | stub |
| ask_followup_question | BuiltIn | Interaction | agent loop 拦截 | stub |
| attempt_completion | BuiltIn | Interaction | agent loop 拦截 | stub |
| use_mcp | Mcp | Mcp | McpConnectionManager | mcp |

关键事实：默认注册的 builtin handler 只有 6 个——call_agent / execute_workflow / query_workflow_status / cancel_workflow / skill / general（`crates/engine/wf-tools/src/executor/builtin_handlers.rs:383`）。`ask_followup_question` 与 `attempt_completion` **没有** handler，它们是 agent 循环的信号工具，本方案对其只做 schema 快照与「信号语义」说明，不通过 registry 执行。

## 四、目录布局

所有产物都放在 `wf-tools` 包内部：

```
crates/engine/wf-tools/
├── Cargo.toml                 # 新增可选 feature `harness`
├── src/
│   └── harness.rs (+ harness/)  # feature 门控的复用测试台（非默认编译）
├── examples/
│   └── tool_showcase.rs       # 驱动入口：--all / --tool / --category / --tier / --check
├── fixtures/
│   ├── _manifest.json         # 全局开关（启用的 tier、默认超时等）
│   ├── filesystem/
│   │   └── read_file/
│   │       ├── basic.json
│   │       ├── offset_limit.json
│   │       └── missing_file.json
│   ├── shell/…
│   ├── memory/…
│   ├── web/…
│   └── …
├── outputs/                   # 生成物，纳入版本控制以便 diff（volatile 除外）
│   ├── _index.json
│   ├── read_file/
│   │   ├── schema.json
│   │   ├── doc_examples.txt
│   │   └── basic.json
│   └── …
└── tests/
    └── tool_golden.rs         # 黄金输出校验 + 覆盖度守卫
```

理由：`wf-cli-demo` 已确立 `examples/ + outputs/` 的目录惯例；把复用逻辑放进 feature 门控的 `src/harness`，可同时被 example 与集成测试引用，避免复制粘贴，且不污染默认构建的公开面。examples 不能依赖 lib 的 dev-dependencies，因此 harness 只使用 `serde_json` / `tokio`（均为常规依赖）与标准库临时目录，不新增依赖。

## 五、Runner 设计

### 5.1 装配顺序

1. `ToolRegistry::new()`——已内置五类执行器与 6 个 builtin handler（`crates/engine/wf-tools/src/registry.rs:41`、`:57`）。
2. 用自定义 `BuiltinHandlersConfig` 调 `register_builtin_handlers`（`crates/engine/wf-tools/src/handlers.rs:39`），其中：
   - `FsToolConfig.workspace_dir` 指向本次用例的隔离临时目录，保证相对路径与写入落到沙箱；
   - `ShellToolConfig` 的 cwd 同步该目录；
   - `WebToolConfig` / `CodeContextConfig` 按 tier 决定是否配置。
3. 注册**全部**工具定义：`for def in predefined::all_definitions() { registry.register_tool(def.tool_def()); }`。
   - 注意：`register_builtin_handlers` 只装配 handler/factory，只有 `knowledge` 与 `general` 会顺带注册自身定义（`crates/engine/wf-tools/src/predefined/knowledge.rs:18`、`crates/engine/wf-tools/src/predefined/general.rs:55`），其余定义必须由 runner 显式注册，否则 `execute_tool` 会报 `NotFound`。
4. 构造 `ToolExecutionContext`（`crates/engine/wf-tools/src/executor/trait_def.rs:16`）：`execution_id` 取 fixture 指定值（缺省用稳定值如 `fixture-exec`），按需注入 `general_invoker` / `checkpoint_session` / `cancellation`。
5. 调 `registry.execute_tool(id, &params, &options, &ctx)`（`crates/engine/wf-tools/src/registry.rs:257`），捕获 `ToolExecutionResult`。

### 5.2 需要 stub 的能力

- **ExecutionCallback**：为 `call_agent` / `execute_workflow` / `query_workflow_status` / `cancel_workflow` 实现一个确定性 stub，返回 canned 输出（trait 定义见 `crates/engine/wf-tools/src/callback.rs:119`；全局注册入口 `:216`，也可只放进 `BuiltinHandlerResources.callback` 局部生效，`crates/engine/wf-tools/src/executor/builtin_handler.rs:24`）。
- **SkillLoader**：为 `skill` 提供一个指向 fixture 技能目录的加载器，或用最小 skill 内容验证模板替换路径。
- **GeneralToolInvoker**：为 `general` 注入一个把内部请求原样回显（或路由到其它已注册工具）的 invoker，验证代理语义（`crates/engine/wf-tools/src/general.rs`）。
- **McpConnectionManager**：为 `use_mcp` 复用 `tests/mcp_integration.rs` 的 shell 脚本 stub server（`crates/engine/wf-tools/tests/mcp_integration.rs:13`），注册后走真实 JSON-RPC 往返。

### 5.3 归一化

为保证黄金输出可 diff，runner 写盘前对结果做确定性归一：

- 临时/沙箱绝对路径 → `<WORKSPACE>`；
- 生成的时间戳 → `<TIMESTAMP>`；
- `session_id` / `pid` / 随机 id → 稳定占位符（或按出现顺序编号）；
- `execution_time` 等耗时字段 → `0` 或删除；
- `error` 文本中的路径同样替换。

归一规则集中实现于 harness，并在文档中列明，避免各处重复。

## 六、Fixture 模型

每个用例是一个 JSON 文件，字段建议：

| 字段 | 必填 | 说明 |
|------|------|------|
| `tool` | 是 | 工具 id，必须存在于 `all_tool_ids()` |
| `description` | 否 | 用例意图，写入 outputs 便于审查 |
| `tier` | 否 | 覆盖 manifest 的默认 tier（用于同一工具的正/负样例） |
| `workspace` | 否 | 沙箱种子：文件 → 内容；可选目录结构 |
| `setup` | 否 | 观测调用之前按顺序执行的预备工具调用（`{tool, parameters}`），仅取副作用不记录结果，用于驱动有状态存储（如先 `record_note` 再 `recall_notes`） |
| `context` | 否 | `execution_id`、`metadata` 等 |
| `parameters` | 是 | 直接传给 `execute_tool` 的参数对象 |
| `expect` | 否 | 断言：`success`、`error_contains`、`result_contains` |

示例（节选，仅示意结构）：

```json
{
  "tool": "read_file",
  "description": "read a small seeded file",
  "tier": "offline",
  "workspace": { "files": { "notes/hello.txt": "hello\nworld\n" } },
  "parameters": { "path": "notes/hello.txt", "limit": 1 },
  "expect": { "success": true, "result_contains": "hello" }
}
```

fixtures 只读、可提交；`workspace` 里的种子在每次运行时物化到一次性临时目录，避免污染仓库。

`network` tier 的用例无法指向固定端口，故约定用占位符 `<MOCK_HTTP_BASE>`：runner 启动一个进程内的 mock HTTP 服务（随机端口、惰性单例），并把 fixture 字符串里出现的占位符替换为该服务的真实 origin 后再执行。`web_search` 的 `search_endpoint` 也指向同一 mock；`/page` 路径返回供 `web_fetch` 使用的固定 HTML。这样 web 组无需外网即可获得确定性结果。

## 七、输出布局与产物

每个用例产出 `outputs/<tool_id>/<case>.json`，内容包含：

- 归一化后的 `ToolExecutionResult`（`success` / `result` / `error` / `retryable`）；
- 回填的 fixture 摘要（tool、description、tier、parameters）；
- 断言结果（供快速定位失败）。

此外每工具额外产出：

- `outputs/<tool_id>/schema.json`——`ToolDefinition.tool_def()` 序列化结果（注册侧 schema），用于审查参数、默认值、`additionalProperties:false`、enum 等；
- `outputs/<tool_id>/doc_examples.txt`——`ToolDefinition.description_data()` 中的 tips / examples，把「文档示例」与「可运行示例」对齐；
- `outputs/_index.json`——全部工具与用例的清单及状态汇总。

不同 tier 的产物隔离：`offline` / `stub` 产物提交入库并纳入黄金校验；`process` / `network` / `mcp` 产物写入 `outputs/_volatile/<tool_id>/`，该目录被 `outputs/.gitignore` 排除、默认不纳入 diff，`--check` 也会跳过这些用例。

`_index.json` 只在整轮（无过滤）运行时重写，避免 `--tier <t>` 之类的过滤运行把完整索引覆盖成子集。

## 八、命令与用法

统一入口（feature 门控）：

```sh
# 跑通全部默认 tier，写出 outputs/
cargo run -p wf-tools --features harness --example tool_showcase -- --all

# 只跑某个工具，便于单点调试
cargo run -p wf-tools --features harness --example tool_showcase -- --tool read_file

# 按类别或 tier 过滤
cargo run -p wf-tools --features harness --example tool_showcase -- --category filesystem
cargo run -p wf-tools --features harness --example tool_showcase -- --tier process

# CI 黄金校验：不写盘，逐文件比对，有漂移则非零退出
cargo run -p wf-tools --features harness --example tool_showcase -- --check

# 更新黄金输出
cargo run -p wf-tools --features harness --example tool_showcase -- --all --update
```

CLI 使用参数解析而非 `clap`，控制依赖面；`--tool <id>` 除写盘外同时把结果打印到 stdout。

## 九、覆盖度守卫与回归

- **覆盖度守卫**（`tests/tool_golden.rs`，feature 门控）：遍历 `all_definitions()`，要求每个 id 要么存在 `fixtures/**/<id>/` 目录，要么命中一份显式的 `schema_only` 白名单（仅 `ask_followup_question` / `attempt_completion` 这类由 agent 循环拦截、registry 无 handler 的工具）。新增工具而漏配 fixture 时测试失败。
- **黄金校验**：对 `offline` / `stub` tier 的所有用例，把当前运行结果与已提交 `outputs/` 逐字段比对（先归一化）。纳入 CI，防止 schema/默认值/错误文案在无感中漂移。
- **正负样例**：每个工具至少一条成功样例；对有稳定错误分支的工具（如 `read_file` 文件不存在、`session_note` 未知 operation）补一条失败样例，锁定错误语义。

## 十、实施步骤（分阶段）

1. **阶段一（骨架）**：新增 `harness` feature 与 `src/harness`，实现 registry 装配、fixture 解析、临时沙箱、归一化、输出写出；接入 `examples/tool_showcase.rs` 与 `--check`。
2. **阶段二（offline）**：补齐 filesystem / memory / utility / `read_file_folded` 的 fixtures 与黄金输出；加覆盖度守卫与黄金测试。
3. **阶段三（stub）**：实现确定性 `ExecutionCallback` / `SkillLoader` / `GeneralToolInvoker` stub，覆盖 call_agent、execute_workflow、query_workflow_status、cancel_workflow、skill、general，并为 interaction 两工具做 schema 快照。
4. **阶段四（process）**：接入 shell 组，用例控制短命令与稳定退出码；产物写 `_volatile`，默认不纳入 diff。
5. **阶段五（network / mcp）**：web 组优先用纯解析 fixture 离线验证解析逻辑，真实请求走可选开关；code_context 用本地 mock 或标记为需服务；`use_mcp` 复用现有 stub server。
6. **收尾**：在 `AGENTS.md` 或各 crate 说明中登记命令与 tier 约定。

## 十一、风险与取舍

| 风险 | 取舍 |
|------|------|
| shell / web / mcp 运行时不确定性 | 归入非默认 tier，产物隔离到 `_volatile`，不阻塞 CI |
| 时间戳、pid、随机 id 导致 diff 抖动 | 统一归一化占位符，规则集中且文档化 |
| 新增 feature 侵入 crate | feature 非默认，默认构建与 DAG 不变；`--all-features` 下参与编译检查 |
| stub 与真实引擎行为偏离 | stub 只覆盖「分派与参数解析」语义；真实行为由各自引擎测试负责 |
| `all_definitions()` 与 fixture 目录漂移 | 覆盖度守卫强制同步 |

## 十二、关键引用

- 定义源与枚举：`crates/engine/wf-tools/src/predefined/schema.rs:107`、`crates/engine/wf-tools/src/predefined.rs:39`
- 注册装配：`crates/engine/wf-tools/src/handlers.rs:39`、`:24`
- 执行入口与分派：`crates/engine/wf-tools/src/registry.rs:257`、`:152`
- 上下文与执行器：`crates/engine/wf-tools/src/executor/trait_def.rs:16`、`:148`；`crates/engine/wf-tools/src/executor/stateless.rs:15`；`crates/engine/wf-tools/src/executor/stateful.rs:13`
- builtin handler 与回调：`crates/engine/wf-tools/src/executor/builtin_handler.rs:35`；`crates/engine/wf-tools/src/executor/builtin_handlers.rs:383`；`crates/engine/wf-tools/src/callback.rs:119`
- 现有黄金输出范例：`crates/app/cli/wf-cli-demo/examples/component_output.rs:24`
- MCP stub 范例：`crates/engine/wf-tools/tests/mcp_integration.rs:13`

## 十三、实现状态

阶段一至阶段四已完成，阶段五完成 web 组：

- feature `harness`、`src/harness.rs`、`examples/tool_showcase.rs`、`tests/tool_golden.rs` 均已落地；
- **offline + stub**：覆盖 filesystem（8）、memory（7，含 `session_note`）、utility（`update_todo_list`）、`read_file_folded`、以及 6 个 builtin（`call_agent` / `execute_workflow` / `query_workflow_status` / `cancel_workflow` / `skill` / `general`），全部通过断言并与黄金输出一致；
- **stub 能力**：harness 内置确定性 `ExecutionCallback`（`call_agent` 等的 canned 响应）、指向用例 `skills/` 目录的 `SkillLoader`、以及回显型 `GeneralToolInvoker`；
- **process**：`execute_command` 正/负样例，产物写 `_volatile`，`--tier process` 可选运行；
- **network**：`web_search` / `web_fetch` 经进程内 mock HTTP 服务，产物写 `_volatile`，`--tier network` 可选运行；
- `outputs/` 已为全部 43 个工具生成注册侧 `schema.json` 与 `doc_examples.txt`；
- 覆盖度守卫：每个工具必须有 schema 快照；每个 offline/stub 工具必须有 fixture（`ask_followup_question` / `attempt_completion` 由 agent 循环拦截、无 handler，列入 `schema_only` 白名单）。

未纳入 fixture：`code_context` 的检索类工具（除 `read_file_folded`）与 `use_mcp` 需要真实外部服务（代码上下文服务 / MCP server），归入 `network` / `mcp` tier，仅保留 schema 快照，按需手动接入。
