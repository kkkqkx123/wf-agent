# wf-agent RAG 模块消费链路与设计评估

> 仓库：`https://github.com/kkkqkx123/wf-agent`（depth=1，HEAD `655582b update rag function`）
> 规范依据：仓库根 `AGENTS.md`（分层 crate、严格 DAG、生成产物不手改、代码内英文/文档中文）
> 评估对象：`crates/infra/rag/*`、`crates/app/wf-api`、`crates/app/wf-server`、`crates/engine/wf-tools|wf-workflow|wf-resource`、`apps/web-app`、`crates/app/cli/wf-cli-shared`
> 配套方案：整改任务与实施细节见 [rag-integration-convergence.md](./rag-integration-convergence.md)

---

## 0. 结论速览

- **RAG 内容侧分层是干净的**：`vector-contracts` / `keyword-contracts` / `source-contracts` 三个叶子契约 + `wf-rag` 门面，宿主注入后端，管线不选模型不选库。这一层设计合理。
- **真正的消费面有四个，不是两个**：`wf rag` CLI、Web 前端、Agent 工具 `rag_*`、工作流节点 `RAG_QUERY/RAG_INGEST`（还有 `wf-resource` 的 4 个引用模板）。前端与 CLI 都**只经 HTTP** 消费，不复制算法——这点是对的。
- **但集成层存在三类系统性不合理**：**配置多源**（同一检索策略有 4 套默认值来源）、**契约多形**（同一"命中"有 3 种结构、同一"摄取结果"有 3 种结构）、**能力多门**（HTTP 面与工具面由两组开关分别决定）。
- 由此产生 4 个可复现的正确性缺陷：预览检索不代表真实检索、reindex 是破坏性且丢参数的操作、`embeddings on + rag off` 能建库但 Agent 查不到、CLI 手拼查询串未做 URL 编码。
- 前端还有一个结构性缺口：**RAG 工作流节点在 UI 上无法组装**，只存在于后端 handler。
- 综合判断：**架构方向合理，集成层是"薄但平行"的实现，尚未收敛到单一契约与单一配置源**。建议按 §6 的五步收敛，而不是重写。

---

## 1. 消费面全景

| 消费面 | 入口 | 传输 | 代码位置 | 能力门禁 |
|---|---|---|---|---|
| Web 前端 | `/rag` 页面 + 7 个 service 函数 | HTTP（openapi-fetch 类型客户端） | `apps/web-app/src/lib/services/rag.ts`、`routes/rag/+page.svelte` | 只依赖 `embedding.enabled`（无句柄则 503） |
| CLI | `wf rag {stats,preview,diagnose,sources …}` | HTTP（本机服务或远程隧道） | `crates/app/cli/wf-cli-shared/src/{args_rag.rs,cmd/rag.rs,rag_render.rs}` | 同上，仅凭错误码判断 |
| Agent 工具 | `rag_search`（基线）+ 6 个增强工具 | 进程内 trait 注入 | `crates/engine/wf-tools/src/predefined/rag*`、`wf-api/src/rag_capabilities.rs` | `rag.enabled` + 子开关（ingest/eval） |
| 工作流节点 | `RAG_QUERY` / `RAG_INGEST` | 进程内 handler | `crates/engine/wf-workflow/src/handler/rag.rs` | 显式 `register_rag_handlers` |
| 提示词模板 | `rag.context.model/display`、`rag.citation`、`rag.no_hit` | 资源注册表 | `crates/engine/wf-resource/src/predefined/rag.rs` | 随能力注册 |

HTTP 契约只有 7 个操作（`crates/app/wf-server/src/api/rag.rs:296-308`）：`GET /rag/stats|preview|diagnose|sources`、`POST /rag/sources`、`POST /rag/sources/reindex`、`DELETE /rag/sources`。路由整体受 `rag-vector` feature 控制（`router.rs:167-169`），OpenAPI 快照与 TS/Rust 客户端同源生成。

**调用链（前端）**：`+page.svelte` → `services/rag.ts`（`client.GET/POST/DELETE` + `call/requireData` 拆信封）→ `/api/v1/rag/*` → `wf_api::rag::{collection_stats,preview_search,diagnose_no_hits}`、`wf_api::rag_source::RagSourceService` → `wf_rag::pipeline::{query,ingest_documents}` → `vector-contracts` / `keyword-contracts`。

**调用链（CLI）**：`args_rag.rs`（clap 定义，默认 `collection=kb/namespace=kb`）→ `cmd/rag.rs` 手拼 path 与 query string → `WfClient::get_json/post_json/delete_json_with_body`（返回 `serde_json::Value`）→ 同一批 HTTP 路由 → `rag_render.rs` 做文本渲染。

---

## 2. 前端如何使用

1. **服务层是唯一入口**：`services/rag.ts` 导出 7 个函数，类型全部取自生成 schema（`components['schemas']['Rag*']`），不在组件里拼路径。符合项目约定。
2. **页面结构**：顶部 collection/namespace 范围选择 → 统计卡（向量点数 + 关键词点数）→ 添加 source 表单（source/uri/kind/converter/options JSON/force）→ source 列表（状态、chunks、points、reindex/delete）→ 检索预览（query/top_k/excerpt）→ 诊断原因列表。
3. **能力探测靠试错**：`isUnavailable()` 只认 `SERVICE_UNAVAILABLE`/503（`+page.svelte:47-51`），没有能力发现接口可用——`ServerCapabilities` 目前只上报 `persistentPty`（`api/system/server.rs:39-44`），所以导航项 `/rag`（`config/navigation.ts:62`）永远显示，未配置的服务进去后靠一条黄色提示降级。
4. **预览模式**：`#lib/api/client.preview.ts` 别名 + `fixtures/rag.ts` + `mock/routes/rag.ts`，页面无环境分支，mock 里 source 列表可变，能连续演示增删改。
5. **工具结果呈现**：`utils/toolcalls.ts` 把 `rag_*` 归为 `knowledge`，并用键兜底（`chunk_id`/`chunkId`、`content`/`excerpt`）解析命中数组，`ToolCallCard.svelte` 展示引用列表。这是前端唯一接触"模型侧检索结果"的地方。

**前端侧问题**：

- **reindex 丢参数**（P0）：`reindex(entry)` 把 `options` 硬编码成 `{}`（`+page.svelte:140-151`）。用 `selector`/`focus`/`min_words` 抓过的页面，重建后会以默认规则重抓，且界面无任何提示。
- **无删除确认、无分页**：`remove()` 直接发请求；source 列表全量渲染，无 cursor。
- **输入未校验**：`top_k` 用 `Number(topK)` 直传，空串/非法值变成 `NaN`，服务端报参数错误（400）而非友好提示。
- **无法组装 RAG 节点**（P1）：前端节点类型是后端 `StaticNodeType` 的静态镜像（`graph/node-kind.ts:12-35`），`RAG_QUERY`/`RAG_INGEST` 是 `Custom("…")`（`handler/rag.rs:22-24`），既不在镜像里，也没有任何 node template 注册点（模板只来自插件和用户自建），AddNodeDrawer 因此永远选不到；即使手写 JSON 插进去，`window_radius`/`fusion_mode`/`keyword_candidates` 等 14 个配置字段也没有表单。

---

## 3. CLI 如何使用

命令树：`stats`、`preview QUERY`、`diagnose QUERY`、`sources {list,add,reindex,delete}`，全部走 HTTP，不内嵌引擎；`--options` 收 JSON 串，非法在参数层拒绝；文本模式经 `rag_render::format_hits` 输出（最多 10 条、截 120 字），结构化模式保留原始 DTO。

**CLI 侧问题**：

- **完全绕开生成客户端**（P1）：`wf-client/build.rs` 从 `openapi.json` 用 progenitor 生成 `handle_rag_*` 类型化方法，`analysis`/`template` 等域的 cmd 都在用（如 `cmd/analysis.rs:75`），但 RAG 七个操作全部走 `get_json/post_json` + `serde_json::Value` + 手写路径。`docs/app/client/03-rag-clients.md §4` 声称"已生成操作……都进入生成客户端"，与代码不符（文档漂移）。
- **查询串未编码**（P0）：`query_path()` 直接 `format!("{key}={value}")`（`cmd/rag.rs:179-189`）。query 含 `&`、`#`、`=`、`%` 时会被拆参或截断（空格与中文由 `url` crate 兜住，但保留字符不会）。`wf rag preview "a&b"` 实际搜的是 `a`。
- **reindex 要求重输 URI/KIND**：`RagSourceSub::Reindex` 把 source/uri/kind 都做成必填位置参数，与"服务端已知该 source"的事实重复，容易传错组合（配合 P0-2 更危险）。
- **渲染层重复解析**：`rag_render.rs` 与前端 `toolcalls.ts` 各自对同一份 JSON 做键兜底，两处规则要同步维护。

---

## 4. 设计是否合理：分项判定

| 维度 | 判定 | 依据 |
|---|---|---|
| crate 分层与依赖方向 | **合理** | `rag/*` 严格 DAG，叶契约无 `wf-` 前缀，门面 `wf-rag` 唯一，宿主只持有 backend-neutral 句柄；与 `checkpoint/*` 同构 |
| 算法与传输解耦 | **合理** | 前端/CLI 零算法；`preview_search` 只回 chunk_id/source/excerpt/score，不外泄向量 |
| HTTP 契约生成链 | **基本合理** | 服务端 DTO + utoipa + 快照 + 双端生成，契约单一来源；但 CLI 未使用生成产物，前端手写 mock 需同步 |
| 配置模型 | **不合理** | 检索策略有 4 处默认值来源（见 §5.1） |
| 数据形状 | **不合理** | 命中 3 形、摄取结果 3 形（见 §5.2） |
| 能力门禁 | **不合理** | HTTP 面与工具面双轨（见 §5.3） |
| source 生命周期 | **不合理** | 状态不是完整描述符，reindex 是"先删再写"（见 §5.4） |
| 可观测/排障 | **部分合理** | 有 stats/diagnose，但 diagnose 是纯启发式、错误分类错位 |
| 文档 | **较好，但有漂移** | 五层文档齐全（api/server/webapp/client/cli + 计划），个别条目与代码不符 |

**做得好的地方**（值得保留）：能力按子开关装配、缺失 backend 不注册工具；`SERVICE_UNAVAILABLE` 与"空结果"语义分离；`rag_search` 工具参数极简（6 个字段）、把复杂形状留给 MCP/插件；`AGENTS.md` 的"禁止在注释里引用文档编号"在 RAG 文件里执行得比较干净。

---

## 5. 关键问题清单

### 5.1 P0：预览检索与真实检索不同规（配置多源）

`wf_api::rag::preview_search` 用 `RagQueryConfig::default()`（`crates/app/wf-api/src/rag.rs:976-979`），把 `top_k` 覆盖后直接跑 `pipeline::query`：

- `keyword_candidates` 默认 `0` → **混合检索永远关闭**；
- `rerank` 默认 `None` → **重排永远不跑**；
- `window_radius=0`、`context_budget_chars=None` → 无句窗扩展、无上下文预算裁剪。

而 Agent 用的 `search_with_defaults`（`wf-tools/src/predefined/rag.rs:252-290`）读的是 `RagCapabilitySettings.query`（来自 `config.rag.retrieval`，含 hybrid/rerank/fusion/window/budget）；工作流节点又读第三套（节点 `config` 逐字段覆盖 `default_query`，`wf-workflow/src/handler/rag.rs:277-345`）；CLI 复用 HTTP 预览，因此继承第 4 套（默认值）。

**后果**：运维在新版 RAG 管理页"预览"看到的结果，与模型实际召回不一致；混合检索明明开着，预览里永远只有稠密召回，排障结论必然是错的（`diagnose` 还会顺势输出"keyword index is enabled but returned no matches"）。

### 5.2 P1：命中与摄取结果多形

| 概念 | 结构 | 位置 |
|---|---|---|
| 内核命中 | `RetrievedChunk {chunk_id, document_id, source, namespace, ordinal, content, score, prev_id, next_id, parent_id}` | `wf-rag/src/retriever.rs:19` |
| 工具命中 | `RagHit {chunk_id, document_id, source, content, score}` | `wf-tools/src/predefined/rag.rs:165` |
| HTTP 预览命中 | `PreviewHit {chunk_id, source, excerpt, score}` | `wf-api/src/rag.rs:951` |

三形差异（`content` vs `excerpt`、有无 `namespace`/`ordinal`）直接导致前端 `toolcalls.ts:98-110` 与 CLI `rag_render.rs:36-60` 必须做键名兜底解析。摄取结果同样三形：`RagSourceIngestResult`（HTTP）、`RagIngestUrlResult`（含 final_url/validator/title）、`RagSourceOperationResult`（含 removed）。

### 5.3 P0：能力门禁双轨

- HTTP 面：`rag_handles` 只在 `embedding.enabled` 时构造，`rag_source_service` 随后无条件派生（`wf-runtime/src/bootstrap/inner.rs:340-357`）→ **只要配了 embedding，`/rag/sources` 就能写库**，与 `config.rag.enabled` 无关。
- 工具面：`attach_rag_capabilities` 在 `!any_enabled()` 时直接 return（`bootstrap/runtime.rs:172-174`），`any_enabled()` 只等于 `enabled`（`wf-types/src/config/rag.rs:199-201`）→ **`rag.enabled=false` 时 Agent 拿不到 `rag_search`**。
- 反向组合：`rag.enabled=true` 且 `embedding.enabled=false` → 启动直接 `Err`（`inner.rs:710` + `runtime.rs:175-179`），整个服务起不来。

**后果**：默认安装（rag off）在 UI 上看起来"知识库可用但能力未配置"；运维若只开 embedding 建库，会得到"库里有数据、Agent 却查不到"的黑洞；把开关顺序弄反则直接启动失败。同一能力由两组开关分别决定，应当收敛为一个。

### 5.4 P0：source 生命周期不可重放

`RagSourceState`（`wf-api/src/rag_source.rs:38-72`）保存 collection/namespace/source/kind/uri/converter/vector_kind/encoder*/hash/etag/计数/status，**唯独不保存 `options`**。于是：

- `reindex` = `delete()` + `ingest(force)`（`rag_source.rs:604-615`）：先删索引与状态，再重新读；读失败（网络抖动、目标站点 5xx）→ **原有内容已被清空**，只剩一条 failed 状态。
- 前端只能用 `options: {}` 重建（§2），CLI 只能让人重新敲 URI/KIND（§3）。同一 source 重建后读取规则静默变化，而 `content_hash` 会更新，事后无从对账。
- `from_descriptor` 里 `vector_kind` 恒为 `"dense"`、`encoder` 恒 `None`（`rag_source.rs:88-90`），稀疏/命名编码场景下状态记录与实际写入不符。

此外 `ingest` 的翻页循环是 `for _ in 0..64`（`rag_source.rs:424`），第 64 页后**静默停止**，超大源会被截断且返回成功。

### 5.5 P1：source 枚举的自维护注册表理由已过时

`wf-api/src/rag.rs:1-6` 的模块注释把"每个 namespace 维护一小份已知 source 注册表"归因于**存储契约没有扫描调用**。但 `VectorStorage` 已含 `scroll(collection, ScrollQuery{filter, limit, offset})`（`vector-contracts/src/store.rs:479`、`:335-342`）。即：真实约束已不存在，而由此产生的代价是——状态存储损坏/迁移丢失后，索引里真实存在的 source 在 UI 与 CLI 中**完全不可见**，也无法列举"哪些 source 属于这个 namespace"。

### 5.6 P1：没有集合/命名空间发现，也没有能力总览

`collection_stats` 不校验 collection 是否存在（`rag.rs:876-908`），拼错名字与空库都返回 `point_count: 0`；契约里 `ensure_collection` 存在但没有列举集合的方法，页面只能让人手输（默认 `kb`）。`docs/plan/rag-frontend-cli-adoption.md §1` 自己就把"无面向 UI 的能力总览"列为缺口，落地时只做了 5 个 DTO，能力总览没有对应端点，前端只能靠 503 试错。

### 5.7 P1：错误分类错位

`collection_stats` / `preview_search` 把所有后端错误映射成 `ApiError::Validation`（`rag.rs:889-901`、`:983`），即 HTTP 400。向量库不可达、维度不匹配这类故障被报成"参数非法"，前端 `isUnavailable()` 只识 503，会把基础设施故障显示成红色错误条并引导用户改参数。

### 5.8 P2：其余

- **DELETE 带 body**（`wf-server/src/api/rag.rs:542`）迫使客户端新增 `delete_json_with_body`（`wf-client/src/client.rs:914`），部分代理/网关会丢 DELETE 体；`DELETE /rag/sources/{source}?collection=&namespace=` 更稳。
- **请求结构重复**：`RagScope`（都必填）、`RagPreviewQuery`、`RagDiagnoseQuery`（scope 可选、参数最多）、`RagSourceWrite`、`RagSourceDelete` 五个结构表达 3 个概念，diagnose 是唯一允许省略 scope 的。
- **diagnose 不查数据**：`diagnose_no_hits`（`rag.rs:1041-1080`）纯按入参形态拼字符串，从不真正执行查询，却输出"keyword index is enabled but returned no matches"这类断言；只有同时给了 collection+namespace 时才追加一行点数。
- **`rag_ingest_url` 与 `rag_ingest_source(kind=url)` 重叠**：前者是后者的特化包装（`rag_capabilities.rs:293-315` 固定 `converter="markdown"` 并转换输出形状），模型要在两个近义工具间选，维护要改两处。
- **命名遮蔽**：`wf_types::config::rag::RagCapabilitiesConfig`（声明式配置）与 `wf_tools::predefined::rag::RagCapabilitiesConfig`（backend 槽位）同名不同义，同一文件 `rag_capabilities.rs` 里 import 的是后者，`bootstrap` 里用的是前者。

---

## 6. 建议的收敛路径（保持现有分层，不重写）

1. **单一配置源**：新增 `GET /rag/capabilities`（或扩展 `/rag/stats`），返回有效检索策略（hybrid、fusion、rerank、window、budget、chunking、vector_kind、defaults、可用性）。`preview_search` 改为接收该策略（或接受 `mode=exact` 参数），让预览、CLI、页面与 `rag_search` 走同一份 `RagQueryConfig`。
2. **单一点形状与配置开关**：定义 `RetrievalHit {chunk_id, document_id, source, namespace, content, score}` 与 `IngestResult` 各一份，工具输出、HTTP DTO、CLI 渲染、前端卡片全部消费它；顺手把 `rag_ingest_url` 折叠成 `rag_ingest_source` 的 `kind=url` 预设。
3. **门禁合一**：`rag.enabled` 同时决定 HTTP 面与工具面（或 `rag.enabled` 隐含 `embedding.enabled`），删掉"HTTP 面只看 embedding"的隐式规则；`rag.enabled` 但缺 embedding 时降级为 warning + 只读面关闭，而不是启动失败。
4. **source 可重放**：`RagSourceState` 增补 `options`（及 `chunking`/`encoder` 实际值），`reindex` 改为两阶段（读全 → 暂存 → 删 → 写，失败回滚），翻页上限改成显式报错；`sources list` 用 `scroll` 兜底，注册表退化为"增量索引"而非唯一真相。
5. **消费端对齐**：CLI 七个操作改用 `openapi.json` 生成的 `handle_rag_*`（与 analysis/template 域一致，顺带消灭手拼 query string）；补 `RAG_QUERY`/`RAG_INGEST` 的 node template 与最简配置表单，或明确在文档里声明"仅支持 JSON 编排"。

**验收建议**：一次 `cargo clippy --all-targets --all-features`＋`bash scripts/sync-deploy.sh --check`；一条 e2e：开启 hybrid+rerank 后对同一 query 断言 `/rag/preview` 与 `rag_search` 命中集合一致；一条回归：`options` 非空的 URL source 重建后 `options` 不变、内容哈希变化。

---

## 7. 附：本次核对的关键证据位置

```
crates/infra/rag/wf-rag/src/lib.rs                门面导出，无后端耦合
crates/infra/rag/vector-contracts/src/store.rs:479  scroll 契约（推翻"无扫描调用"的注释理由）
crates/engine/wf-tools/src/predefined/rag.rs:41,252  工具 schema + search_with_defaults
crates/engine/wf-workflow/src/handler/rag.rs:22,277  节点类型 + 第三套查询配置
crates/app/wf-api/src/rag.rs:1,876,951,976,1041      stats / preview / 诊断 / 预览默认值
crates/app/wf-api/src/rag_source.rs:38,424,604       state 字段 / 64 页上限 / 破坏性 reindex
crates/app/wf-api/src/rag_capabilities.rs:29,235,293 设置映射 / 工具装配 / ingest_url 特化
crates/app/wf-server/src/api/rag.rs:296,542          7 条路由 / DELETE 带 body
crates/app/wf-runtime/src/bootstrap/inner.rs:340,710 句柄与 source service / 工具装配触发点
crates/app/wf-runtime/src/bootstrap/runtime.rs:165   attach_rag_capabilities 门禁
crates/app/cli/wf-cli-shared/src/cmd/rag.rs:179     手拼 query string（未编码）
apps/web-app/src/routes/rag/+page.svelte:140        reindex 丢 options
apps/web-app/src/lib/graph/node-kind.ts:12          前端节点类型镜像（无 RAG）
docs/plan/rag-frontend-cli-adoption.md              本轮接入的设计意图与三处未落地缺口
```
