# 内置 Provider 目录与模型元数据

本篇覆盖 `wf-llm` 暴露的内置 provider 定义与模型元数据解析：`wf-llm-catalog`
crate。它提供编译期内置的 provider 定义列表与从内嵌 LiteLLM 表派生的模型元数据。

## 内置 provider 定义

- `builtin_provider_definitions()`：返回所有内置 `LlmProviderDefinition`。
- `builtin_provider_ids()`：返回内置 provider id 列表。
- `register_builtin_providers(registry)`：把内置定义注册到 `ProviderDefinitionRegistry`。

常量：`BUILTIN_*_ID`（每个内置 provider 的 id，如 `BUILTIN_OPENAI_ID`、
`BUILTIN_ANTHROPIC_ID`、`BUILTIN_GITHUB_COPILOT_ID` 等）。

OpenCode 相关常量：`OPENCODE_CLIENT_HEADER`、`OPENCODE_CLIENT_NAME`、`OPENCODE_SESSION_HEADER`。

## 自定义 LiteLLM

- `custom_litellm_of` / `litellm_provider_for` / `resolve_litellm_namespace` /
  `provider_mapping_for`：provider id → LiteLLM 命名空间映射与自定义 LiteLLM 处理。
- `model_metadata` / `model_metadata_for` / `model_metadata_for_namespace` /
  `model_metadata_for_namespaces`：按 provider/model 查询模型元数据。
- `ModelMetadataResolver`：模型元数据解析器（从内嵌表派生）。

## ModelMetadataResolver

与计价同源（同一份内嵌 `litellm_public.json`），回答计价之外的互补问题：上下文窗口、
最大输出、图像输入、推理支持、思考等级、模型类型与线格式。

- 仅保留 `chat` / `responses` / `image_generation` 模式（`responses` 视为 Chat）。
- 两段式解析：先 JSON 再强类型；未知字段忽略；真实条目违反 schema 则构建期失败。
- `metadata_for_provider(litellm_provider, model)`：先全名查；否则命名空间拼接；最后回退短名；
  返回的 `id` 回显请求模型名而非表键。
- 费用字段恒为 `None`（计价走 pricing 路径）。
- 线格式推断：anthropic → Anthropic；gemini/vertex → GeminiNative；其余 → OpenaiChat。

## 语义约定

- **内嵌表**：模型元数据随内嵌价格表发版；运维即时覆盖走文件层自定义 provider 声明，不改代码。
- **命名空间映射**：内建映射 + 文件层 `litellm_provider` 声明；自定义网关无映射时回退规范短名。
