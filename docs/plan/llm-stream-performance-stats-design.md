# LLM 流式性能统计（首 Token / Tokens Per Second）设计

## 背景与现状

本方案回答两个问题：TPS（tokens per second）与首 Token 统计应该在哪一层实现、如何实现。分析参照 `wf-pricing` 与 llm-suite 中价格计算的现有分层。

### 现有链路盘点

1. **价格计算的四层分工（参照模式）**
   - llm-suite：持有契约与计算——`llm-suite-token` 定义 `PricingResolver` trait 与 `TokenPricing`/`ResolvedPricing` 类型，`llm-suite-gateway` 在收到 usage 时注入 resolver 计算成本后经 `TokenUsageSink` 上报。
   - wf-pricing：宿主侧数据源（内置 LiteLLM 公共表 + wf-storage 自定义规则 + 组合 resolver）。
   - wf-runtime：启动期接线（`build_composite_resolver` → `with_pricing_resolver`）。
   - wf-metrics：聚合与导出（`TokenMetricsCollector` 计数器/直方图）。

2. **首 Token / TPS 的现状**
   - llm-suite `TokenRecordingStream` 已经完整采集每次流式请求的 `StreamStats`（`time_to_first_chunk` 真实首分片延迟、`stream_duration` 首末分片间隔、`chunk_count`、`total_duration`），并附加在 `FinalMessage` 事件上。
   - 但宿主侧消费方（agent 循环、workflow LLM 节点）全部丢弃 `stream_stats`；gateway 层的 `record_first_byte` 只覆盖建流耗时（TTFB 代理），并非真实首 Token。
   - agent 审计 `LlmCallRecord` 已有 `started_at`/`duration_ms`/token 数，可粗算 TPS 但无法剥离排队与首 Token 等待。
   - 全仓库无任何 TPS 指标。

### 核心结论

**数据源已经存在，缺的是贯通。** 分层方式与价格计算一致：测量与契约留在 llm-suite，聚合留在宿主 infra 层（wf-metrics），消费落在 app 层（审计/API）。不放 wf-agent——因为 LLM 调用有四个来源（agent 循环、workflow LLM 节点、single-shot、脚本路径），只有 gateway 能统一覆盖且独占计时窗口；agent 层测得的 wall-clock 会混入队列与迭代开销。

## 各阶段改动

### 阶段一：llm-suite 子模块（独立仓库，单独 patch）

1. `llm-suite-client` 的 `LlmMetricsSink` trait 增加观察点 `record_stream_stats(stats, completion_tokens, model)`，提供默认空实现，既有实现方不受影响。
2. `TokenRecordingStream` 增加可选的 `SharedLlmMetricsSink`（builder 式 `with_metrics_sink`），在发出 `FinalMessage`（stats 已附加）时上报一次，带上累计的 completion tokens；每个流最多上报一次。
3. `llm-suite-gateway` 的 `generate_stream` 在包装 `TokenRecordingStream` 时把已有的 `token_metrics` sink 传入。非流式路径不产生该统计（无首 Token 概念），由既有 `record_request` 覆盖。

### 阶段二：wf-llm 适配器转发

`token_stream_adapter::MetricsSinkAdapter` 重写新的 trait 方法，将 `time_to_first_chunk`、`stream_duration` 与 completion tokens 转发到 `TokenMetricsCollector`。与既有 `record_first_byte` 的转发模式完全一致。

### 阶段三：wf-metrics 聚合

1. 新指标常量：
   - `token.stream.first_token.duration`（真实首 Token 延迟直方图；与既有 `token.stream.first_byte.duration` 建流代理并存，口径不同）
   - `token.stream.generation.duration`（生成窗口直方图）
   - `token.stream.generation.tokens`（生成窗口内输出 tokens 计数器）
2. `TokenMetricsCollector` 新增 `record_stream_stats(first_token_ms, generation_ms, completion_tokens, model)`。
3. `TokenUsageStats` 统计输出新增 `avg_first_token_duration_ms`、`avg_generation_duration_ms`、`avg_output_tokens_per_second`（= 生成 tokens 总量 / 生成时长总量，比值口径，避免直方图均值的比值失真）。
4. 补充 `descriptions.rs` 与常量唯一性测试。

### 阶段四：审计贯通（wf-types / wf-agent）

1. `LlmCallRecord` 新增可选字段 `first_token_ms`、`generation_ms`（plain Option，旧 blob 反序列化为 None，与文件内既有兼容策略一致）。
2. `llm_call_record` 构造函数增加 `stream_stats` 参数；agent 流式路径从 `FinalMessage` 捕获 `stream_stats` 填入，错误/中止路径传 None；阻塞路径透传 `LlmResult.stream_stats`（非流式通常为 None）。
3. 每个 agent 请求在审计中即可看到真实首 Token 与生成窗口，TPS 可派生。

### 阶段五：展示面（wf-workflow / wf-api）

1. workflow LLM 节点把 `FinalMessage.stream_stats` 透传到返回的 `LlmResult.stream_stats`（该占位字段首次被真实填充）。
2. wf-api 审计视图 `LlmCallAuditView` 新增 `first_token_ms`、`generation_ms`、派生 `tokens_per_second`（= completion_tokens / generation_ms，仅两者均有效时输出）。

## 语义口径

- `first_byte`（既有）：建流耗时，gateway 在 `generate_stream` 返回时测量，是 TTFT 的上界代理。
- `first_token`（新增）：首个流事件到达的真实延迟，来自 `StreamStats.time_to_first_chunk`。
- TPS：completion tokens / 生成窗口（首末分片间隔）。reasoning tokens 已含在 completion 口径内，不再单列。非流式请求不产生首 Token 与 TPS 统计。
- 双轨原则：以上均为记账数据（cost track），只进 metrics 与审计，不进入压缩/限额等决策逻辑。

## 明确不做

- 不改 `AgentStreamEvent::Usage` / `ExecutionStreamEvent::Usage`：该事件面在 TUI/CLI 有十余处穷举匹配，为一个可选展示字段引入大范围涟漪不划算。状态栏如需实时 TPS，后续可基于 wf-metrics 或审计数据单独设计。
- 不改 llm-suite 的 `TokenUsageHistory`：每请求计时由宿主侧 `LlmCallRecord` 承载，避免扩散持久化格式。
- 不需要 wf-pricing 式的新宿主 crate：TPS/首 Token 是纯测量统计，无外部数据源，wf-metrics 即可承载。

## 兼容性

- trait 新方法带默认实现，所有既有 `LlmMetricsSink` 实现方无需改动即可编译。
- `LlmCallRecord` 与审计视图新增字段均为可选 + `skip_serializing_if`，旧 checkpoint / 旧审计 JSON 反序列化不受影响。
- 指标均为新增序列，Prometheus/JSON 导出向后兼容。

## 验证

- `cargo clippy --all-targets --all-features` 全量编译检查（需先 `git submodule update --init crates/infra/llm-suite`）。
- llm-suite 子模块：`cargo test -p llm-suite-client token_stream`（新增 stats 上报测试）。
- 宿主侧：wf-metrics token collector 单测、wf-agent stream/审计相关单测、wf-api audit 测试。

## 交付物

- 宿主仓库 patch（设计文档 + 阶段二至五代码）。
- llm-suite 子模块 patch（阶段一），需在子模块仓库内单独应用与提交。
