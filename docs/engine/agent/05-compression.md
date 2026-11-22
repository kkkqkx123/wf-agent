# 会话压缩

> 范围：`crates/engine/wf-agent/src/conversation_compression.rs` 及 `src/coordinator/iteration/compression.rs`。

## 1. 总体机制

引擎自持对话会话，因此压缩/写回由自身消费者闭环：

- **后台消费者** `spawn_conversation_compression_consumer`（`src/conversation_compression.rs:51-196`）：订阅事件总线的 tokio 任务，只处理 `execution_id` 匹配本 loop 的事件，处理四类事件：
  - `ContextCompressionCompleted`：调用 `apply_compression` 应用压缩结果；应用成功后若仍超预算则记录 streak 并可能升级告警（`conversation_compression.rs:103-124`），随后经策略门控触发 `AfterCompression` 检查点（`conversation_compression.rs:127-142`）；
  - `ContextCompressionFailed` / `Discarded`：释放背压锚点（`end_compression_flight`）、清理 pending compaction（`conversation_compression.rs:144-173`）；
  - `ConversationWritebackCompleted`：嵌套 agent 结果的版本化写回（`conversation_compression.rs:174-191`）。

消费者由生命周期模板 `run_loop` 启动（`coordinator/lifecycle/run.rs:46-76`），循环结束后 abort。

## 2. 版本化写回

完成事件携带产生结果时的 `array_version`；`apply_compression`（`conversation_compression.rs:210-235`）与 `apply_versioned_writeback`（`conversation_compression.rs:243-265`）仅在会话当前版本仍等于锚点版本时才应用（`check_anchor`）——**新消息优先，过期结果直接丢弃**。丢弃也会发布版本化记录并释放背压锚点，避免后续迭代等一个注定被丢弃的结果（`conversation_compression.rs:84-101`）。

**压缩语义**：压缩不替换对话——完整历史保留，只追加摘要并收窄读投影（view），撤销压缩是零成本切回全历史（`conversation_compression.rs:202-206`）；分块（plan-mode）结果走 `apply_compaction_result`，旧格式走 `compress_with_tail`（`conversation_compression.rs:227-232`）。

## 3. 迭代内协作（`coordinator/iteration/compression.rs`）

- **`settle_compression_flight`**（`compression.rs:16`）：有压缩写回在飞时阻塞等待落回当前会话版本（受 settle 预算约束）；终态失败统一 park（`compression_failure_park`，`compression.rs:110`）——置 pause、记录 `CompressionFailure` 类错误记录，使暂停可被外部感知处理。
- **`publish_forced_compression`**（`compression.rs:141`）：安全网——provider 以 context-length-exceeded 拒绝请求时，强制发出 `CONTEXT_COMPRESSION_REQUESTED`（stateless plan，不读规划建议），打 `BeforeCompression` checkpoint，按 Taken/Vetoed/Untaken 处理（`compression.rs:224-242`）。
- **连续超预算升级**：`still_over_budget` streak 达阈值 → park 而非无限压缩（`coordinator/iteration.rs:486-520`）。

## 4. token 计账与压缩触发

LLM 调用后 `record_usage_and_compression`（`compression.rs:250`）：双轨 token 计账、任务级 token 警告/限额事件、按上下文预算压缩触发 `record_compaction_decision`（`compression.rs:328`）。
