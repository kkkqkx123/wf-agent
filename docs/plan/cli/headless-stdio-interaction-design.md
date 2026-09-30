# 无头模式 stdio 交互扩展设计

## 一、现状：为什么 headless 目前"不能问"

无头路径上有两类交互，处理方式都是"绕开"而非"应答"：

| 交互点 | 现状 | 代码位置 |
|---|---|---|
| 工具审批 | 由 `PolicyApprovalHandler`（`ApprovalPolicy` 前缀白名单）同步决策，人审路由被前置拦截，从不落地到 UI | `wf-cli-shared/src/run.rs` |
| follow-up 提问 | `HeadlessInteractionGuard` 只置一个 `AtomicBool` 标记，run 结束后以 exit code 1 失败 | 同上 |
| 交互注册 | `register_handler` 只提供**事件通知**（`on_tool_approval_requested` / `on_followup_question_requested`），handler 返回 `()`，没有应答回传通道 | `wf-api/src/entity/user_interaction.rs` |

结论：**通知通道已存在，应答通道不存在**。扩展的本质是补一条"stdin 读入 → 应答交付"的通路。

## 二、扩展方案

### 1. 交互协议（stdout 侧，人/程序可读）

向 stderr（诊断通道，不污染 stdout 的 `| jq` 纪律）输出一行结构化提问，请求读取 stdin：

```text
? APPROVE tool_call_id=b3f2 tool=bash reason="policy ask" (y/n)
```

带 `--json` 时输出 JSON 行（NDJSON），便于脚本应答。**stdout 保持纯业务输出不变**——这是现有 `OutputSink` / `DiagWriter` 分流纪律的直接延续。

### 2. 应答通道（stdin 侧）

新增 `wf-cli-shared/src/stdio_prompt.rs`：

```rust
pub struct StdioPromptSource { /* tokio mpsc<String> + stdin reader task */ }

impl StdioPromptSource {
    /// Spawn a blocking stdin line reader; each line lands on a channel.
    pub fn spawn() -> (Self, JoinHandle<()>);
    /// Await the next user answer line, with optional timeout.
    pub async fn next_answer(&self, timeout: Duration) -> Option<String>;
}
```

要点：

- stdin 用 `tokio::task::spawn_blocking` + `BufReader::lines()` 泵到 `mpsc`，与事件流泵（`StreamExt::next`）并发等待——用 `tokio::select!` 同时等"下一个交互请求"和"执行流事件"，避免死锁（提问未到时 stdin 有缓存行也不阻塞）。
- TTY stdin 下 `y` 回车即可应答；非 TTY（管道）下由上游脚本按协议回写。

### 3. 审批接入点：新增 `StdioApprovalHandler`

实现已有的 `ToolApprovalHandler` trait（`wf-execution-shared/src/approval.rs`），插入点与 `PolicyApprovalHandler` 完全一致（run.rs 中构造 `RunAgentLoopParams` 处）：

```rust
#[async_trait::async_trait]
impl ToolApprovalHandler for StdioApprovalHandler {
    async fn request_approval(&self, request: &ToolApprovalRequest) -> ToolApprovalResult {
        // 1. diag line: "? APPROVE ..." (stderr)
        // 2. stdio_prompt.next_answer(timeout).await
        // 3. parse "y"/"yes"/"n"/"no" (+ --approval-timeout 到期默认 deny)
        // 4. return Approved / Denied
    }
}
```

组合方式：`--interactive-approval` 开启时，策略路由到人审的 `Ask` 决策交给 `StdioApprovalHandler`；其余仍走前缀白名单。也可做成 `PolicyApprovalHandler` 的 wrapper（fallback handler），保持策略层不变。

### 4. follow-up 提问接入点：改造 `HeadlessInteractionGuard`

`on_followup_question_requested` 收到的 `request: &Value` 中含 `interactionId`（TUI handler 在 `wf-tui/src/interactive/handlers.rs` 已这样解析）。改造为：

```rust
fn on_followup_question_requested(&self, execution_id: &str, request: &Value) {
    // spawn task: 输出 "? ANSWER interaction_id=.. prompt=..",
    // 等待 stdin 应答, 通过既有交互应答 API 提交
    // (wf-api 已有 interaction 记录的 respond/update 路径, 供 TUI/web 使用)
}
```

应答提交复用 wf-api 现有的 interaction 应答 API（`InteractionApprovalHandler` 打开的是**持久化 `user_interaction` 记录并等待响应**）——headless 端只需像 TUI / web 端一样调用"应答该 interaction"的入口，等待方自动被唤醒。这一步不需要改 `UserInteractionHandler` trait 本身。

### 5. CLI 参数与行为矩阵

新增参数（`wf-cli-shared/src/args.rs`）：

```text
--interactive          # 允许从 stdin 读取应答（默认关闭，保持现行为）
--approval-timeout <s> # 无应答超时，默认 120s，超时按 deny 处理
--assume-yes / -y      # 无人值守下自动同意（等价当前前置策略的替代写法）
```

| 场景 | 行为 |
|---|---|
| 默认（无 flag） | 完全维持现状：策略审批 + follow-up 失败 exit 1 |
| `--interactive` + TTY stdin | 审批 / follow-up 打到 stderr，读 stdin 行应答 |
| `--interactive` + 管道 stdin | 提示仍输出，从管道读应答行（脚本可回写） |
| stdin 已被用作 prompt（`wf run` 无位置参数） | 冲突检测：拒绝 `--interactive` 或要求显式分隔（prompt 改为必须传位置参数） |
| 超时 / EOF | 按 deny 记录到 diag，run 继续或按现有语义终止 |

### 6. 实施顺序（3 步，均可独立验证）

1. **StdioApprovalHandler + stdin 泵**：只覆盖工具审批（y/n），改动集中在 `wf-cli-shared`，用 `--interactive` 灰度。
2. **follow-up 应答**：接入 wf-api interaction 应答 API，补交互协议 NDJSON 格式。
3. **协议泛化**：若后续出现新的交互类型（选项选择、自由文本输入），把 `? TYPE key=value` 行协议抽象成统一的 `InteractionPrompt` 渲染器 + 应答解析器，新类型零成本接入。

### 7. 设计约束（与项目规范对齐）

- stdout / stderr 纪律不破坏：提问走 stderr，结构化应答协议只影响 stderr 与 stdin。
- 不引入额外动态抽象：直接实现既有 `ToolApprovalHandler` / 复用 `UserInteractionHandler`，不加新 trait。
- 无 backward-compat 负担：`HeadlessInteractionGuard` 的"置位失败"语义可直接替换。
- 所有新代码使用英文，模块按 `wf-cli-shared/src/stdio_prompt.rs` 平铺命名。

### 8. 核心结论

通知管线（`UserInteractionHandler`）和应答等待点（`ToolApprovalHandler` / interaction 记录）都已存在，缺的只是 `wf-cli-shared` 里一个 stdin 行泵 + 两个把"提问打出去、应答读进来"的 handler 实现。
