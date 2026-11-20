# 工具调用失败信息：现状评估与统一错误契约方案

> 范围：`wf-tools` 的错误模型与执行器包装、`wf-agent` 的工具消息装配、`wf-workflow` 的工具消息装配、`wf-types` 的工具结果与消息元数据、`llm-suite-codec` 的 `is_error` 映射
> 定位：回答三件事——失败时模型实际看到什么、这些信息是否合理、是否应该改成统一整合的错误信息而不是原始错误

## 〇、结论摘要

| 问题 | 结论 | 关键证据 |
|---|---|---|
| 失败信息是否合理 | **部分合理，整体不合理**：失败原因分类与提示文案质量尚可，但同一条失败被三到四层前缀包裹、内部枚举名泄漏到模型文本、两个引擎（agent / workflow）给出的形态完全不同，且部分失败被当成成功 | 第二节、第四节 |
| 文件编辑失败是否给出完整调用内容 | **不给出，且现状是"该给的信息没给，不该给的信息也没给"**：`edit_file` 失败文本只有路径加一句建议，既不回显 `old_string`/`new_string`，也不给文件侧最近邻位置或片段；参数只存在于审计记录（`ToolCallRecord.arguments`），不进模型上下文 | `handlers_write.rs:274-315`、`runner.rs:270-286` |
| 是否应该统一 | **应该统一序列化点，但不应该"统一返回原始错误"**：需要一个结构化失败信封（code / message / hint / retryable / detail），由唯一函数序列化，代理、内置、MCP、工作流共用；`detail` 保留原始错误但走落盘裁剪，不再拼进给模型看的单行文本 | 第五节、第六节 |

最严重的单点缺陷：**工作流引擎在工具返回 `success=false` 时把失败原因整段丢弃，模型收到空内容**（`wf-workflow/src/handler/llm/tool_exec.rs:369-377`）。这不是风格问题，是信息丢失。

## 一、取证范围与复现方法

仓库以 `gh-proxy.com` 代理浅克隆，commit `16d399c`，子模块 `llm-suite`、`simvec` 已递归检出；依赖经 rsproxy 镜像 `cargo fetch` 通过。本文所有结论均以源码行为为依据，未运行整包构建。

两条可复核的推导规则：

1. 文本由 `thiserror` 的 `#[error(...)]` 属性逐字生成，因此复合串可由"包装点 + 属性字面量"还原，无需运行。
2. 模型可见文本只由两个装配函数产出：`wf-agent` 的 `runner::error_message`（`runner.rs:502-519`）与 `wf-workflow` 的 `tool_result_message`（`messages.rs:60-72`）。除此之外没有第三个入口，这点本身就构成后文的改造前提。

不在范围：`wf-api` 的 HTTP 错误信封、审批 UI、前端本地化文案。

## 二、失败信息的生产链路

一条失败要经过四层才到模型，每层都有机会改写文本，也每层都改写了一次：

| 层 | 职责 | 代码 | 对文本的影响 |
|---|---|---|---|
| 处理器 | 产生带业务语义的错误 | `filesystem/handlers_write.rs`、`patch_ops.rs` 等 | `Execution error: ...` / `Tool validation failed: ...` 的原始原因 |
| 执行器 | 把 `Err` 转成失败结果 | `executor/stateless.rs:130-136`、`executor/builtin.rs:145-152` | 用 `e.to_string()` 塞进 `error` 字段，**再叠一层变体名前缀** |
| 引擎 | 转成 `ToolError` 并决定重试 | `wf-agent/src/coordinator/tool/runner.rs:406-432` | 包成 `ExecutionFailed{tool_id, reason}`，**再叠一层"工具名 + 破折号"** |
| 装配 | 生成模型消息 | `runner.rs:502-519` | 再包一层 `{"error": "..."}` JSON |

链路结果是同一条原因被套了四层壳。以 `edit_file` 找不到 `old_string` 为例，模型收到的完整文本是：

```
{"error":"Tool execution failed: edit_file - Execution error: old_string not found in '/w/a.rs'. Provide more context in old_string to make it unique, or use apply_diff for complex edits."}
```

四段信息里只有最后一句对模型有用；前两段（`Tool execution failed:`、`Execution error:`）是内部结构，第三段（工具名）与消息的 `tool_name` 字段完全重复。

## 三、模型实际看到的形态：六种，语义互不兼容

| 编号 | 触发路径 | 模型看到的内容 | 构造点 |
|---|---|---|---|
| A | 内置/无状态工具硬失败 | `{"error":"Tool execution failed: <tool> - Execution error: <reason>"}` | `stateless.rs:130-136` → `runner.rs:406-423` |
| B | 参数 schema 或 JSON 解析失败 | `{"error":"Tool validation failed: <reason>"}` | `executor/base.rs:50-78`、`runner.rs:59-72` |
| C | 作用域违规 | `{"error":"Scope violation in tool '<tool>': scope_violation: <detail>"}` | `error.rs:53-55,78-83` |
| D | 结果里自带 `success:false`（`apply_patch`、`apply_diff` 零成功）或 `isError`（MCP）或非零退出（shell） | **成功消息**，内容里嵌 `"success": false` 与 `"error": "..."`，MCP 还会变成 `Error:\n<原文>` | `patch_ops.rs:175-179`、`shell.rs:502-511`、`mcp.rs:274-340` |
| E | 工作流引擎的失败 | `Error: <ToolError Display>`；**`success=false` 且无 result 时是空字符串** | `tool_exec.rs:369-390` |
| F | `general` 代理内的子调用失败 | 外层成功，内层 `{"error":"<ToolError Display>"}`，批量时是数组按下标对齐，条目里没有工具名 | `coordinator/tool/general.rs:143-184,271-279` |

同一件事（"这个工具没能完成"）在六个地方有六种字节形态，其中 D 和 F 的 `{"error": ...}` 与 A 的载荷**字面同形但成功位相反**，模型无法区分"硬失败"和"代理层拒绝"。

## 四、逐项合理性评估

### 4.1 是否给出"完整调用内容"

不给出，且现状两头都不占优。

- 不回显参数：`edit_file` 的失败文案只含路径与建议（`handlers_write.rs:283-315`），不支持参数的审计面在模型侧完全没有。
- 也不给可自纠的上下文：没有最近邻匹配位置、没有冲突行号、没有片段。`edit_diff.rs` 的模糊匹配（`seek_sequence`）在 `NotFound` 时只回一个布尔式结果，位置信息被丢弃（`edit_diff.rs:141-154`）；对照 `patch/v4a.rs:323,350` 已经有 ` near line: N` 与 ` near "..."` 的提示能力，文件工具没有对齐。

判断：**"回显完整调用内容"不该做**。参数是模型自己刚发出的、仍在上下文里的内容，回显纯属重复计费；真正缺的是文件侧证据。所以正确的替代不是"把调用原样返回"，而是给出"错在哪一行、附近是什么"的最小上下文（建议上限 3 至 5 行），并对超限部分走落盘指针。

### 4.2 前缀堆叠与内部枚举名泄漏

`Tool execution failed: X - Execution error: Y` 是两层包装的副产物：执行器把 `Display` 当数据传递（`stateless.rs:132`），引擎再包一次（`runner.rs:417-420`）。`ValidationFailed` 路径更夸张，会出现 `Tool execution failed: edit_file - Tool validation failed: ...`，"Tool" 一词出现两次。

同时，`Execution error`、`Tool validation failed`、`MCP error` 这些**内部变体名**对模型没有信息量，却占 token 且暗示了并不存在的语义层次。

### 4.3 标签契约与实现不符

`error.rs:64-75` 声明"稳定机器可读标签置于模型可见错误文本的开头"，但：

- 实际位置不在开头。`not_found:` 被包在 `Execution error:` 与 `Tool execution failed:` 之后，成了文本中段（`handlers_write.rs:246-258` + 上述包装）。
- 没有消费者。全仓库除测试断言外无任何代码解析 `NOT_FOUND_TAG`（`grep` 仅命中 `tests.rs:756,780,793` 与两处测试消息）。所谓"机器可读"目前是装饰。
- 真正的机器信号是元数据位 `scope_violation` 与 `is_error`，与文本标签并行存在，两套机制做同一件事。

### 4.4 双引擎分叉：工作流丢错误（最高优先级）

`wf-workflow` 的 `execute_tool_call` 在成功分支只取 `exec_result.result`，`exec_result.error` **从未被读取**（`tool_exec.rs:369-377`，全文件对 `.error` 无引用）。而内置工具失败一律是 `build_result(false, None, Some(err))`（`stateless.rs:130-136`），`result` 为 `None`，于是模型收到的工具消息是**空字符串 + `is_error` 元数据**。

后果：工作流里的文件编辑失败，模型既看不到原因也看不到建议，只能靠猜或直接重试；`is_error` 只在 Anthropic 编码器映射到线缆（`codec/.../anthropic/messages.rs:365-368`），OpenAI 兼容端点直接丢弃（`codec/.../openai_chat/messages.rs:196` 只发 `role/content`），也就是说在 OpenAI 系模型上这个失败**完全不可见**。

### 4.5 success 在三处不一致

| 位置 | 判定来源 | 问题 |
|---|---|---|
| 处理器载荷 `"success"` | 工具自己 | `apply_patch` 部分失败写 `false`（`patch_ops.rs:175-179`） |
| `ToolExecutionResult.success` | 执行器 | 只要处理器返回 `Ok` 就是 `true`，载荷里的 `false` 被忽略（`stateless.rs:112-127`） |
| 流事件 `ToolEnd.success` | `failure.is_none()`（`streaming.rs:421-430`） | 与上面两者都不是同一判据 |

具体后果三条：

1. **MCP 的 `isError` 永远成不了失败**：`mcp.rs:340` 无条件 `build_result(true, ...)`，`isError` 只用于统计与给文本加 `Error:\n` 前缀（`mcp.rs:274-278,363-367`）。于是 `ToolEnd.success=true`、失败保护不计数、指标记为成功。
2. **`apply_patch` 部分失败不触发任何失败机制**：模型看到 `success:false` 的载荷被装进一条成功消息，`failure_protection` 不累加（`runner.rs:359-361` 只记成功）。
3. **shell 非零退出永远是成功**（`shell.rs:502-511`）——这一点本身可接受（退出码是数据不是工具故障），但它依赖"模型会读 `success` 字段"，而 UI 侧收到的是 `success=true`，人看到的是绿色成功。

### 4.6 结构化通道闲置

项目已经有结构化错误能力，但没接到模型面：

- `ToolExecutionResult.retryable`（`wf-types/src/tool/execution.rs:41-45`）注释明确写着"错误文本不承担协议职责，重试循环读这个字段而不是解析消息"，但只有 REST 执行器设置它（`executor/rest.rs:370`），内置工具恒为 `None`，agent 引擎的失败重试只按预算计数（`runner.rs:235-248`，不看 `retryable`），`error_analysis` 另走一套 `ToolError` 变体分类（`error_analysis.rs:66-171`）。三条重试口径，只有一条在跑。
- `ErrorAnalysis` 产出的 `kind / error_type / recovery_action` 从不进入模型文本，模型拿不到"这是确定性失败，别重试"这个最重要的判断。
- `is_error` 元数据只有工作流装配（`messages.rs:66-72`），agent 装配不带（`runner.rs:507-518` 的 `metadata: None`），同一条失败在两条链路上的机器语义不同。

### 4.7 无长度上限，聚合规范缺位

- 统一落盘层只作用于结果值（`tool_output.rs:1-11`），**错误文本不过任何裁剪**：`mcp.rs:346-352` 直接把 `e.to_string()` 塞进 `error`；`apply_diff` 把每个失败块的文本拼成一整段（`patch_ops.rs:399-404`）。一个 200 块的补丁可以生成任意长的失败消息，直接进模型上下文，没有任何上限。
- 聚合只在 `apply_diff` 实现（一次性汇总所有块失败），`edit_file` 批量是首个失败即返回（`handlers_write.rs:274-315`，`apply_edits_to_content` 遇错即刻 `return`）。同样是批量改写，一个给全量失败清单，一个只给第一条 —— 模型修复一轮要重新试错多轮。

### 4.8 分类信息在观测侧也丢了

指标把所有失败压成同一个标签 `execution_failed`（`runner.rs:310-314,326-330`），`Ok(Err(e))` 与 `Ok(Ok(result.success=false))` 无法区分；审计记录 `call_error` 存的是带前缀的复合串（`runner.rs:257-268`），事后要做统计只能对文本做正则。

### 4.9 附带发现

- `wf-tools/src/general.rs:70` 的格式错误示例里含中文 `rust 异步`，与 `AGENTS.md` "任何代码文件不得出现中文"冲突。
- `[general-unavailable]` 前缀被 `is_invoker_missing` 用 `starts_with` 匹配（`general.rs:130-132`），又是在用错误文本承担协议职责，与 4.6 的结论方向相反。

## 五、是否应该统一：结论与边界

**结论：统一，但统一的对象是"失败的序列化"，不是"失败的来源"。**

三条边界必须先说清，否则容易做成另一个错误：

1. **不要统一成"回显原始错误"。** 原始错误（`io::Error`、serde 报错、MCP 服务端原文）对模型多是噪声，对排查有价值。正确做法是拆成两层：`message`（单行、人话、可直接行动）与 `detail`（原始文本，按需取用、走落盘）。
2. **不要给成功结果也套信封。** 成功结果是模型消费的主体，包一层只增加 token 与解析失败面；只标准化失败即可获得全部可判定性。
3. **不要把内部枚举名暴露给模型。** 变体名（`Execution error`、`MCP error`）是 Rust 实现细节，对外只暴露稳定 `code`。

统一能解决的具体问题：4.2 前缀堆叠、4.3 标签错位、4.4 双引擎分叉、4.5 成功位不一致、4.7 长度与聚合、4.8 观测分类。4.1 与 4.6 是能力补齐，靠同一处扩展字段实现。

## 六、建议的错误契约

单一失败信封，唯一序列化函数（建议置于 `wf-tools`，与 `tool_output` 平级，作为模型可见错误的唯一出口）：

| 字段 | 类型 | 必有 | 语义 |
|---|---|---|---|
| `code` | 稳定枚举 | 是 | `not_found` / `scope_denied` / `param_invalid` / `ambiguous_match` / `patch_failed` / `write_protected` / `approval_denied` / `tool_blocked` / `timeout` / `cancelled` / `upstream_error` / `internal` |
| `message` | 单行文本 | 是 | 人话，含对象（路径/行号/次数），不含内部枚举名，不含工具名（工具名已在线缆字段） |
| `hint` | 单行文本 | 否 | 下一步动作，限定一到两句，动词开头 |
| `retryable` | 布尔 | 是 | 与引擎重试判定同源，不允许两处各算一次 |
| `detail` | 文本或落盘指针 | 否 | 原始错误/服务端原文/多块失败清单，超限走 `truncate_with_spill` |
| `context` | 结构化小对象 | 否 | 自纠证据：`near_line`、`near_text`、`match_count`、`failed_indices` |

形态（协议形状，不是实现片段）：

```
{"error":{"code":"ambiguous_match","message":"old_string matches 3 places in /w/a.rs","hint":"Add surrounding lines to make it unique, or use apply_diff for multi-block edits.","retryable":false,"context":{"match_count":3}}}
```

硬规则四条：

1. 一个失败只有一段人类可读句；任何前缀只能来自本信封，不允许调用方再拼。
2. 部分成功也是失败：`apply_patch`、`apply_diff`、`general` 批量在存在失败条目时置 `success=false`，失败条目进 `detail`，条目数进 `context.failed_indices`。
3. `is_error` 元数据与线缆 `is_error` 由序列化函数一并设置，agent 与 workflow 两条链路共用同一个函数。
4. 错误文本与 `detail` 同样过落盘层，`message` 与 `hint` 有硬字符上限（建议各 200 与 300 字符）。

## 七、改造点清单（按依赖顺序）

| 序号 | 位置 | 动作 | 价值 |
|---|---|---|---|
| 1 | 新增 `wf-tools/src/tool_failure.rs` | 定义信封类型与唯一序列化函数；`tool_error_analysis` 的结果作为 `retryable` 与 `code` 的来源 | 建立单一出口 |
| 2 | `executor/stateless.rs:130-136`、`executor/builtin.rs:145-152` | 不再把 `Display` 塞进 `error` 字段，改存结构化失败（或原样返回 `ToolError`），去掉一层前缀 | 消除双层包装 |
| 3 | `wf-agent/src/coordinator/tool/runner.rs:406-432,502-519` | 用新函数序列化，设置 `is_error` 元数据，去掉 `Tool execution failed: ...` 拼接 | 模型文本归一 |
| 4 | `wf-workflow/src/handler/llm/tool_exec.rs:369-390` | **必须修**：`success=false` 时改用 `exec_result.error`（走新函数），不得产出空内容 | 修复信息丢失 |
| 5 | `executor/mcp.rs:274-340` | `isError` 置 `success=false`；文本前缀 `Error:\n` 取消，原文进 `detail` | 成功位诚实 |
| 6 | `filesystem/patch_ops.rs:175-179,399-404` | 部分失败进 `detail` 与 `context.failed_indices`，`success` 按失败条目数取反 | 部分成功可判定 |
| 7 | `filesystem/handlers_write.rs:274-315`、`edit_diff.rs:141-154` | 批量编辑改为收集全部失败下标；`NotFound` 时补 `near_line` / `near_text`（复用 `v4a.rs:323,350` 的提示能力） | 一轮修复替代多轮试错 |
| 8 | `coordinator/tool/general.rs:143-184,271-279` | 子调用失败改用同一信封并带上 `tool`；不再用 `{"error": ...}` 冒充成功载荷 | 消除 A 与 F 的同形歧义 |
| 9 | `coordinator/iteration/streaming.rs:421-430` | `success` 改读工具结果的成功位，而不是 `failure.is_none()` | UI 与模型一致 |
| 10 | `error.rs:64-75` 与 `tests.rs:756,780,793` | 删除文本级标签，改由 `code` 承担；测试改为断言 `code` | 去掉装饰性契约 |
| 11 | `wf-types/src/tool/execution.rs:41-45`、`runner.rs:235-248` | 让 `retryable` 成为唯一重试口径（或删掉字段，二选一，不要并存） | 消除三套重试口径 |
| 12 | `general.rs:70` | 示例改英文 | 对齐 `AGENTS.md` |

## 八、迁移与兼容

- `AGENTS.md` 明确项目不做向后兼容（第 5-6 行），因此可以一次性切换，不需要双写。
- 历史消息归一入口已存在：`general_history.rs` 与 `predefined/fold/`，旧格式（`{"error": "<string>"}`、`Error: ...`）在入库或折叠时归一为新信封，避免回放与直播形态分叉。
- 需要同步更新断言的测试集中在 `filesystem/tests.rs`、`wf-agent/src/coordinator/tool/test.rs:1120-1135`、`wf-workflow/src/handler/llm/tool_exec.rs:395`（测试模块起点）。
- 前端一侧：`ToolEnd.error` 目前直接塞全文（`streaming.rs:421`），改后应展示 `message`，`detail` 只在展开器中显示；`web-app` 与 `tui` 的错误预览行数预算（`tool-result-display-detail.md`）不需要改口径。
- 文档面：`docs/engine/tools/tool-output-handling.md` 的"机器标记一览"需要增加 `error.code` 一行，并说明它与 `truncated`/`output_path` 的关系。

## 九、验收标准

1. 任意内置工具失败，模型文本等于单一信封，且 `message` 中不含 `Tool execution failed`、`Execution error`、`Tool validation failed` 任一子串。
2. 工作流引擎在 `success=false` 时产出的消息内容非空，且与 agent 引擎对同一失败产出的信封字段一致。
3. MCP 服务端返回 `isError` 时，`ToolExecutionResult.success == false`、`ToolEnd.success == false`、失败保护计数加一。
4. `apply_patch` 三块中一块失败：`success == false`，`context.failed_indices == [idx]`，`detail` 含该块原因。
5. 失败消息的 `is_error` 元数据在 agent 与 workflow 两条链路上均存在。
6. 超过 200 块的 `apply_diff` 失败，模型消息长度不超过信封上限，全文可在落盘文件中读回。
7. `retryable` 只有一个来源：给定同一 `ToolError`，`error_analysis` 与信封字段一致（可用表驱动测试断言）。

## 十、决策定稿

九个开放点全部定稿，不再保留"倾向"列；后续实现与验收以本表为准。

| 编号 | 议题 | 决策 | 落地含义 |
|---|---|---|---|
| 1 | 信封位置 | 正文承载完整信封；`metadata` 只加 `is_error`（布尔）与 `error_code`（字符串） | 正文是模型唯一读取处，元数据只服务统计与前端判定，不复制 `message`/`hint` |
| 2 | 成功结果是否套信封 | 不套 | 成功结果保持现有 JSON 载荷与纯文本形态，仅失败走信封 |
| 3 | `code` 的可见性 | 公开契约 | 取值集合在 `wf-tools` 中固定并文档化，前端可按 `code` 选文案，新增取值视为协议变更 |
| 4 | 自纠上下文的形态与上限 | 位置与次数内联进 `message`（`nearest line N: <片段>` 最多一行，`match_count` 内联进 `message`）；结构化 `context` 只承载可枚举计数（`failed_indices`） | 不新增 `context.near_line`/`near_text` 字段；handler 层错误没有结构化载体，内联是唯一不扩 `ToolError` 的做法 |
| 5 | 部分成功 | `success=false` + `code=partial_failure` + `context.failed_indices`；不进入失败保护（不熔断），指标计一次失败 | 部分成功必须让模型与熔断器看到不同信号：模型要修，熔断器不该封工具 |
| 6 | `retryable` 来源 | 引擎分类为准（`wf-agent::error_analysis`）；`wf-tools` 提供 `default_retryable` 供无引擎侧（工作流、MCP）使用，并由跨 crate 一致性测试钉住两者相同 | 处理器不参与判定，只允许通过既有 `retryable` 字段收紧为 `false` |
| 7 | `detail` 中的原始文本 | 保留，且与结果一样过统一落盘层 | `detail` 是排障通道，必须有上限与可读回路径 |
| 8 | 失败文本语言 | 英文 | 与 `AGENTS.md` 一致，中文只出现在文档 |
| 9 | 暴露给模型的结构化深度 | 只暴露 `code`、`retryable`、`hint`、`context` | `ErrorKind`/`ErrorType`/`RecoveryAction` 属内部路由词汇，不进模型上下文 |

补充定稿（分析中新增、需一并记录）：

| 编号 | 议题 | 决策 |
|---|---|---|
| 10 | 文本级标签（`not_found:`、`scope_violation:`） | 降级为**生产侧编码**：`tag_detail` 仍是唯一写入口，但标签在信封渲染时被剥离并转成 `code`；标签永不进入模型文本，`code` 是唯一对外信号 |
| 11 | 部分成功的判定位置 | 统一在 `BaseExecutor` 做"载荷归一"：处理器返回 `Ok` 且载荷顶层 `success == false` 时，改判为失败结果（`code` 取载荷 `error_code`，`message` 取载荷 `error`）；不再由引擎各自嗅探 |
| 12 | 失败的审计与指标分类 | 审计记录 `call_error` 存信封 `code` + `message`（不再存复合串）；指标错误标签由 `execution_failed` 细化为 `code` |

## 十一、分阶段实施

七个阶段，每阶段独立可编译、可验收；后一阶段只依赖前一阶段的产物，不做跨阶段并行改动。

| 阶段 | 范围 | 主要文件 | 验收 |
|---|---|---|---|
| P1 | 信封基础：类型、`code` 取值、`ToolError`→信封映射、句读拆分、`detail` 落盘、渲染 | 新增 `wf-tools/src/tool_failure.rs`、`tool_output.rs`、`wf-types/tool/execution.rs`（加 `error_code`） | 单元测试：各变体的 `code`/`message`/`hint`/`retryable`；超长 `detail` 落盘可读回 |
| P2 | 执行器归一：不再拼 `Display` 前缀，载荷 `success:false` 改判失败 | `executor/base.rs`、`stateless.rs`、`builtin.rs`、`trait_def.rs` | 处理器错误进入结果时 `error` 不含变体名；`apply_patch` 部分失败即失败结果 |
| P3 | agent 装配：单点渲染、`is_error`/`error_code` 元数据、重试按 `retryable` | `coordinator/tool/runner.rs`、`coordinator/tool.rs` | 失败消息等于信封；非重试类失败不再消耗重试预算 |
| P4 | 工作流装配修复：`success=false` 不再产出空内容 | `handler/llm/tool_exec.rs`、`handler/llm/messages.rs` | 两条引擎对同一失败产出一致信封 |
| P5 | MCP 与部分成功：`isError` 改判失败、`apply_diff` 部分失败上报、失败列表进 `detail` | `executor/mcp.rs`、`filesystem/patch_ops.rs` | `isError` 时三层 `success` 全为 `false` |
| P6 | 文件编辑自纠：批量收集全部失败、`nearest line` 提示 | `filesystem/handlers_write.rs`、`patch/search_replace.rs` | 批量两处失配一次返回两条；位置提示可断言 |
| P7 | 收口：`general` 代理信封带 `tool`、流事件成功位、标签退场、`general.rs` 中文清理、文档同步 | `coordinator/tool/general.rs`、`coordinator/iteration/streaming.rs`、`error.rs`、`general.rs` | `cargo check` 通过；一致性测试与既有断言全绿 |

阶段间不保留旧格式双写：`AGENTS.md` 已声明不做向后兼容，历史消息由 `general_history` 与折叠层归一。

## 十二、风险与非目标

风险：

1. 失败文本是现有测试断言的对象，第 12 项改造会一次性打掉多处断言，需要同批更新，避免 CI 红一段。
2. `general` 代理的批量结果当前靠"下标加内容"定位，改为带 `tool` 的条目后，历史归一必须能识别旧形态，否则回放中同一会话出现两种条目结构。
3. MCP `isError` 改判失败会改变现有指标口径（成功率下降），需要提前告知依赖该指标的使用方。

非目标：不重写 `ToolError` 枚举本身（变体划分是对的，问题在外层包装与序列化）；不引入跨进程错误协议；不改动审批与作用域判定逻辑；不把错误文本做成多模态或富内容。

## 十三、实施结果

七个阶段全部落地，`cargo check`、`cargo clippy`、`cargo fmt` 与相关单测通过。

| 阶段 | 结果 | 落地位置 |
|---|---|---|
| P1 | 完成 | 新增 `wf-tools/src/tool_failure.rs`（错误码枚举、信封、`from_tool_error`/`from_result`/`from_reason`/`from_payload`、单点渲染、明细落盘、元数据）；`wf-types` 结果类型加 `error_code` |
| P2 | 完成 | `executor/base.rs` 的 `build_failure` 与 `from_handler_outcome`；`stateless.rs`、`builtin.rs` 去掉显示前缀；`apply_patch`、`apply_diff` 部分成功上报失败码与失败下标 |
| P3 | 完成 | `coordinator/tool/runner.rs` 一次分类供审计/指标/信封共用，消息由 `failure_message` 单点渲染；重试按可重试性收敛；`tool.rs` 三条路径改接 `ToolFailure` |
| P4 | 完成 | `wf-workflow/handler/llm/tool_exec.rs` 失败不再产出空内容，改用同一信封；同文件早期拒绝路径（参数、可见性、检查点会话）一并归入信封 |
| P5 | 完成 | `executor/mcp.rs` 的 `isError` 改判失败（错误码上游失败、不可重试），去掉 `Error:` 前缀；`apply_diff` 失败零块时打补丁失败码 |
| P6 | 完成 | `edit_file` 批量走 `apply_edits_collecting` 一次列全部失败条目；单条缺失时给出最近命中行号；`edit_diff.rs` 新增 `EditApplyError` 的描述与下标访问、`nearest_line` |
| P7 | 完成 | `general` 代理失败条目带工具名并保持信封；流事件错误字段改带单行消息；`error.rs` 说明标签为生产侧编码；`general.rs` 示例改英文 |

与分析阶段的出入（已按实况修正计划）：

1. 决策 4 的 `context.near_line` 未新增字段：处理器的失败只经字符串传递，因此最近行号内联进消息，`context` 只承载 `failed_indices` 这类可枚举计数。
2. 决策 10 的"删除文本级标签"落成"标签不再进入模型文本"：标签仍是生产侧唯一编码入口（`tag_detail`），渲染时转成错误码并剥离，既有断言因此保持有效。
3. 决策 11 的载荷归一放在 `BaseExecutor::from_handler_outcome`，由执行器统一改判，引擎不再各自嗅探。
4. 引擎侧重试增加可重试性闸门：确定性失败不再消耗共享重试预算，工作流与 MCP 无引擎分类时读 `default_retryable`，由跨 crate 一致性测试钉住与 `error_analysis` 相同。

已知遗留：

1. ~~工作区全量编译需要系统 `luajit`~~：已装（apt 走腾讯云镜像，`libluajit-5.1-dev` 2.1），`cargo check --workspace --all-targets` 通过，`wf-api`/`wf-runtime`/`wf-server`/CLI 全部纳入验证。
2. ~~前端尚未按 `error.code` 做本地化~~：已跟进，见第十四节。前端按码分支、按信封取建议动作，历史记录也能从信封恢复单行消息。
3. `wf-agent --test hook` / `--test timeout` 的失败已修：两者直接构造循环上下文，漏了运行时启动本该做的 `set_process_compaction_limits`，而该查询按设计"缺装即响"（`strategy.rs:113-118`），因此 panic。按 `wf-api` 同类测试的既有惯例补 `install_compaction_limits()`（`Once` 守卫 + `builtin_compaction_limits()`），两组共 6 项现在全过。
4. ~~`wf-workflow` 的 `preprocess::tests::duplicate_fork_path_ids_emit_warning_but_stay_valid` 失败~~：已修复，见第十五节。根因不在被测代码，而在夹具本身。

## 十四、OpenAPI 与前端同步

本轮的失败信封改变了客户端可见的两处形态：工具结束事件的失败文本（去掉引擎前缀）、以及失败全文（从字符串变成信封对象）。据此同步文档、快照与前端。

**快照与 schema**

| 项 | 结果 |
|---|---|
| `crates/app/wf-server/openapi.json` | 刷新命令跑通，产物与刷新前逐字节相同（691 操作、691 唯一 `operationId`、390 schema、OpenAPI 3.1.0） |
| `apps/web-app/src/lib/api/schema.d.ts` | `tools/openapi-codegen` 的 `npm run gen` 重新生成，与刷新前逐字节相同 |
| 不变原因 | 失败信封走工具消息正文与流事件，不经 REST 响应体；`ToolExecutionResult` 未派生 `ToSchema`，也不在文档 `components` 中 |

**文档修正**

- `docs/app/api/09-openapi-doc-generation.md`：刷新命令去掉 `--no-default-features`。该特性集会裁掉 `openapi` 模块本身（模块要求 `file-checkpoint`/`git-fs`/`pty`/`rag-vector`/`backends`/`metrics` 同时开启），测试收集到 0 项且以 `0 passed; N filtered out` 报成功，是静默假通过。同时更正规模数字（511→691 操作、221→390 schema）、补 luajit 前置条件、删去已不存在的"手工复制 schema.d.ts"步骤。
- `docs/engine/tools/tool-result-display-detail.md`：工具结束事件由六要素改七要素（增加失败类别），说明历史记录从信封恢复单行消息。

**流事件**

`AgentStreamEvent::ToolEnd` 与 `ExecutionStreamEvent::ToolEnd` 各增 `error_code`（`#[serde(default)]`，成功与老服务端为 `None`），三条产出路径（审批/可见性拒绝、钩子否决、正常执行）分别从拒绝消息元数据与失败信封取同一个码。

**前端**

| 位置 | 改动 |
|---|---|
| `utils/stream-parser.ts` | `ToolLifecycle.errorCode`，从 `error_code` 帧字段解析 |
| `stores/stream-run.svelte.ts` | `toolEnd` 由八位置参数改为接收 `ToolLifecycle`；`LiveToolCall.errorCode` |
| `chat/chat-session.svelte.ts` | 调用点收敛为 `chatStream.toolEnd(tool)`；`toLiveEntry` 透传类别 |
| `types/models.ts` | `ToolCallEntry.errorCode` |
| `utils/tool-output.ts` | 新增 `parseFailureEnvelope`（码、消息、建议动作、明细、失败下标）；失败但无摘要时从信封恢复单行消息，不把原始 JSON 当摘要 |
| `components/domain/ToolCallCard.svelte` | 输出标题旁显示失败类别胶囊（零文案成本，直接显示码），建议动作单独成块 |

验证：`svelte-check` 0 error 0 warning；`vitest` 53 文件 510 项通过（新增 5 项信封解析断言）；`prettier --check` 与 `eslint` 干净。后端 `cargo check --workspace --all-targets` 通过，`cargo fmt --check` 通过，`clippy --all-targets` 无新告警。测试：`wf-tools` 401、`wf-agent` 208 单测 + 全部集成（`hook` 2、`timeout` 4、`tool_boundaries` 6、`stream_lifecycle` 3）、`wf-types` 143、`wf-api` 556、`wf-server` 249、`wf-workflow` 393 + e2e 二进制（此前的 2 项既有失败已修，见第十五、十六节）、`tui-core` 294、`wf-cli-shared` 169、`wf-mini` 146、`wf-tui` 198 + 19 集成。

## 十五、既有失败用例修复：preprocess 的 FORK 路径冲突用例

现象（改动前后一致，与本轮信封改造无关）：

```
panicked at crates/engine/wf-workflow/src/preprocess.rs:1209:9:
path collision is a warning, not a rejection: [
  ValidationError { field: "nodes.embed.embed.nodes.f2",
                    message: "FORK node (f2) has no matching JOIN node" },
  ValidationError { field: "nodes.embed.embed.nodes.f2.config.fork_paths[0]",
                    message: "FORK node 'f2' branch targets unknown node 'body'" }]
```

**归因一：夹具构造了非法子图。** 原夹具的 embed 子图只有 `s2(START) / f2(FORK) / e2(END)`，而 `f1` 与 `f2` 共用同一个闭包，`fork_paths[0].child_node_id` 固定写成 `"body"`——这个节点在任何一层图里都不存在。于是两条规则同时触发：

| 规则 | 位置 | 报错 |
|---|---|---|
| 分支目标必须存在 | `validation/fork_join.rs:211-239`（`validate_fork_children`） | `branch targets unknown node 'body'` |
| 每个 FORK 必须有配对 JOIN | `validation/fork_join.rs:17-181`（`validate_fork_join_pairs`，报错在 `:178`） | `no matching JOIN node` |

也就是说，用例原本想验证"（合法输入下）路径冲突只警告不拒绝"，却用了一个必然被拒绝的子图，断言方向与夹具自相矛盾。

**归因二：`embed.embed` 连续出现不是拼接缺陷。** `collect_embed_errors`（`preprocess.rs:248-255`）的字段模板是 `nodes.{embed_node_id}.embed.{child_field}`，第一段是 EMBED_GRAPH 节点 id，第二段是"进入子图定义"的标记段。夹具恰好把节点命名为 `embed`，所以输出 `nodes.embed.embed.nodes.f2`。用临时打印核对过链条：子图内 `e.field == "nodes.f2"`，前缀后变成 `nodes.embed.embed.nodes.f2`，与模板一致。路径语义正确，只是命名巧合造成可读性误导——原遗留清单把它当成"嵌套路径解析缺陷"是误判。

**归因三：被测逻辑本身正常。** 单个定义内的 `path_id` 唯一性只在"同一份定义"里检查（`validation/fork_join.rs:73-86`），父图与子图各持一份 `p1` 都不报错；扁平化时子图节点被加上 `{embed_id}:` 前缀（`preprocess.rs:443`），两份通知才在同一张执行图上相遇，正该由 `duplicate_fork_path_warnings`（`preprocess.rs:262-305`）以警告上报、不进 `validation_errors`。

**改动**（`preprocess.rs:1179-1251`，仅测试）：

1. 子图改为完整合法的独立工作流：`s2 → f2(FORK, p1 → b2) → b2 → j2(JOIN, [p1]) → e2`，FORK 有配对 JOIN、分支目标存在。
2. 父图补上 `j1(JOIN, [p1])`，使父图自身也合法（`f1 → embed → j1`），冲突只剩"同一个 `p1` 被两份定义各用一次"这一条。
3. 新增夹具自检断言：先 `GraphValidator::validate(sub.clone()).is_ok()`，夹具一旦再退化成非法子图，失败点会立刻指向夹具而不是被测逻辑。
4. 给 `fork` 闭包加 `child` 参数，避免两条定义被迫共用一个不存在的目标。

**验证**：`cargo test -p wf-workflow --lib preprocess::tests` 13 项全过（含该用例）；同类问题的第二处见第十六节。

## 十六、既有失败用例修复：AGENT_LOOP e2e 缺装 compaction limits

替换 `--all-targets` 跑法后暴露出的第二处既有失败（与信封闭环无关，`wf-workflow` 的 e2e 二进制此前没被单独跑过）：

```
panicked at crates/engine/wf-execution-shared/src/compaction/strategy.rs:116:10:
compaction limits not installed
...
assertion `left == right` failed
  left: Object {"via": String("default")}
 right: Object {"via": String("timeout")}
```

**归因**：`process_compaction_limits`（`compaction/strategy.rs:108-117`）按设计"缺装即响"，进程级参数只由运行时启动注入（`set_process_compaction_limits`，`strategy.rs:98-106`）。`error_branch_e2e.rs` 的 `agent_node_timeout_routes_as_transport_timeout` 直接构造 `AgentLoopHandler`，绕过了启动注入，于是 panic 打断节点执行、错误类别没被标成 `TransportTimeout`，最终走了 workflow default（`via: default`）而不是显式 `TransportTimeout` 路由。与第十三节遗留 3 的 `wf-agent --test hook/timeout` 是同一族问题、同一修法。

**改动**（`crates/engine/wf-workflow/tests/error_branch_e2e.rs`，仅测试）：

1. 新增 `install_compaction_limits()`：`Once` 守卫 + `builtin_compaction_limits()`，与 `wf-agent/tests/timeout.rs:24-35`、`wf-api/agent/agent_execution/tests.rs:18` 保持一致。
2. 在该用例入口处调用，注释说明"缺装即响"是设计而非缺陷。

**验证**：`cargo test -p wf-workflow --test error_branch_e2e` 12 项全过；`agent_loop_e2e`（3 项）与 `agent_compression_e2e`（2 项）复核未触发同类路径，无需改动。

> 说明：本轮只做定点验证（单 crate / 单测试二进制），未跑全 workspace 回归与 clippy；大规模编译在开发侧编译服务器上完成。
