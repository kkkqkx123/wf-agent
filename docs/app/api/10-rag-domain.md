# RAG 域功能分析

> 范围：`crates/app/wf-api` 的检索与数据源服务、`crates/app/wf-server` 的 RAG 路由，以及引擎侧工具与节点的注入边界
> 定位：说明应用层如何消费已有的 `wf-rag` 管线，不重复描述分块、融合与重排算法

应用层不实现检索算法。`wf-rag` 负责清洗、切分、嵌入、索引、召回、融合和引用；`wf-api` 负责把这些能力包装成宿主可注入的服务与稳定的 HTTP 契约；`wf-server` 只做路由、信封和 OpenAPI 文档。前端与 CLI 都只消费 HTTP 契约。

## 1. 分层职责

| 层 | 组件 | 职责 |
|---|---|---|
| 引擎工具 | `wf-tools::predefined::rag` | 定义 `rag_search` 与增强工具的参数 schema、风险等级和输出形状；执行 backend 由宿主注入 |
| 引擎节点 | `wf-workflow::handler::rag` | `RAG_QUERY` / `RAG_INGEST` 工作流节点，使用同一组检索句柄；仅显式注册时进入节点集合 |
| 运行时 | `wf-runtime::bootstrap` | 打开向量存储与关键词索引，构造 embedding provider 与 `RagHandles`，装配 source service 和工具能力 |
| 应用门面 | `wf-api::rag` | 只读统计、检索预览、无命中诊断；打开后端与 provider 的宿主函数 |
| 应用门面 | `wf-api::rag_source` | source 描述符的状态机：读取、转换、切分、索引、增量跳过、强制重建、删除与状态列举 |
| 应用门面 | `wf-api::rag_capabilities` | 把检索、摄取、源管理、状态、评估包装成工具 backend，并把配置映射成工具默认值 |
| HTTP 传输 | `wf-server::api::rag` | 5 组路由、请求/响应 DTO、错误映射、OpenAPI 注解 |

## 2. 运行时装配

运行时按配置顺序完成装配：

1. 解析关键词索引位置与分词器，打开共享关键词索引；未启用时槽位为空，读取退化为纯稠密检索。
2. 解析向量后端位置，打开共享向量存储；后端由配置显式选择，内存后端仅用于开发。
3. 当 embedding 开启时构造 provider，并与向量存储、关键词索引一起组成 `RagHandles`，注入 `ApiContext`。
4. 有检索句柄时创建 `RagSourceService`，复用统一 web fetch backend，遵守代理、域名和 robots 策略；source 状态写入持久化存储。
5. 主开关开启时调用 `attach_rag_capabilities`，注册工具定义与执行 handler，并登记工具描述资源。

主开关是工具装配的唯一总闸。只读检索属于主开关能力，不要求同时开启摄取或评估；摄取、源管理、状态与评估工具各自还有子开关，未配置 backend 的工具不注册。这样工具列表与实际可执行性一致。

## 3. wf-api 服务边界

### 3.1 只读检索面

`wf-api::rag` 提供三类宿主函数：

- `collection_stats`：按 collection 与 namespace 统计向量点数，并在关键词索引存在时返回关键词点数，供界面发现双写漂移。
- `preview_search`：用标准查询管线取得命中，但只输出 chunk 标识、source、截断摘录和分数，不向管理接口暴露向量。
- `diagnose_no_hits`：根据查询形状与关键词开关给出可能原因；调用方可追加具体命名空间的点数提示。

这些函数不读取插件注册表和配置源码，查询规约仍由 `RagQueryConfig` 承担。

### 3.2 source 同步面

`RagSourceService` 以 `SourceDescriptor` 为输入，流程是：读取 → 归一化转换 → 切分 → 索引 → 保存状态。

- 内置读取器覆盖 URL 与本地文件；文件读取受配置的文件根约束。
- revision 由内容哈希与可选的 HTTP 校验信息组成；未变化时默认跳过，显式 force 或 reindex 才重写。
- 状态以 collection、namespace、source 三元组为键持久化；失败 source 保留错误信息并允许重试。
- `delete` 删除状态记录并清理对应索引；`status` 把持久化状态与实时点数合并后返回。

### 3.3 工具能力面

`RagCapabilityService` 实现工具 crate 定义的 backend trait：

| Backend | 工具 | 门禁 |
|---|---|---|
| `RagSearchBackend` | `rag_search` | 主开关开启且检索句柄可用 |
| `RagIngestBackend` | `rag_ingest_url` | 摄取子开关 |
| `RagSourceBackend` | `rag_ingest_source` / `rag_delete_source` / `rag_reindex_source` | 摄取子开关 |
| `RagStatusBackend` | `rag_index_status` | 摄取子开关 |
| `RagEvalBackend` | `rag_evaluate` | 评估子开关 |

工具输出保持中立：`rag_search` 返回命中数组，`chunk_id` 是引用定位符；状态与摄取结果是独立结构，不混入模型检索结果。

## 4. HTTP 契约

路由整体受 `rag-vector` feature 控制，路径与参数详见 [16-rag.md](../server/16-rag.md)。本层补充响应契约：

| 路由 | 响应数据 |
|---|---|
| `GET /rag/stats` | `RagCollectionStatsView`：collection、namespace、向量点数、可选关键词点数 |
| `GET /rag/preview` | `RagPreviewHitView` 数组：chunk 标识、source、摘录、分数 |
| `GET /rag/diagnose` | `RagNoHitDiagnosisView`：可能原因数组 |
| `GET /rag/sources` | `RagSourceStatusEntry` 数组：持久化状态 + 实时点数 |
| `POST /rag/sources` / `POST /rag/sources/reindex` | `RagIngestResultView`：动作、计数、内容哈希、revision、metadata |
| `DELETE /rag/sources` | 受影响 source 数量 |

这些 DTO 只存在于服务端，用于把原本的通用 JSON 响应注解变成可生成的 schema；运行时 JSON 字段保持原样。OpenAPI 快照由测试刷新，Web 类型文件与 Rust 生成客户端都从该快照产出，链路见 [09-openapi-doc-generation.md](./09-openapi-doc-generation.md)。

错误语义：参数非法返回校验错误；未注入检索句柄时 stats/preview 返回服务不可用；未注入 source service 时 source 路由返回服务不可用；诊断不依赖检索句柄，始终可以基于查询形状工作。上层应用据此把“能力未配置”与“空结果”区分开。

## 5. 上层消费

- Web 前端的页面、服务层和预览固件见 [../webapp/02-rag.md](../webapp/02-rag.md)。
- 两个 HTTP 客户端的类型同步与调用封装见 [../client/03-rag-clients.md](../client/03-rag-clients.md)。
- CLI 命令树与命中渲染见 [../cli/07-rag.md](../cli/07-rag.md)。
