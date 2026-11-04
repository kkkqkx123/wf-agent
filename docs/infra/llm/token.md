# Token 估算、计数、定价与用量

本篇覆盖 `wf-llm` 暴露的 token 能力：文本/消息估算、provider 形状计数、定价解析、
用量归一化与记账。token 模块是纯计算与数据模型，定价解析器的组合实现位于
`wf-pricing`（见 `docs/infra/pricing/README.md`）。

## count（估算）

- `estimate_tokens(text, estimator)`：文本 token 估算。
- `estimate_message_tokens(message)` / `estimate_messages(messages)`：消息级估算。
- `estimate_request_tokens(request)`：请求级估算（含工具声明）。
- `estimate_tool_declarations(tools)`：工具声明 token 估算。
- `estimate_image_tokens`：图像 token 估算。
- `TokenEstimator`：估算器 trait/实现。

## estimation

- `estimate_tokens`：便捷入口。
- `TokenEstimator`：可插拔估算器。

## pricing（成本计算）

- `TokenPricing` / `TokenPricingTier`：定价模型与阶梯。
- `TokenCost`：单次调用成本。
- `CacheTokenScope`：缓存 token 归属范围。
- `compute_cost` / `compute_baseline_cost` / `compute_cache_savings`：成本计算函数。

## pricing_resolver

- `PricingResolver` / `ResolvedPricing`：定价解析器 trait 与解析结果。
- 具体实现（公共表、存储表、组合、provider 映射）由 `wf-pricing` 提供。

## usage（用量与记账）

- `NormalizedUsage`：归一化用量（input/output/total/reasoning/cache_read/cache_write）。
- `LlmCallContext`：调用上下文（model/profile/provider/format/execution_id）。
- `LlmUsageReport`：单次调用用量报告。
- `normalize_usage`：provider 原始用量 → 归一化用量。
- `token_accounting` / `TokenAccounting`：用量记账累加器。
- `ReasoningTokenScope`：推理 token 归属范围。

## 语义约定

- **双计数视图**：同时保留 provider 原始计数（prompt/completion/reported_total）与
  归一化计数（input/output/total），reported_total 仅用于对账。
- **缓存 token 归属**：Anthropic 风格（缓存创建成本存在）时 prompt_tokens 不含缓存
  token（`SeparateFromPrompt`）；否则 `IncludedInPrompt`。
- **定价唯一路径**：计价走 pricing 路径；模型元数据中的费用字段恒为 `None`，避免重复。
- **热路径**：存储定价解析器在构造时加载内存镜像，解析为同步 HashMap 查找，不阻塞存储后端。
