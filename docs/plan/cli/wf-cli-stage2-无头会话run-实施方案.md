# wf-cli Stage 2 实施方案：无头会话 run（第一个端到端闭环）

> 状态：已完成（阶段 2A–2F，2026-08-18）
> 上游方案：`docs/plan/cli/wf-cli-分阶段实现方案.md`（Stage 2 任务定义）、`docs/cli/05-opencode-mini模式与无头模式设计.md` §5（无头模式设计）
> 范围：`wf run "<prompt>"` 端到端可用——流式输出、审批降级、退出码、stdin 管道。**不引入任何 UI 依赖**（无 ratatui/crossterm）。

## 一、现状与缺口

Stage 1 已交付：`error.rs`（退出码映射）、`output.rs`（Format × Sink 两层）、`args.rs`/`mode.rs`（命令树与形态判定）、`events.rs`（UnifiedEvent 雏形）、`domain.rs`（DomainAdapter：Runtime bootstrap/shutdown）、`lib.rs`（打点式最小闭环）。

Stage 2 缺口：

| # | 缺口 | 说明 |
| :- | :--- | :--- |
| G1 | `run.rs` 不存在 | `run_headless` 仅打点（envelope + promptChars），无真实 agent 会话 |
| G2 | 会话事件流未消费 | `wf_api::agent::agent_execution::stream` 未接线；`ExecutionStreamEvent → UnifiedEvent` 归一缺失 |
| G3 | 审批策略缺失 | 无头审批降级（deny/白名单/预授权）没有承载点；wf-api 的 `stream()` 无法注入 `ToolApprovalHandler` |
| G4 | 执行标识不可得 | `stream()` 路径拿不到 `agent_loop_id`（摘要行 `▣ exec_id · iterations · duration` 需要） |
| G5 | stdin 管道断链 | `wf run`（无位置参数）在 stdin 非 TTY 时不读管道（mode.rs 仅在 `--no-tui`/非 TTY stdout 分支读取） |
| G6 | SIGINT 无处理 | 中断应映射 exit 4 |
| G7 | 无头追问策略缺失 | `UserInteractionHandler` 未注册，追问到来时无拒绝语义 |

## 二、外部最佳实践参考（context7 检索结论）

1. **tokio（/websites/rs_tokio）**：`tokio::signal::ctrl_c()` 与 `tokio::select!` 组合是流循环中断的标准做法；持有 `JoinHandle` 的流类型在 drop 时 abort 驱动任务（`AgentEventStream`/`ExecutionEventStream` 均如此设计，drop 消费端即可级联中止 agent 驱动任务，无需显式 cancel）。
2. **clap（/websites/rs_clap）**：`Vec<String>` 字段天然支持 `--flag a --flag b` 重复累积（隐式 `ArgAction::Append`），适配 `--approve-prefix` 多值预授权。
3. **仓库内既有范式**（源码研读结论）：
   - 消费方样板：`wf-server/src/api/agent/loops.rs` `handle_stream_loop`（`stream()` → `Stream::next()` 循环）；
   - mock LLM：`LlmGateway::register_mock(profile, MockLlmClient)`，`LlmResponseSpec::text/tool_calls/script_stream`，profile 未配置时引擎返回 Failed（映射 exit 1）；
   - 审批注入点：`AgentLoopCoordinator::with_approval(options, handler)`；handler 存在而无 options 时全部工具进入 pending 由 handler 裁决（`wf-agent/src/coordinator/tool.rs` fast-path 规则）；
   - 交互钩子：`wf_api::entity::user_interaction::register_handler`（进程级 `RwLock<Option<...>>` 槽位）。

## 三、关键设计决策

| # | 决策 | 理由 |
| :- | :--- | :--- |
| D1 | **wf-api 最小扩展**：`RunAgentLoopParams` 新增 `agent_loop_id: Option<Id>` 与 `approval_handler: Option<Arc<dyn ToolApprovalHandler>>`，`run()`/`stream()` 装配进 coordinator | CLI 不绕过 API 层自行组装 coordinator（避免复制装配逻辑）；两字段分别解决 G3/G4；`Arc<dyn Trait>` 可 Clone，Debug 手写（handler 只打印是否注入） |
| D2 | **审批裁决全部收口 CLI handler**：不传 `ToolApprovalOptions`，挂 `HeadlessApprovalHandler`（sensitive → deny；低危白名单 → allow；`--approve-prefix` 命中工具名/命令前缀 → allow；其余 → deny） | 裁决纯函数化（`ApprovalPolicy::decide`），可独立单测；引擎侧"handler 在场即全 pending"语义恰好把策略完整交给 CLI |
| D3 | **stdout/stderr 纪律**：主输出（LLM 文本/消息记录/摘要信封）走 OutputSink（stdout）；工具行 `▲/✓/✗`、审批拒绝原因、中断提示走 stderr 诊断通道（`DiagWriter`，可注入缓冲以便测试） | 对齐 05 §5.2"工具摘要行不污染主输出"；`wf run \| jq` 管道语义成立 |
| D4 | **LlmDelta 行缓冲合并**：`DeltaBuffer` 按换行或 8KB 阈值 flush；迭代边界/终态强制 flush | 对齐 05 §5.2"按换行/固定缓冲合并后输出，避免逐 token 系统调用"；纯逻辑可单测 |
| D5 | **SIGINT → drop 流 → exit 4**：`tokio::select!` 监听 `ctrl_c`；触发即中断事件循环（流 drop 级联 abort 驱动任务），返回 `CliError::Interrupted` | tokio 官方模式 + 既有 drop-abort 语义；无需 loop id 即可中断 |
| D6 | **追问拒绝**：注册 `HeadlessInteractionGuard`（`UserInteractionHandler`）：`on_followup_question_requested` 置 `AtomicBool` + stderr 提示；会话终态检查该标志 → `CliError::Business`（exit 1） | 对齐 05 §5.3"无 TTY 无法交互 → 拒绝并报错退出"；handler 槽位是通知式回调，用标志位在驱动层变现 |
| D7 | **摘要行**：`▣ {exec_id} · {iterations} iterations · {secs}s`（text 模式走 stdout sink；json 模式由终态信封承载，silent 无输出）；空会话提示 `▣ {exec_id} · no output` | 对齐 05 §5.2/§3.3 turn summary 语义 |
| D8 | **无 prompt 报参数错**：`wf run` 既无位置参数又无 stdin 管道内容 → `CliError::Arguments`（exit 2） | 空消息会话无意义，显式报错优于静默空跑 |

## 四、模块落点

```
crates/wf-cli/src/
├── run.rs           ← 新增：RunOptions/RunOutcome/run_session 驱动器
│                      + SessionRenderer（事件→stdout/stderr 渲染）
│                      + DeltaBuffer（行缓冲合并）
│                      + ApprovalPolicy/HeadlessApprovalHandler（审批降级）
│                      + HeadlessInteractionGuard（追问拒绝）
├── events.rs        ← 扩展：UnifiedEvent::Completed 携带 result；
│                      ExecutionStreamEvent → UnifiedEvent 归一
├── args.rs          ← 扩展：run 子命令 --model <PROFILE>、--approve-prefix <PREFIX>（可重复）
├── mode.rs          ← 修复：run 子命令在 stdin 非 TTY 时读取管道全文为 prompt（G5）
├── domain.rs        ← 扩展：llm_gateway() 访问器（测试注入 mock）
└── lib.rs           ← 接线：run_headless → run::run_session

crates/wf-api/src/agent/agent_execution.rs
                     ← 扩展：RunAgentLoopParams{agent_loop_id, approval_handler}（D1）

crates/wf-cli/tests/run_e2e.rs ← 新增：mock LLM 端到端矩阵
```

## 五、分阶段任务与验收

### 阶段 2A：wf-api 领域层扩展（G3/G4 解除阻塞）

- [x] `RunAgentLoopParams` 增加 `agent_loop_id: Option<Id>`（预置执行标识）与 `approval_handler: Option<Arc<dyn ToolApprovalHandler>>`（审批注入），手写 `Debug`/`Clone`。
- [x] `run()`：`params.agent_loop_id.unwrap_or_else(generate)`；`coordinator_for(ctx, &params)` 挂 `with_approval_handler`。
- [x] `stream()`：同上装配；预置 id 经 `with_agent_loop_id` 生效（`build_entity` 已支持）。
- [x] 更新仓内构造点（wf-api builder/测试、wf-server `params_from_body`）。

**验收**：`cargo check -p wf-api -p wf-server` 通过；既有 wf-api 测试不回归。✅

### 阶段 2B：CLI 参数/模式/事件层

- [x] `args.rs`：`Run` 子命令新增 `--model <PROFILE>`（LLM profile，默认 `default`）、`--approve-prefix <PREFIX>`（`Vec<String>`，可重复）；单测覆盖解析。
- [x] `mode.rs`：`Command::Run` 分支在 prompt 缺失且 stdin 非 TTY 时读管道全文；单测覆盖 `echo "p" | wf run` 语义（注入 TTY 状态）。
- [x] `events.rs`：`UnifiedEvent::Completed { result: Value, iterations }` 携带终态结果；新增 `unified_from_execution_stream(ExecutionStreamEvent) -> Option<UnifiedEvent>`（`Engine` 事件过滤）；既有测试同步更新。

**验收**：`cargo test -p wf-cli`（单元）通过；`--model/--approve-prefix` 解析矩阵、stdin 管道判定、事件归一映射断言。✅

### 阶段 2C：run.rs 会话驱动器（G1/G2/G6/G7 主体）

- [x] `RunOptions { prompt, agent_id, model, approve_prefixes }`、`RunOutcome { execution_id, iterations, duration_ms, had_output }`（format/color 归入 `RunIo`，见"与方案的偏差"）。
- [x] `run_session(adapter, opts, io)`：
  1. 预置 `agent_loop_id`（`wf_common::generate_id`）→ 摘要行/诊断可用（G4）；
  2. 注册 `HeadlessInteractionGuard`（追问标志）（G7）；
  3. `RunAgentLoopParams` 挂 `HeadlessApprovalHandler`（G3）；
  4. `stream()` 启动 → `tokio::select!` 消费事件 / `ctrl_c`（G6）；
  5. 终态：Completed → flush + 摘要/信封；Failed → `CliError::Business`；Interrupted 事件 → `CliError::Interrupted`；SIGINT → drop 流 + `CliError::Interrupted`。
- [x] `SessionRenderer`：
  - text：user 消息行 → LlmDelta 经 `DeltaBuffer` 流式写 sink → 工具行 stderr（`▲ name` / `✓ name (ms)` / `✗ name`，耗时驱动器侧计时）→ `▣` 摘要行；
  - json/jsonl：user 记录 + 按迭代聚合的 assistant 消息记录（迭代边界提交）+ 终态信封（data 含 executionId/iterations/durationMs/model）；
  - silent：全部静默，退出码表达结果；
  - 空会话（无任何 assistant 文本/工具行）→ `▣ … no output` 提示（D7）。
- [x] `ApprovalPolicy::decide(tool_name, arguments) -> ApprovalDecision`（纯函数；**预授权前缀优先级最高**，见"与方案的偏差"P2）：
  1. `--approve-prefix` 命中工具名或命令参数（`command`/`cmd`）前缀 → allow（显式同意覆盖敏感判定）；
  2. sensitive 集（`approve_changes`/`write_file`/`edit_file`/`apply_patch`/`apply_diff`/`execute_command`）→ deny（打印原因）；
  3. 低危白名单（`read_file`/`list_files`/`grep_search`/`glob_search`/`update_todo_list`/`skill`）→ allow；
  4. 其余 → deny（提示 `--approve-prefix`）。
  拒绝原因经 stderr 诊断通道打印。
- [x] `DeltaBuffer`：push 累积；遇 `\n` 或 ≥8KB 输出就绪段；`take_remaining()` 终态冲刷。

**验收**：`cargo test -p wf-cli` 单元全绿（decide 矩阵、DeltaBuffer 边界、renderer 快照式断言）。✅

### 阶段 2D：接线与依赖

- [x] `domain.rs`：`llm_gateway()` 访问器。
- [x] `lib.rs`：`run_headless` 组装 `RunOptions`（prompt 缺失 → `CliError::Arguments`，D8）→ `run_session` → shutdown 保序 → 退出码经 `CliResult` 自然映射。
- [x] `Cargo.toml`：wf-cli 增 `futures`、`async-trait`、`wf-tools`、`wf-llm`；dev-deps 增 `wf-llm(features=["mock"])`（审批类型经 `wf-agent` 再导出，无需直接依赖 `wf-execution-shared`，见"与方案的偏差"P3）。

**验收**：`cargo check -p wf-cli` 通过；`wf run --help` 展示新选项。✅

### 阶段 2E：端到端测试矩阵（mock LLM）

e2e 落在 `run.rs` 内嵌 `tests::e2e` 模块（bootstrap DomainAdapter → `llm_gateway()` 注入 mock → `run_session`，sink/diag 用内存缓冲断言；落点偏差见"与方案的偏差"P1）：

- [x] text 流式：mock 文本 → stdout 流式文本 + `▣` 摘要行（含 execution id）；
- [x] json/jsonl 信封：jsonl 终态 `execution_summary` 记录字段完整（success/executionId/iterations）；json 信封字段经单元测试断言（success/type/entity/data/timestamp）；
- [x] 审批 deny：敏感工具（execute_command `rm -rf /`）被拒 + stderr 打印拒绝原因 + 会话仍收敛（拒绝消息回灌 LLM 后按默认文本收尾）；
- [x] 白名单放行：read_file（临时文件）放行并真实执行（`✓` 行）；
- [x] 失败映射：mock `script_error`（饱和脚本队列覆盖重试）→ `CliError::Business`（exit 1）；
- [x] 空输出：mock 空文本 → `▣ … no output` 提示；
- [x] 空 prompt：`CliError::Arguments`（exit 2）；
- [x] 退出码矩阵：error.rs 既有单测复验（0/1/2/3/4）。

**验收**：`cargo test -p wf-cli` 全绿（单元 + e2e，共 50 项）。✅

### 阶段 2F：收尾

- [x] 勾选本文档与总方案 Stage 2 任务项，补完成记录。
- [x] git 变更生成 patch（排除构建产物：target/、Cargo.lock 不动）。

## 五·一、与方案的偏差（实施期决策）

| # | 偏差 | 原因 |
| :- | :--- | :--- |
| P1 | e2e 测试落在 `run.rs` 的 `tests::e2e` 模块而非独立 `tests/run_e2e.rs` | 复用同文件测试基建（`SinkForwarder`/`diag_text`）；`run_session`/`RunIo` 均为 pub，后续如需独立集成测试可直接迁移 |
| P2 | 审批裁决优先级调整为：预授权前缀 > 敏感拒绝 > 低危白名单 | `--approve-prefix git` 的语义是显式同意（含 execute_command 中的 git 命令）；若敏感名单优先，该参数对命令类工具永远无效，与 CLI help 语义矛盾 |
| P3 | 未直接依赖 `wf-execution-shared` | `ToolApprovalHandler/Request/Result` 经 `wf_agent::approval` 再导出，wf-cli 依赖 wf-agent 即可；少一条依赖边 |
| P4 | `RunOptions` 不含 format/color，改由 `RunIo`（sink/diag/format）承载 | format/color 是 IO 装配概念而非会话参数；分离后 `run_session` 对渲染目标无感，测试可全内存注入 |

## 五·二、实施中修复的引擎侧缺陷（wf-agent，超出原定范围但为 Stage 2 验收必需）

| # | 缺陷 | 修复 |
| :- | :--- | :--- |
| E1 | `AgentIterationCoordinator::with_visibility_store` 重建 tool coordinator 时**丢失 approval 配置**（lifecycle.rs 装配链中它链接在 `with_approval` 之后，导致注入的审批 handler 从未生效、全部工具静默自动放行） | 重建时经 `approval_config()` 取出并回填（`wf-agent/src/coordinator/iteration.rs`、`tool.rs::approval_config`） |
| E2 | 流式工具执行路径 `execute_tool_calls_streaming` **完全绕过审批管道**（仅 blocking 路径的 `execute_sequential/parallel` 接入 `approve_tool_calls`），`wf run` 全部走流式 ⇒ 审批降级形同虚设 | 新增 `ToolExecutionCoordinator::approve_single_for_stream`，流式循环在执行前逐一过审；被拒调用以失败 ToolEnd 呈现且不执行（`wf-agent/src/coordinator/tool.rs`、`iteration.rs`） |

两处修复均由 e2e `sensitive_tool_call_is_denied_and_session_recovers` 驱动发现并验证；既有 wf-agent/wf-api/wf-workflow 测试无回归（workspace 全绿）。

另：修复 `wf-api/src/llm/llm.rs` 测试构造器缺失 `context_window_size` 字段的仓内既有编译错误（与 Stage 2 无关，阻塞 `cargo test -p wf-api`）。

## 六、风险与边界

| 风险 | 缓解 |
| :--- | :--- |
| stream 路径 conversation 不落库（run() 才持久化） | Stage 2 不依赖会话重放；json 输出以事件流自聚合为准（重放属 Stage 8） |
| 引擎 Interrupted 事件与 SIGINT 双路径 | 两者统一映射 `CliError::Interrupted`（exit 4），诊断行区分来源 |
| `--approve-prefix` 误授权 | 文档明示前缀语义（工具名或命令前缀）；默认无前缀 → 仅白名单放行 |
| mock 特性泄漏到生产依赖 | `wf-llm` 仅 dev-deps 启用 `mock` feature（与 wf-api 同法） |
| executor 对未知 profile 报错 | 映射为 Failed → exit 1，stderr 提示 profile 未配置 |
