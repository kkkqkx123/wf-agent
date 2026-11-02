# wf-pricing

LiteLLM 风格的 LLM token 计价解析器。提供"自定义（存储）优先、公共（内嵌）兜底"的组合解析能力，同时从同一份内嵌 LiteLLM 表派生模型元数据（上下文窗口、图像输入、推理支持等）。解析器 trait 定义在 `llm-suite-token`（`llm_token::PricingResolver`），本 crate 提供具体实现与运行时工厂。

## 定位与依赖

- 属 infra 层，依赖：`llm-token`（llm-suite 子模块）、`llm-types`、`wf-storage`。
- 上游（指标统计、网关等）通过 `build_composite_resolver` / `build_mapped_resolver` 获得组合后的解析器，无需感知内部表结构。

## 模块结构

| 模块 | 内容 |
|---|---|
| `resolver.rs` | 共享查找键 `ModelKey` 与组合解析器 `CompositePricingResolver`；re-export `PricingResolver` / `ResolvedPricing` trait 与类型 |
| `static_table.rs` | `StaticPricingResolver`：内嵌公共 LiteLLM 表解析器 |
| `storage_table.rs` | `StoragePricingResolver`：`wf-storage` 中 `pricing_rule` 存储的内存镜像解析器 |
| `mapped.rs` | `ProviderMappedPricingResolver`：本地 provider id 到 LiteLLM 命名空间的翻译包装 |
| `metadata.rs` | `ModelMetadataResolver`：从同一内嵌表派生模型元数据 |
| `build.rs` + `data/litellm_public.json` | 构建脚本将 `src/data/litellm_public.json` 拷入 `OUT_DIR` 供 `include_str!` 内嵌 |

## 查找键 ModelKey

`(provider: Option<String>, model: String)`。解析时先尝试 `(provider, model)` 精确匹配，未命中则回退到仅按 model 查找，使规则可在未指定 provider 时跨 provider 生效。

## 解析器实现

### StaticPricingResolver（公共表）

- 数据来源：编译期内嵌的 `litellm_public.json`（LiteLLM 官方公共计价表快照）。
- 表按模型名建键（provider 为 `None`）；`sample_spec`（模式说明条目）在解析时剔除，不作为可计价模型。
- 成本字段遵循 LiteLLM 的 `input_cost_per_token`（每 token）约定，读取时统一换算为每百万 token（`*_per_mtok`）。
- 支持阶梯定价 `tiered_pricing`（每档含 `up_to_tokens` 上界；最后一档上界为 `None`）。基础价格为 0 时回退取第一档价格。
- 缓存成本：`cache_read_input_token_cost` → 缓存读；优先取显式 `cache_creation_input_token_cost_5m`，否则取 `cache_creation_input_token_cost` 作为 5 分钟缓存写（1h 写沿用 5m 值）。
- 缓存 token 归属：存在缓存创建成本即视为 Anthropic 风格（`SeparateFromPrompt`，prompt_tokens 不含缓存 token），否则 `IncludedInPrompt`。
- `empty()` 构造空表，用作无存储时的 custom 槽位。

### StoragePricingResolver（自定义规则）

- 数据来源：`StorageContext` 中的 `pricing_rule` 存储（`PricingScope` 映射为 `CacheTokenScope`）。
- 构造时将全部规则加载进内存镜像（`Mutex<HashMap<ModelKey, ResolvedPricing>>`）；后续解析为同步 HashMap 查找，指标热路径上绝不阻塞存储后端。
- 自定义规则变更后需调用 `refresh(&StorageContext)` 重新加载镜像。

### CompositePricingResolver（组合）

custom 解析器先查、未命中回退 public。运行时组合规则：存储可用时 custom 为 `StoragePricingResolver`，否则为 `StaticPricingResolver::empty()`；public 恒为内嵌公共表。

### ProviderMappedPricingResolver（命名空间翻译）

本地 provider id（如 `builtin-azure`）不是合法 LiteLLM 命名空间，而公共表中 provider 特化条目以 `azure/gpt-4o` 等形式存储。此包装持有独立（未预先组合）的 custom 与 public 解析器及映射表 `HashMap<本地id, 命名空间>`，保证任何 custom 规则都先于任何 public 条目被查询：

1. 模型名不含 `/` 且 provider 在映射表中时，构造 `namespace/model` 限定键；
2. custom 依次尝试：限定键（精确）→ 本地 `(provider, model)` → 规范 model-only；
3. public 依次尝试：限定键 → 已含 `/` 的模型名原样 → 规范回退。

映射表中不存在的 provider 直接走规范回退。

## 模型元数据 ModelMetadataResolver

与计价同源（同一份内嵌 `litellm_public.json`），回答计价之外的互补问题：上下文窗口、最大输出、图像输入、推理支持、思考等级、模型类型与线格式。

- 两段式解析：先按 `serde_json::Value` 读入并剔除 `sample_spec`，再逐条强类型解析；未知字段忽略，真实条目违反 schema 则构建期显式失败。
- 仅保留 `chat` / `responses` / `image_generation` 模式（`responses` 视为 Chat）；embedding 等其他模式不占用常驻内存。
- `metadata_for_provider(litellm_provider, model)`：已含 `/` 的模型名先按全名查；否则用命名空间拼 `namespace/model` 限定键查；最后回退规范短名。返回的 `id` 回显请求的模型名而非表键。
- 费用字段恒为 `None`——计价唯一走 pricing 路径，不在此重复。
- 线格式推断：anthropic → `LlmFormat::Anthropic`；gemini/vertex → `LlmFormat::GeminiNative`；其余 → `LlmFormat::OpenaiChat`。

## 运行时工厂（lib.rs）

- `build_composite_resolver(storage: Option<&StorageContext>)`：custom（存储或空）+ public 内嵌表，适合只按模型名计价。
- `build_mapped_resolver(storage, mapping)`：同样的表，但 custom 与 public 保持独立置于 `ProviderMappedPricingResolver`，通过映射表启用 provider 特化计价。

两者均为异步（需从存储加载 custom 规则），返回 `Arc<dyn llm_token::PricingResolver>`。
