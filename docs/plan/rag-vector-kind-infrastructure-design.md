# RAG 向量种类扩展：向量侧基础设施设计

> 本文是 `docs/plan/long-term/rag-vector-kind-extension-design.md` 的落地前置设计，只覆盖**向量侧**（`vector-contracts` 及其三个后端）的基础设施改造。按当前决定，先忽略立项门槛，提前实现所需基础设施，为后续把稀疏/多向量/具名向量作为可选功能接入做准备；本轮不写任何消费分支，也不触发 `wf-rag` 的编码器与融合改造。数据源装配见 `docs/plan/rag-data-source-abstraction-design.md`，门槛与长期参考见 `docs/plan/long-term/sparse-writes.md`。

## 0. 结论速览

* 向量侧要落地的不是"稀疏/多向量检索功能"，而是**能表达这些向量种类的契约形状 + 后端能力声明 + 不支持时的显式错误**。功能是否启用由上层后续决定，本轮只保证契约与后端"准备好的时候能安全接上"。
* 三个后端能力差异极大：**Qdrant 原生支持稀疏、多向量、具名向量**（仅用现有 REST 栈即可认领）；**simvec 只支持单稠密向量**，属于显式不支持；**内存后端**作为契约参考实现，先补稀疏倒排与多向量 MaxSim 的可选分支。
* Qdrant 侧无需引入 `qdrant_client` crate，也无需 gRPC：在现有 `reqwest` + 手写 JSON 的薄客户端上扩展建集合、写点、查询三处 body 即可。官方文档确认的字段形状见第 5 节。
* 现有三处"单稠密向量"假设必须同时改：`VectorPoint.vector`、`VectorQuery.vector`、`CollectionSpec.dimension`；再补 trait 的能力方法与 `VectorError` 的 `Unsupported` 变体。改造方式与 `lib.rs` 已声明的 "additive via optional fields" 一致，保持默认稠密行为完全不变。
* 未认领能力的后端必须返回明确的 `Unsupported` 错误，不允许用空结果或错误排序伪装支持。

## 1. 范围与定位

### 1.1 本轮做什么

* 收紧 `vector-contracts` 的向量数据形状，使其能表达稠密、稀疏、多向量三类载荷，并预留具名向量的寻址结构。
* 在 `VectorStorage` 上增加能力查询，让宿主在建集合前能判断某后端是否支持某种类。
* 让三个后端按真实能力认领或拒绝：内存后端实现参考语义，Qdrant 认领其原生能力，simvec 显式拒绝。
* 扩展 Qdrant 薄客户端的 REST/JSON 构造，使其具备写入和查询稀疏/多向量/具名向量的能力（即便本轮上层还不调用）。

### 1.2 本轮不做什么

* 不改 `wf-rag` 的 pipeline 查询分派、`vector-encoding` 插件桥接、编码器端口、source 指纹与 reindex、融合与 Weighted 归一化约束。这些属于后续"可选功能"阶段。
* 不为任何种类写消费分支、默认开关或自动降级；缺省路径与今天完全一致。
* 不改变集合的对外行为：现有以 `CollectionSpec::new(name, dim)` 建立的稠密集合必须零改动通过。

### 1.3 能力边界为何必须先做

当前契约把"一个点一个稠密向量"写死在类型里（`VectorPoint.vector` / `VectorQuery.vector` / `CollectionSpec.dimension`），任何后端想在将来支持其它种类，都绕不开这三处源头。把它们先扩展成可表达多种形状、且后端能声明能力的结构，是把"是否启用功能"从类型层解耦出来的前提。这样上层后续只需打开开关、选择种类，而不用再次改动契约与三个后端。

## 2. 后端能力调研结论

### 2.1 现状：单一稠密向量贯穿全链路

调研确认三处源头类型与三个后端实现都只有一个稠密向量：

| 位置 | 现状 |
| --- | --- |
| `types.rs` 的 `VectorPoint` | `vector: Vec<f32>`，单个稠密向量 |
| `store.rs` 的 `VectorQuery` | `vector: Vec<f32>`，查询为单个稠密向量 |
| `store.rs` 的 `CollectionSpec` | `dimension: usize` 全局唯一维度，无种类字段 |
| `memory.rs` | `StoredPoint.vector: Vec<f32>`，精确扫描按单一向量打分 |
| `simvec_adapter.rs` | 映射到 `simvec::VectorPoint::new(id, Vec<f32>)` |
| `qdrant.rs` | 建集合只写 `vectors: {size, distance}`，写点只写 `"vector": point.vector` |

`VectorStorage` trait 目前 13 个方法，没有任何能力探测方法；`VectorError` 也没有 `Unsupported` 变体，"做不到"目前被塞进 `InvalidInput`/`Backend`。`lib.rs` 的 crate 文档已把 sparse/multi vectors、named vectors、prefetch fusion 列为推迟项，并约定用可选字段加性扩展。

### 2.2 Qdrant：原生支持，适配缺口只剩薄层

Qdrant 服务端已原生支持全部三种能力，且当前后端使用的 REST 端口与手写 JSON 栈完全够用，无需新增依赖：

* **稀疏向量**：自 v1.7.0 起支持。集合用 `sparse_vectors` 声明（稀疏向量必须具名，可带 `modifier: "idf"`）；点的向量是 `{"indices":[u32],"values":[f32]}`；查询用 Query API 的 `query` + `using` 指向稀疏向量名。
* **多向量（late interaction / ColBERT）**：自 v1.10.0 起支持。稠密向量参数里带 `multivector_config: {"comparator":"max_sim"}`；点存的是矩阵 `[[f32],[f32],...]`；查询传矩阵，服务端算 MaxSim，返回**点级单一分数**；多向量要求关闭 HNSW（`m=0`）。
* **具名向量**：自 v1.2.0 起支持。`vectors` 用 `名称 -> 参数` 的 map 声明，同一记录可挂多个向量，查询必须 `using` 指定名称。
* 关键事实：现有后端其实**已经能读取** Qdrant 返回的 named-vector map（`parse_vector`/`parse_collection_vectors` 取 `default` 或首个值并压平），但创建和写入始终只产生一个默认稠密向量。具备的只是"单向兼容读取"，不是能力。

因此 Qdrant 侧属于 `sparse-writes.md` 门槛中"某后端原生支持，使执行矩阵缺口只剩适配层"的情形，是本次基础设施最适合真正认领的后端。

### 2.3 simvec：显式不支持

simvec 是纯 dense、单向量、单度量、单索引的本地引擎，作为 git submodule 固定在该仓 `crates/infra/simvec`（提交 `b64542fd…`）。全代码库（含 C API 与 5 个语言绑定、文档）都没有 sparse/multi-vector/ColBERT MaxSim/named vector 的实现或占位入口：

* `VectorPoint` 只有一个 `vector: Vec<f32>`，`CollectionConfig.vector_size` 全局唯一。
* 距离内核签名统一为 `(&[f32], &[f32])`，无稀疏表示、无双指针求交、无 MaxSim。
* 存储 `vectors.bin` 是 `dim*slots*4` 定长行；HNSW/IVF 都绑定单一 `dim`+`metric`。
* 引擎 API 与 C 头文件都没有能力探测方法。

要原生支持属于跨类型/内核/存储/索引/API/绑定六层的中大型改造，且会牵动 submodule 版本。本轮不做；simvec 适配器对非稠密种类一律返回显式 `Unsupported`，不做"展开成稠密"的近似伪装。

### 2.4 能力矩阵

| 后端 | 稠密 | 稀疏 | 多向量 | 具名向量 | 本轮认领 |
| --- | --- | --- | --- | --- | --- |
| memory | 支持 | 参考实现（可开关） | 参考实现（可开关） | 后续 | 稠密 + 稀疏/多向量参考语义 |
| simvec | 支持 | 不支持 | 不支持 | 不支持 | 仅稠密，其余显式 `Unsupported` |
| Qdrant | 支持 | 原生支持 | 原生支持 | 原生支持 | 全部四种（REST 适配） |

## 3. 契约层改动设计（`vector-contracts`）

改造遵循 `lib.rs` 已声明的加性扩展原则：所有新增字段用 `Option` 或带默认值的枚举，缺省即稠密，保持对象安全（`Arc<dyn VectorStorage>` 不变）。

### 3.1 向量种类与载荷形状

* 新增 `VectorKind` 枚举：`Dense`、`Sparse`、`Multi`。具名向量本轮只保留结构预留，不作为可认领种类。
* 新增 `VectorData` 枚举统一 write/query 的载荷：
  * `Dense(Vec<f32>)`：单个稠密向量。
  * `Sparse { indices: Vec<u32>, values: Vec<f32> }`：递增索引数组 + 有限权重数组，长度一致、索引非负。
  * `Multi(Vec<Vec<f32>>)`：定长上界内的向量序列，每个子向量维度一致。
* `VectorPoint` 的 `vector: Vec<f32>` 改为 `VectorData`（或等价的具名数据字段），保留 `id` 与 `payload` 不变；为兼容现有调用点，提供 `VectorPoint::new(id, Vec<f32>)` 构造稠密点的便捷入口，以及 `with_data` 构造其它种类。
* `ScoredPoint` 保持 `id`/`score`/`payload` 不变，沿用 higher-is-better 语义；稀疏后端可在 payload 放可选 explain 字段，但 explain 缺失不得影响召回与排序。

### 3.2 集合声明扩展

`CollectionSpec` 增加可选种类与种类参数：

* `kind: VectorKind`，缺省为 `Dense`。
* 稠密：继续要求 `dimension`、距离、量化配置。
* 稀疏：要求词表维度上限与索引非负规则；该上限用于内存参考实现与边界校验，Qdrant 侧不依赖固定词表维度。
* 多向量：要求单向量维度与每点向量数上限；距离继续复用 `DistanceMetric`。
* 校验按种类分别执行，声明与写入种类不一致即建集合/写入失败，不静默截断、不跨种类降级。

### 3.3 查询载荷扩展

`VectorQuery` 的 `vector: Vec<f32>` 改为 `VectorData`，查询载荷形状必须与集合种类一致；`VectorQuery::new` 继续返回稠密查询，新增按数据构造的入口。查询参数不跨种类复用：例如稠密的量化控制 `quantization` 与 `hnsw_ef` 出现在稀疏集合上应直接报错。其余结果整形字段（`limit`、`score_threshold`、`filter`、`exact`）保持不变。

### 3.4 能力查询与错误

* `VectorStorage` 增加能力查询方法（如 `supported_kinds(&self) -> Vec<VectorKind>` 或 `supports(&self, kind) -> bool`），返回后端实际支持的种类集合。该方法应与 `ensure_collection` 的拒绝行为一致，避免"声明支持但写入失败"。
* `VectorError` 新增 `Unsupported(String)` 变体，携带后端名与不支持的种类，错误文案要可读（例如"simvec does not support sparse vectors"）。
* 后端未认领的种类在建集合与写入两处都返回 `Unsupported`，不允许用空结果或错误排序假装支持。

### 3.5 校验规则（集中实现，三后端复用）

把当前散落在三个后端的合法性校验收敛为 `vector-contracts` 内的共享函数：

* 稠密：长度等于集合维度、元素有限（复用现有 `validate`）。
* 稀疏：索引非负且严格递增、索引与权重长度一致、权重有限、非空。
* 多向量：序列非空、每子向量维度等于单向量维度、点数不超过上限、元素有限。
* 建集合校验 `CollectionSpec` 的种类参数组合是否合法；查询校验种类匹配与参数不跨种类。

## 4. 后端实现策略

### 4.1 内存后端：契约参考实现

* 稠密路径完全保持现状。
* 稀疏：在 `CollectionData` 记录种类与词表上限；`StoredPoint` 保存稀疏结构；实现倒排召回（token 到 point 权重累加）与递增索引校验；`search` 按种类分派。作为其它后端的一致性参考，不追求生产规模。
* 多向量：保存向量序列，`search` 实现精确 MaxSim（查询向量逐条对每个文档向量取最大值再求和），返回点级分数。
* 具名向量本轮不认领，返回 `Unsupported`。
* 量化/HNSW 对稀疏与多向量不生效，仅做参数校验。

### 4.2 simvec 适配器：显式不支持

* 稠密路径完全保持现状，映射函数不改。
* `supported_kinds` 只返回 `Dense`；`ensure_collection`/`upsert`/`search` 遇到非稠密种类返回 `Unsupported("simvec …")`。
* 不改 submodule、不改 C API 与绑定。这样把 simvec 的大改造推迟到确有需求且有基准证据时，本轮零成本。

### 4.3 Qdrant 后端：按能力认领

* `supported_kinds` 返回 `Dense`、`Sparse`、`Multi`（具名向量作为寻址能力，本轮可一并返回或暂缓，见第 5 节）。
* 启动时做版本与能力探测：解析 `GET /` 返回的 server 版本，用现有 `is_compatible_version` 的"同 major、minor 差不超过 1"规则，再对所需种类做最低版本校验（稀疏 ≥ 1.7、多向量 ≥ 1.10）。能力不足时 fail fast，不降级成稠密查询。
* 三处 body 构造按种类分派（详见第 5 节）；`CachedCollection` 需扩充以记录种类与向量名，避免把稀疏/多向量集合误当稠密集合校验。
* ID 映射（`native_id` 的 UUID v5）与 `pid` payload 约定保持不变，三种种类共用同一套 id 与 `namespace/source` 约定。

## 5. Qdrant 客户端改造（基于官方 REST API）

当前后端是 `reqwest` + 手写 JSON 的薄客户端（`6333` REST 端口，gRPC `6334` 被显式拒绝），不依赖 `qdrant_client`，所有操作都是 `send`/`send_read` + 自建 body。扩展沿用这一方式，**不新增客户端 crate、不引入 gRPC**。以下为官方文档确认的字段形状，改造集中在建集合、写点、查询三处。

### 5.1 建集合：`collection_create_body`

现状只产出 `{"vectors":{"size":…,"distance":…}}`。按种类扩展为：

* 稀疏：`{"sparse_vectors":{"<name>":{"modifier":"idf"}}}`。稀疏向量在 Qdrant 必须具名，名称由集合配置提供；`modifier:"idf"` 用于让服务端在查询期计算逆文档频率（BM25 语义），是否启用由配置决定。
* 多向量：在稠密向量参数里加 `multivector_config: {"comparator":"max_sim"}`，并按官方建议把 HNSW 关掉（`hnsw_config.m=0`），避免建无用的图索引。
* 具名向量：`vectors` 改为 `名称 -> {size,distance}` 的 map；查询必须带名称。
* 稠密：保持今天的默认单向量形状不变。
* `ensure_collection` 读取远端配置时的校验要按种类解析 `vectors`/`sparse_vectors`，不能继续用只取 `default` 或首个值的压平逻辑去比对稀疏/多向量集合。

### 5.2 写点：`upsert`

现状把每点写成 `{"id":native_id,"vector":point.vector,"payload":{…,pid}}`。按载荷形状扩展 `vector` 字段：

* 稠密：保持数组 `Vec<f32>`。
* 稀疏：写成具名 map，值为 `{"indices":[...],"values":[...]}`；`indices` 与 `values` 必须等长，Qdrant 内部会排序但调用方应保证索引唯一。
* 多向量：写成矩阵 `[[…],[…],…]`，每个子向量维度等于声明的 size。
* 具名向量：`vector` 写成 `名称 -> 向量` 的 map，可在同一点携带多个向量。
* `pid` 字段与分批、`wait=true` 等逻辑不变；写入前按 3.5 校验整批，任一点非法则整批失败。

### 5.3 查询：`search_body` 与 Query API

现状用 `POST /collections/{collection}/points/query`，body 为 `{"query":vec,"limit":…,"with_payload":true}`（含可选 filter 与召回参数）。按种类扩展：

* 稠密：保持现状。
* 稀疏：`query` 写稀疏对象 `{"indices":[...],"values":[...]}`，并加 `using:"<sparse-name>"` 指向稀疏向量。
* 多向量：`query` 写查询矩阵，服务端算 MaxSim；关闭 HNSW 的集合走精确比较。
* 具名向量：用 `using` 指定名称；若要稠密+稀疏同时召回，用 Query API 的 `prefetch` 提交多路子查询，顶层用 `{"fusion":"rrf"}` 融合（RRF/Concat 不依赖原始分数，可安全跨种类）。Weighted 需可比归一化分数，本轮不接入。
* 过滤条件 `filter`、`score_threshold` 等继续复用现有 `build_filter`；`exact`/`hnsw_ef`/量化参数仅在稠密路径下发，非稠密路径携带这些参数应报错。
* 结果解析继续优先用 payload 的 `pid` 还原 contracts id；Euclid 分数用现有 `normalize_score` 统一为 higher-is-better。

### 5.4 版本与能力探测

* `GET /` 取 server 版本（复用 `version()`），用 `is_compatible_version` 判断主版本兼容，再按种类做最低版本门禁。
* 探测结果可在 `QdrantVectorStore::new` 之后的初始化阶段执行（当前构造不发请求，需新增一个显式探测入口或由宿主调用 `check_compatibility`），能力不足返回可读错误并 fail fast。
* 探测是"够不够用"的开关，不是把不支持种类偷偷降级。

## 6. 测试与验证

* **契约对齐测试**：三种形状各自的合法性用例（稠密维度、稀疏索引递增/等长/有限、多向量维度与上限）；种类不匹配的建集合/写入/查询必须返回明确错误；跨种类参数（稠密量化用于稀疏）被拒绝。
* **能力矩阵测试**：`supported_kinds` 与三后端 `ensure_collection` 的拒绝行为一致；simvec 对非稠密种类返回 `Unsupported` 且文案含后端名与种类。
* **默认兼容测试**：现有全部以 `CollectionSpec::new(name, dim)` 建立的稠密集合、写点、查询、过滤、分页、删除、计数测试零改动通过。
* **Qdrant REST body 测试**：为稀疏/多向量/具名向量各加 body 构造单测，断言 `sparse_vectors`、`multivector_config`、`using`、`prefetch`、`fusion` 等字段形状；沿用现有 `search_body_carries_optional_recall_params` 的风格，用 mock server 断言请求体，不依赖真实 Qdrant。
* **跨后端一致性**：稀疏/多向量的过滤、分页、删除、计数、分数顺序在内存与 Qdrant 之间对齐；近似计数仍按现有契约说明处理，不用于精确一致性断言。

## 7. 实施顺序

1. 收紧契约形状：`VectorKind`/`VectorData`、`CollectionSpec`、`VectorQuery` 改为可选字段 + 默认值，保证稠密路径零改动。
2. 收敛共享校验函数，新增 `VectorError::Unsupported` 与 `VectorStorage` 能力方法。
3. 三个后端接入能力方法：simvec 仅稠密；内存补稀疏/多向量参考语义；Qdrant 声明全部能力。
4. 扩展 Qdrant 薄客户端三处 body 构造与版本门禁，加 REST body 单测。
5. 跑通默认兼容与契约对齐测试，确认稠密行为与改前一致。

## 8. 不做事项

* 不改 `wf-rag` 的 pipeline 分派、编码器端口、`vector-encoding` 插件桥接、source 指纹、reindex、融合与 Weighted 归一化。
* 不打开任何种类的默认开关，不为假想需求写消费分支；本轮只交付"能安全接上"的基础设施。
* 不改 simvec submodule、C API 与语言绑定；simvec 只做显式拒绝。
* 不引入 `qdrant_client` crate 或 gRPC 传输；Qdrant 侧继续走共享 `reqwest` 栈。
* 不做排序、recommend、discover、分组、快照、分片、别名、gRPC，以及旧向量格式兼容或自动迁移。
