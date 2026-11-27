# RAG 集成层收敛：任务清单与修改方案

> 范围：`crates/app`（`wf-api` / `wf-server` / `wf-runtime` / `cli/wf-cli-shared`）、`crates/engine/wf-workflow`、`crates/infra/rag/vector-contracts`（§6 预存修复）、`apps/web-app`
> 上游分析：[rag-integration-review.md](./rag-integration-review.md)（消费链路盘点与设计评估）
> 约束：遵循 `AGENTS.md`——不保留向后兼容、代码内英文、文档中文、生成产物不手改、crate 依赖保持 DAG

## 1. 问题与目标

分析阶段确认 RAG 的分层（`vector-contracts` / `keyword-contracts` / `source-contracts` + `wf-rag` 门面）是合理的，问题全部落在"集成层"：同一份检索策略有 4 套默认值来源、同一份命中/摄取结果有 3 种结构、同一能力由 2 组开关分别决定。本轮目标是把这三件事各自收敛到唯一来源，并修掉由此产生的 4 个可复现缺陷。

不改变召回、融合、重排、引用、分块与数据源读取算法；不引入新的后端或模型选择。

## 2. 缺陷与对应任务

| 编号 | 缺陷 | 触发条件 | 任务 |
|---|---|---|---|
| D1 | `/rag/preview` 用内置默认值跑检索：混合检索与重排永不生效，与管理页展示的"实际效果"不符 | 配置开启 hybrid/rerank 后预览仍只有稠密结果 | T1 |
| D2 | 重建 source 是"先删再写"，读取失败即丢索引；且状态不存读取选项，前端只能以空 options 重建 | 源站 5xx 或网络抖动时点重建 | T2、T3 |
| D3 | 管理面只需 `embedding.enabled` 即可写库，工具面还需 `rag.enabled`：库建好了但 Agent 查不到；反向组合启动即失败 | 只开 embedding 或只开 rag | T4 |
| D4 | CLI 手拼查询串未编码：query 含 `&`、`#`、`=` 时被拆参或截断 | `wf rag preview "a&b"` 实际搜 `a` | T5 |
| D5 | 命中 3 形、摄取结果 3 形，前端与 CLI 各写一套键名兜底解析 | 新增字段需改两处解析 | T1（预览复用工具命中） |
| D6 | 工作流 RAG 节点不读宿主配置，节点默认查询与工具、预览三者不一致 | 图里配了节点但召回策略与工具不同 | T6 |
| D7 | CLI 七个 RAG 操作全部走 JSON escape hatch，与 `analysis`/`template` 域的类型化调用不一致 | 契约变更时 CLI 静默落后 | T5 |
| D8 | 后端故障被映射成 400，运维误判为参数错误 | 向量库不可达 | T7 |

## 3. 任务清单

状态：`已完成`＝本轮已改并验证；`待办`＝已记录、需后续排期（附原因）。

### T1 预览与工具共用同一条检索路径（已完成）

`wf_api::rag::preview_search` 不再自建 `RagQueryConfig::default()`，改为接收宿主解析的 `&RagQueryConfig` 与关键词索引，并直接委托 `wf_tools::predefined::rag::search_with_defaults`。预览与 `rag_search` 从此共用同一条路径：相同的 hybrid 门禁、相同的重排、相同的句柄窗扩展与上下文预算；差异只剩渲染（截断摘录、不外泄向量）。

为保证调用点可读，请求参数收进 `PreviewParams`（collection / namespace / query / top_k / excerpt_chars），同时消除 clippy 的 `too_many_arguments`。

- 文件：`crates/app/wf-api/src/rag.rs`
- 断言：`preview_honours_injected_query_defaults`（注入 1.5 分阈值后命中为空，默认策略下命中非空）

### T2 重建改为两阶段（已完成）

`RagSourceService::ingest` 拆为 `fetch`（读取+哈希+revision，不碰索引）与 `write`（判重、清理旧点、转换、写入、落状态）。`reindex` 先 `fetch`，成功后才 `delete`，再 `write`。读取失败时索引与状态记录原样保留，不再出现"点被清空、只剩一条 failed 状态"。

同时把翻页上限从 `for _ in 0..64` 的静默截断改为显式错误：超过 `MAX_SOURCE_PAGES` 时报错说明来源需要拆分或提高上限。

- 文件：`crates/app/wf-api/src/rag_source.rs`
- 断言：`failed_rebuild_keeps_indexed_content`（读失败后点数不变、状态仍为 active）

### T3 状态记录保存完整描述符（已完成）

`RagSourceState` 增加 `options` 字段（`#[serde(default)]`，旧记录反序列化为 `null` 不报错），由 `from_descriptor` 写入；新增 `RagSourceState::descriptor()` 由其重建 `SourceDescriptor`。`reindex_known(collection, namespace, source)` 从状态读回 kind / uri / converter / options 后重建，未知源返回 `NotFound`。

HTTP 与 CLI 因此只需三元组：`POST /rag/sources/reindex` 的请求体由 `RagSourceWrite` 改为 `RagSourceRef`，与 DELETE 共用；前端不再传 `options: {}`，CLI 不再要求重输 URI/KIND。

- 文件：`crates/app/wf-api/src/rag_source.rs`、`crates/app/wf-server/src/api/rag.rs`、`crates/app/wf-server/src/openapi.rs`、`apps/web-app/src/lib/services/rag.ts`、`apps/web-app/src/routes/rag/+page.svelte`、`apps/web-app/src/lib/api/mock/routes/rag.ts`
- 断言：`rebuild_replays_persisted_options`（第二次读取收到的仍是首次的 `selector`；描述符 URI 无扩展名，只有 converter 被正确重放才能通过）

### T4 一个开关驱动全部检索面（已完成）

`bootstrap/inner.rs` 只在 `config.rag.enabled` 时构造 `RagHandles` 与 `RagSourceService`，管理路由、工具、工作流节点随之同开同关；`rag.enabled` 但缺 embedding 时仍在 `attach_rag_capabilities` 显式报错，不再出现"能写库但查不到"的中间态。

- 文件：`crates/app/wf-runtime/src/bootstrap/inner.rs`

### T5 CLI 走生成客户端（已完成）

`cmd/rag.rs` 的七个操作改用 `client.generated().handle_rag_*`，与 `analysis` / `template` 域一致：查询参数由 progenitor 的 `QueryParam` 序列化（D4 随之消失），请求体用 `wf_client::models::{RagSourceWrite, RagSourceRef}`，信封统一经 `unwrap_generated_envelope` 拆解。删除手写的 `query_path` / `source_body`，`--options` 仍只接受 JSON 对象。

- 文件：`crates/app/cli/wf-cli-shared/src/cmd/rag.rs`、`crates/app/cli/wf-cli-shared/src/args_rag.rs`

### T6 工作流节点共享宿主默认值（已完成）

`RagNodeHandles` 增加 `with_default_query` / `with_default_index`；`ApiContext` 新增 `rag_defaults: Option<RagDefaults>`，由 `bootstrap` 从 `RagCapabilitySettings` 解析一次，`rag_node_handles()` 用它填默认值，`api_context.rs` 不再硬编码 `("kb", "kb")`。

- 文件：`crates/engine/wf-workflow/src/handler/rag.rs`、`crates/app/wf-api/src/infra/context.rs`、`crates/app/wf-runtime/src/bootstrap/{inner,runtime,api_context}.rs`

### T7 故障不再伪装成参数错误（已完成）

`preview_search` 的后端错误由 `ApiError::Validation` 改为 `ApiError::execution`，`preview` 不再把向量库故障报成 400。`collection_stats` 的同类映射属既有行为，留待 T11 统一处理（改动会波及 stats 的全部调用点与前端提示文案）。

- 文件：`crates/app/wf-api/src/rag.rs`

### T8 契约产物同步（已完成，需复核一次）

- `crates/app/wf-server/openapi.json`：`RagSourceDelete` → `RagSourceRef`（含 `description`），reindex 请求体指向该 schema。
- `apps/web-app/src/lib/api/schema.d.ts`：由 `tools/openapi-codegen` 的 `npm run gen` 从快照重新生成，未手改。
- `wf-client` 的 `codegen.rs` 在构建期从同一快照生成，CLI 编译时已跟随。

> 快照已用权威方式刷新：`WF_REFRESH_OPENAPI=1 cargo test -p wf-server --all-features --lib openapi::tests::committed_snapshot_matches_document`，随后 `committed_snapshot_matches_document` 与 `annotations_are_registered_in_document` 均通过。相对 `HEAD` 的净变化核对为：schema `RagSourceDelete` → `RagSourceRef`（+description）、reindex 请求体 `$ref` 指向新名，无其它漂移。

同一轮刷新还补入了 §6.2 修复带来的两条 path（`POST /api/v1/agent-loops/{id}/compact`、`GET /api/v1/system/backends/{name}/logs`）及其 5 个 schema，因此 `schema.d.ts` 也重新生成了一次（`npm run gen`，+247/−3）。

### T9 文档对齐（已完成）

`docs/app/server/16-rag.md`、`docs/app/api/10-rag-domain.md`、`docs/app/webapp/02-rag.md`、`docs/app/client/03-rag-clients.md`、`docs/app/cli/07-rag.md` 已按新契约更新（reindex 三元组、单开关、预览同规、CLI 类型化调用）。

### T10 画布可插入 RAG 节点（待办）

`RAG_QUERY` / `RAG_INGEST` 是 `Custom(...)` 节点类型，前端 `STATIC_NODE_TYPES` 里没有，节点面板列出的是**存储层**模板（`GET /api/v1/templates/node` 只读 `storage.node_template`），因此向内存注册表登记模板对画布无效。真正落地需要：在 `wf-api::template::node_template` 增加内置模板的幂等播种（复用 `validate_template` + dry-run），再由 `bootstrap` 在能力开启时调用一次；随后补节点配置表单（14 个字段）。

补充核实（本轮）：`AddNodeDrawer.svelte` 的候选 100% 来自 `GET /api/v1/templates/node`，没有静态回退；而 `bootstrap` 只把 `NodeTemplate` 注册进内存 `ResourceRegistries`，从不写 `storage.node_template`。因此不仅 RAG 节点，任何内置节点在干净安装下都不出现在面板里，用户只能加"空白节点"（默认 `SCRIPT`）。这进一步说明缺口不在 RAG，而在"内置模板播种"这一缺失环节。

未做原因：属于"图编辑器可编排性"独立议题，涉及存储写入与模板校验，改动面大于本轮集成收敛；且没有前端表单时即使能插入节点也只能落到默认配置。

### T11 其余记录在案的改进（待办）

| 项 | 说明 |
|---|---|
| `collection_stats` 错误分类 | 与 T7 同类，需同时调整前端"服务不可用/参数错误"的判定与文案 |
| 集合与命名空间发现 | 契约有 `ensure_collection` 却无列举；管理页只能手输 `kb`，拼错与空库不可区分 |
| 能力总览端点 | 页面无法显示生效的 hybrid/rerank/chunking；`ServerCapabilities` 只上报 `persistentPty` |
| `rag_ingest_url` 折叠 | 实为 `rag_ingest_source(kind=url)` 的特化包装，两套参数与输出 |
| `DELETE` 带 body | 建议 `DELETE /rag/sources/{source}?collection=&namespace=`，避免代理丢体 |
| 诊断去启发式 | `diagnose_no_hits` 不执行查询，文案可能与事实相反 |
| source 反向枚举 | `VectorStorage::scroll` 已存在，可据此补齐"索引里有但状态里没有"的 source |

## 4. 变更影响面

| 层 | 变化 | 消费方需跟进 |
|---|---|---|
| HTTP | reindex 请求体 `RagSourceWrite` → `RagSourceRef`；错误码 preview 500 语义修正 | 前端服务层、mock、CLI、Rust 生成客户端 |
| 配置语义 | `rag.enabled=false` 时管理路由与工具同时消失（此前路由仍在） | 运维文档：只配 embedding 不再暴露知识库页面 |
| 工作流 | 节点默认查询/写入策略改由宿主配置注入 | 既有图若依赖内置默认值，行为会随配置变化 |
| CLI | `rag sources reindex` 去掉 URI/KIND 位置参数 | 脚本需改为 `reindex SOURCE --collection --namespace` |
| 工具 | 无变化（`rag_search` 参数与输出不变） | — |

配置迁移：`rag.enabled=true` 时必须同时启用 `embedding`，否则启动报 `RAG capabilities require a configured embedding provider`（不再静默半可用）。

## 5. 验证

已执行（本机 Rust 1.100.0 / Node 22.13）：

| 检查 | 命令 | 结果 |
|---|---|---|
| 全工作区编译（全特性、含测试目标） | `cargo check --workspace --all-features --all-targets` | 通过，无 error / 无新增 warning |
| 向量契约（simvec + qdrant + memory 全特性） | `cargo test -p vector-contracts --all-features --lib` | 79 passed |
| RAG API 层 | `cargo test -p wf-api --all-features --lib` | 572 passed（含本轮新增 3 个用例） |
| HTTP 层（含 OpenAPI 全部不变量） | `cargo test -p wf-server --all-features --lib` | 311 passed，含 `committed_snapshot_matches_document` 与 `annotations_are_registered_in_document` |
| 其余受影响 crate | `cargo test --no-fail-fast -p wf-workflow -p wf-resource -p wf-runtime -p wf-cli-shared --all-features --lib` | 394 / 170 / 184 / 176 passed |
| OpenAPI 快照刷新 | `WF_REFRESH_OPENAPI=1 cargo test -p wf-server --all-features --lib openapi::tests::committed_snapshot_matches_document` | 已刷新并复核净变化 |
| 前端类型生成 | `cd tools/openapi-codegen && npm run gen` | `schema.d.ts` 与服务端快照一致 |
| 前端类型检查 | `cd apps/web-app && npm run check` | 0 error / 0 warning |
| Lint（改动 crate） | `cargo clippy -p wf-api -p wf-cli-shared -p vector-contracts --all-targets` | 改动文件零告警；余下告警全部位于 `simvec` 子模块自身代码 |
| 格式 | `rustfmt --edition 2021 <改动文件>` | 已格式化（仓库整体非 fmt-clean，未全量重排） |

待执行（需运行中的服务或真实源站，本地无法覆盖）：

| 检查 | 说明 |
|---|---|
| 预览与工具一致性 e2e | 开启 hybrid + rerank 后比对 `/rag/preview` 与 `rag_search` 的命中集合应完全相同 |
| 重建回归 | `options` 非空的 URL source 重建后 options 不变、`content_hash` 随内容变化 |
| CLI 特殊字符 | `wf rag preview "a&b#c"` 的命中数应与 `"a%26b%23c"` 一致（编码修复的端到端确认） |

## 6. 预存问题修复（本轮一并处理）

以下三项与 RAG 集成收敛无耦合，但属于同一个仓库 HEAD 上"编译不过 / 查询必失败 / 契约缺项"的既有缺陷，会直接阻断本轮的验证闭环，因此一并修复。

### 6.1 simvec 适配层与引擎 API 对齐

`crates/infra/rag/vector-contracts/src/simvec_adapter/` 是照着另一版 `simvec` API 写的：`cargo check -p vector-contracts --features simvec` 报 15 个错误，连带 `wf-server` 默认特性无法编译（`default` 含 `rag-vector`）。核对 pin（`5a37af0`）与远端 `main`（`a55fc69`）后确认两处都是同一形态，不是子模块过旧。

六处结构性错位与对应改法：

| 错位 | 引擎现状 | 适配层原假设 | 改法 |
|---|---|---|---|
| 点位负载 | `VectorPoint { id, vectors: BTreeMap<String, VectorData>, payload }`，默认空间用 `DEFAULT_VECTOR_NAME`（空串） | `point.vector` + `point.data` + `point.named` 三字段 | 按"是否只有默认空间"分流：只有默认键走单空间路径，否则整体进 `named` |
| 空间声明 | `VectorsConfig` 是 `BTreeMap<String, VectorSpaceConfig>` | 枚举 `Single \| Named` | 用 `!is_named_layout()` 判空空间布局 |
| 空间参数 | `VectorSpaceConfig` 新增 `multi_config: Option<MultiConfig>` | 三字段结构体 | kind 为 `Multi` 时填 `MultiConfig::new(16)`（命名空间禁止 multi，此分支不可达） |
| 多向量载荷 | `VectorData::Multi(MultiVector)`，扁平 `flat + dim` | `Multi(Vec<Vec<f32>>)` | 回程用 `multi.vectors()` 按维切块还原嵌套 |
| 稀疏声明 | `SparseConfig` 已删；`with_sparse(vector_size, distance)` 固定 `SPARSE_DIM_BOUND` | `with_sparse(vocab, distance, SparseConfig{vocab})` | 只传距离；稀疏索引是开放 `u32` id，collection 不再声明词表上界 |
| 多向量上限 | `MultiConfig` 新增 `comparator` | 仅 `max_vectors_per_point` | 改用 `MultiConfig::new(n)` |

行为差异（需要知晓，不是回归）：`simvec` 现在对稀疏检索按 collection 的 `distance` 打分。旧的 `CollectionSpec::new` 默认 `Cosine`，而旧引擎对稀疏恒用裸点积，因此同一份测试数据从 `3.0` 变成 `3.0/√10`。契约层 `validate_sparse_distance` 本就只允许 `Dot`/`Cosine`，说明距离语义一直存在，是引擎补齐了实现。回归测试因此改为：显式 `Dot` 断言裸点积 `3.0`，另建 `Cosine` 集合断言归一化值，把两种读法都钉死。

- 文件：`simvec_adapter/mapping.rs`、`simvec_adapter/store.rs`、`simvec_adapter/test.rs`
- 验证：`cargo test -p vector-contracts --features simvec` → 54 passed；`cargo check --workspace --all-features --all-targets` → 通过

### 6.2 qdrant 检索请求对稀疏/多向量查询必然失败

`QdrantVectorStore::search_body` 先调 `validate_search_params`，而后者无条件执行 `query.vector.as_dense()?` 只为检查元素有限性。结果是任何 `Sparse` / `Multi` 查询在构造请求体前就被拒绝：`InvalidInput("expected a dense vector, got sparse")`。同一文件里 `query_body` 明明有稀疏与多向量的分支，说明这段校验是退化的遗留写法，`point_and_query_bodies_wrap_non_dense_vectors` 一直在失败。

改法：把有限性检查改为按载荷种类分派（`Dense` 走切片、`Sparse` 走 `values`、`Multi` 走嵌套展平），错误文案不变。

- 文件：`crates/infra/rag/vector-contracts/src/qdrant/codec.rs`
- 验证：`cargo test -p vector-contracts --all-features --lib` → 79 passed（此前 78 passed + 1 failed）

### 6.3 OpenAPI 注解未注册（compact / backend-log）

`annotations_are_registered_in_document` 失败：`POST /api/v1/agent-loops/{id}/compact` 与 `GET /api/v1/system/backends/{name}/logs` 两个 handler 有 `#[utoipa::path]` 注解、也在 router 里挂载，但没有列进 `ApiDoc::paths`。后果是运行时能调，发布契约里没有，TypeScript 与 Rust 生成客户端都永远不生成这两个方法（`routes_match_utoipa_annotations` 只校验"路由有注解"，抓不到这个方向）。

改法：两条 path 加入 `paths(...)` 对应分组；`CompactBody`、`CompactResponse`、`BackendLogView` 及两个 `ApiEnvelope<...>` 加入 `components(schemas(...))`，与同文件既有风格一致。

- 文件：`crates/app/wf-server/src/openapi.rs`
- 验证：`annotations_are_registered_in_document` 通过；`cargo test -p wf-server --all-features --lib` → 311 passed

## 7. 回滚

无数据迁移：状态记录新增字段带 `serde(default)`，旧记录可直接读；HTTP 契约只有 reindex 请求体形状变化，且旧的 `RagSourceWrite` 请求体在语义上是新形状的超集（多余字段被忽略前会先判重，故建议同版本内前后端一起升级）。回滚按目录粒度执行即可（`git checkout` 上述文件），但必须同时回退 `openapi.json` 与 `schema.d.ts`，否则生成客户端与运行时不一致。

预存问题修复（§6）与本轮 RAG 收敛无耦合，可独立回退：`simvec_adapter/`、`qdrant/codec.rs`、`openapi.rs` 的改动单独撤销只会让仓库回到"`wf-server` 编译不过 / 两个 handler 不可见"的既有状态，不影响 RAG 行为。
