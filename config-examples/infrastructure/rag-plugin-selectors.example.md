# RAG 插件选择器参考（sample，从不加载）

> 本文列出 wf-agent RAG 子系统暴露的命名扩展点。样例目录从不参与加载，
> 本文只作字段/契约参考。代码标识符保持英文。

## 通用约定

- 每个选择器是一个**已注册的名称字符串**：插件在宿主激活时把贡献注册进对应
  registry，配置里按名字引用；未注册的名称在加载期报错。
- 建议型插件（advisor/selector/postprocessor/judge）输出经宿主校验；
  **失败、空建议、非法值一律回退内置路径**，不会让查询失败。
- 协议版本常量随输入 JSON 一起发送，插件实现必须按对应版本解析。

## 扩展点一览（9 类）

| # | 配置键 | SDK trait（贡献点） | 协议/.schema | 失败回退 |
|---|---|---|---|---|
| 1 | `analyzer_name`（keyword config） | `PluginTextAnalyzer` | 宿主分词协议 | 内置分词器 |
| 2 | `encoder` / `vector_encoding` | `PluginVectorEncoding` | `vector-encoding/v1` / `named-vector-encoding/v1` | 内置 dense embedding（dense 未指定 encoder 时） |
| 3 | `metadata_extractor` | `PluginMetadataExtractor` | 宿主 JSON | 保留调用方提供的 metadata |
| 4 | `chunk_splitter` | `PluginChunkSplitter` | 宿主 JSON | 内置切分器（与 `hierarchical=true` 冲突） |
| 5 | `route_selector` | `PluginRouteSelector` | v1（`ROUTE_SELECTOR_PROTOCOL_VERSION`） | 关键字规则路由 |
| 6 | `retrieval_advisor` | `PluginRetrievalAdvisor` | v1（`RETRIEVAL_ADVISOR_PROTOCOL_VERSION`） | 融合后内置顺序 |
| 7 | `postprocessor` | `PluginPostprocessor` | v1（`POSTPROCESSOR_PROTOCOL_VERSION`） | advisor 之后的顺序 |
| 8 | `eval_judge` | `PluginEvalJudge` | v1（`EVAL_JUDGE_PROTOCOL_VERSION`） | 跳过评分 |
| 9 | 快照策略声明（宿主侧） | `register_snapshot_policy`（注册 `serde_json::Value`，非 SDK trait） | 宿主 JSON | 默认快照策略 |

## 说明

- 只有 **native** 后端分发检索类扩展；Lua/Wasm 宿主不开放检索通道
  （`wf-rag/src/lib.rs` 的声明）。这与插件权限模型一致。
- embedding / rerank **不是**插件扩展点：它们只通过 llm-suite 的
  `EmbeddingProvider` / `RerankProvider` trait 由宿主注入。
- 常量定义见 `wf-rag/src/plugin.rs`；trait 定义见
  `wf-plugin-sdk/src/contributions.rs`。

## 相关样例

- `rag-index.example.toml` — 写入侧（含 #2/#3/#4）
- `rag-query.example.toml` — 查询侧（含 #2/#6/#7）
- `rag-router.example.toml` — 路由（含 #5）
- `rag-synthesis.example.toml` — 合成（含 #8）
