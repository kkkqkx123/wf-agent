# 图像生成与决策能力

本篇覆盖 `wf-llm` 暴露的两类"非对话"能力：图像生成与决策（OpenAI `/decisions` 与
Jev `/v1/systemone`）。两者都遵循"provider trait + OpenAI 兼容 HTTP + mock"的形状。

## 图像生成

### 类型

- `ImageGenerationProvider`：provider trait（`generate_images(prompt, options)`）。
- `ImageModel`：模型描述。
- `ImagesContext`：端点上下文（base_url、model、api_key 等）。
- `ImagesOptions`：单次调用参数。
- `ImagesResult`：结果（含 `ImageData`）。
- `ImageData`：图像数据（base64/url 等）。

### 实现

- `OpenAiCompatibleImagesProvider`：OpenAI 兼容 images 端点传输（POST `{base}/images/generations`）。
- `MockImageGenerationProvider` / `MockImagesStep`（feature `mock`）：测试替身。
- `PluginImagesAdapter`：插件桥接（见 `plugin-adapters.md`）。

### 语义

- 端点差异仅 URL + 可选 bearer key，通过 `ImagesContext` 预设选择。
- prompt 为空时返回 `InvalidRequest`。

## 决策能力

### 协议

`DecisionProtocol`：

- `OpenAi`（`"openai"`）
- `Jev`（`"jev"` / `"typesafe"` / `"systemone"`）

`DecisionProviderConfig`：protocol、base_url、model、api_key、timeout_secs、headers、query_params；
`into_decision_config` 转为 `DecisionConfig`。

`AnyDecisionProvider`：`OpenAi(OpenAIDecisionProvider)` / `Jev(JevDecisionProvider)`；
`from_config` 按 protocol 分发。

### 类型

决策请求/响应相关：`DecisionRequest`, `DecisionResponse`, `DecisionQuestion`, `DecisionAnswer`,
`NamedQuestion`, `NamedAnswer`, `DecisionMessage`, `MessageContent`, `MessageRole`, `ContentPart`,
`ImageAsset`, `ResolvedImage`, `BoxCoordinates`, `ChoiceValue`, `ChoiceProbability`,
`ScoreLevel`, `ScoreProbability`, `DecisionState`, `DecisionUsage`, `DecisionModelMetadata`,
`DecisionError`, `DecisionConfig`。

### 资源与资产

`assets` 模块：`scan_references`, `resolve_assets`, `resolve_assets_async`, `encode_multipart`,
`content_type_from_bytes`, `reference_pattern`, `DecisionAsset`, `ImageAsset`, `Part`, `ResolvedImage`。

### 注册表

- `DecisionProfileRegistry` / `SharedDecisionProfileRegistry`：决策 profile 注册表（共享句柄）。
- `wf-llm` 的 `decision` 模块提供 `DecisionProviderConfig` 与 `AnyDecisionProvider` 的 wf 侧封装。

### 测试替身

- `MockDecisionProvider`（feature `mock`）。

## 语义约定

- **协议分发**：`DecisionProtocol::parse` 大小写不敏感；未知字符串返回 `None`。
- **资产解析**：支持同步与异步两种解析路径；multipart 编码用于带图请求。
- **注册表共享**：`SharedDecisionProfileRegistry` 为 `Arc` 共享句柄，多调用方共享同一注册表。
