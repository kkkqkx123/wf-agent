# 配置注册表与模型发现

本篇覆盖 `wf-llm` 暴露的配置与发现能力：profile/provider 注册表、请求合并、
catalog 元数据、网络模型发现与缓存。配置的文件层语义见 `docs/config/llm.md`。

## 配置层（llm_config）

- `ProfileManager`：profile 注册表（增删查、默认 profile 解析）。
- `ProviderDefinitionRegistry`：provider 定义注册表；profile 注册时按 `provider_id`
  合并 provider 默认值（显式字段优先）。
- `merge_request`：把 profile 与 request 合并为最终请求参数。
- `apply_provider_defaults`：provider 默认值 → profile。
- `apply_catalog_metadata` / `catalog_entry_for_profile`：把静态 catalog 元数据（上下文窗口、
  最大输出等）回填到 profile/model。

## 网络模型发现（llm_discovery）

`ModelCatalog`：按 provider 定义拉取模型列表。

- `new()` / `default()`：构造（共享 reqwest client）。
- `list_models(definition, api_key, proxy_creds)`：单 provider 发现（禁用/静态/端点/自定义端点）。
- `list_models_cached(..., force_refresh)`：带进程内缓存（TTL 5 分钟，按 provider+base_url 键）；
  静态与禁用定义绕过缓存。
- `refresh_all` / `refresh_filtered`：批量发现（事务性报告：每 provider 一条结果，单个失败不阻塞其余）。
- `invalidate(definition)`：清除单个 provider 的缓存。

辅助函数：

- `join_models_url(base_url, api_version, path)`：拼接 `{base}/{version}/{path}`，避免重复 version。
- `extract_models(value, json_path)`：按 `a.b[*].c` 规则从 JSON 提取模型 id。
- `filter_models` / `filter_models_by_allowlist` / `filter_models_by_prefix`：凭据过滤（allow-list、前缀）。

常量：`DEFAULT_MODELS_PATH`（`models`）、`DEFAULT_MODELS_JSON_PATH`（`data[*].id`）。

## 内置 provider 目录（wf-llm-catalog）

详见 `catalog-builtins.md`。`ModelCatalog` 与网关共用 proxy/transport 语义；发现失败在装配
时降级为警告，不阻塞 profile 注册。

## 语义约定

- **缓存**：常规查询走缓存；显式刷新通过 `force_refresh`；聚合查询每个 provider 独立成败。
- **禁用/无 base_url**：返回空列表，不报错；网络失败返回错误；空目录返回空数组——三者在接口层可区分。
- **富化顺序**：显式配置 > 在线发现 > 静态表；`format` 永不自动回填。
- **代理凭据**：`proxy_creds` 映射 route → 存储凭据，发现时瞬态合成地址，不持久化。
