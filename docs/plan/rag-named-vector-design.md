# RAG 具名向量接入设计

> 本文是 `docs/plan/long-term/rag-vector-kind-extension-design.md` 第五步的单独设计方案，只讨论具名向量作为向量寻址能力的接入。它不启动实现，也不改变当前稠密、稀疏、多向量路径。触发条件是出现真实的混合集合需求：同一记录需要同时保存标题与正文向量，或需要在一条记录上按名称选择不同的编码结果。simvec 子仓库的 `e33b146` 已经提供 `NamedStore`，本文据此分析其不足并给出补齐方案。

## 0. 结论速览

* 具名向量不是第四种检索算法，也不是 `VectorKind` 的新变体；它是集合内向量槽位的命名方式。集合声明名称集合，点携带名称到向量数据的映射，查询必须指定一个名称。
* simvec 已经有可用的 `NamedStore`，但引擎边界没有复用 `CollectionConfig::validate_kind()`，能力探测无法表达具名布局，配置回读会丢失单空间字段，查询侧没有拒绝稠密专用的 recall 参数。这些是接入前必须补齐的缺口。
* 向量契约当前只有 `CollectionSpec::vector_name` 这个“单槽位名称”，无法表达名称到向量的映射。`vector_name` 继续用于 Qdrant 单稀疏槽位；具名布局需要独立的空间声明、点载荷和查询目标字段。
* 第一版只支持每个空间为 Dense 或 Sparse。simvec 的 `validate_kind()` 已拒绝 Multi 空间，但创建路径尚未调用它，需要补齐后才是真正的边界约束。每个点必须携带全部声明空间，部分更新按非法输入拒绝，直到有真实需求再定义合并语义。
* 存储层继续做单空间精确扫描。跨空间召回与融合属于 `wf-rag`，不塞进 `VectorStorage`；存储层遇到一次查询多个名称时直接报错，不静默只查第一个名称。

## 1. 现状与能力清单

simvec 子仓库当前 HEAD 为 `e33b146`。与具名向量相关的实现集中在 `src/named_store.rs`，并由 `LocalVectorEngine` 统一路由。

| 位置 | 已具备的能力 |
| --- | --- |
| `CollectionConfig` | `vectors: VectorsConfig::Named` 声明名称到 `VectorSpaceConfig` 的映射；每个空间有自己的 `kind`、`vector_size`、`distance` |
| `VectorPoint` | `named: BTreeMap<String, VectorData>` 保存一个点的全部具名向量 |
| `SearchQuery` | `target` 指定空间名，`named_vector` 携带该空间的查询向量 |
| `NamedStore` | WAL 持久化、重开回放、按空间倒排的稀疏召回、稠密全扫描、MaxSim 以外的点级打分、过滤、分页、删除、payload 更新 |
| `LocalVectorEngine` | 按集合元数据把增删改查、计数、滚动路由到 `NamedStore`；`named_store()` 返回具体句柄 |
| 打分语义 | Dense 与 Sparse 都遵循全局 higher-is-better；Sparse 复用 `kind_store::sparse_score`，Dense 用共享距离内核重映射 |

这些能力足以做本地参考实现，但还不能直接当作稳定的后端契约来消费。

## 2. 当前不足

### 2.1 创建路径没有执行配置校验

`CollectionConfig::validate_kind()` 已经包含具名布局的校验：空间集合非空、名称非空、空间种类不是 Multi、空间维度为正、不能使用 ANN 索引或量化。但 `LocalVectorEngine::create_collection()` 的具名分支在创建 `NamedStore` 前没有调用它，而是把索引和量化字段直接改写为默认值。

后果是空空间集合、空名称、零维度空间、Multi 空间等非法配置可能被持久化；调用方配置的 ANN 与量化参数被静默丢弃，而不是得到明确错误。这会让宿主在配置阶段无法发现错误，到写入或查询时才出现难懂的形状错误。

修复点应放在引擎创建边界：`create_collection()` 对具名布局先执行完整校验，再创建存储；`NamedStore::create()` 自身也应校验一次，保证直接调用存储的实现不会绕过约束。ANN 与量化必须报错，不能静默改写。

### 2.2 能力探测无法表达具名布局

`VectorEngine::supported_kinds()` 返回 `Dense`、`Sparse`、`Multi`，没有“是否支持具名空间”的位置。具名布局通过 `VectorsConfig::Named` 表达，但它属于布局而不是种类。宿主目前只能先尝试创建集合或读取配置来猜测能力，无法在启动或建集合前做 fail-fast 门禁。

需要在引擎侧增加独立的具名布局能力查询，并保持它与 `create_collection` 实际接受的配置一致。适配层再把它映射到 `VectorStorage` 的能力方法。不要用 `VectorKind::Named` 表达这件事，否则会把寻址方式和打分算法混在一起。

### 2.3 配置与集合信息回读有占位值

`LocalVectorEngine::collection_config()` 对具名集合返回 `CollectionConfig::new(0, Cosine)`，只填入 `vectors: Named` 和 `index: Flat`。`vector_size`、`distance`、`kind` 都是单空间字段，对具名集合没有意义，但调用方拿到的是看似合法的 `0` 和 `Cosine`。`collection_info()` 同样给出零内存、零分段的占位统计。

这会让通用校验代码误把具名集合当成“零维稠密集合”，导致维度检查、距离比较和观测都失真。需要提供具名专用的配置读取形态，或明确约定：只要 `vectors` 是 Named，单空间字段一律不得被消费；所有参数只能从空间映射中读取。回读时也必须保留每个空间的真实距离与维度。

### 2.4 查询侧参数拒绝不完整

`NamedStore::search()` 已经拒绝 `quantization` 和 `nprobe`，但没有拒绝 `SearchMode::KNN`。具名空间全部走精确扫描，`search_mode.ef_search` 一类稠密 recall 参数不会生效。静默忽略会让调用方误以为调高了召回宽度。

修复原则是：对具名集合，任何只适用于单空间稠密 ANN 或量化的参数都必须显式报错，包括 `search_mode`、`quantization`、`nprobe`，以及适配层传到引擎的 `exact`。错误文案应带空间名或参数名，便于定位。

### 2.5 部分空间更新语义不统一

`NamedStore::apply_upsert()` 要求一个点携带全部声明空间，少一个空间就拒绝；而 Qdrant 的具名向量允许只写部分名称。如果宿主把“部分更新”当作正常语义，两个后端会给出不同结果。

第一版应在契约层固定为“整点替换、空间集合必须完整”，并要求所有后端对缺空间写入返回明确的非法输入错误。这样行为可预测，也符合 simvec 现有 WAL 记录语义。若以后出现只更新标题向量的真实需求，再单独设计部分合并语义、缺失空间保留规则和对应的 WAL 记录，不能靠适配器偷偷补默认值。

### 2.6 错误分类不够可读

`NamedStore::require_slot()` 对不存在的点返回 `Internal`，而契约层期望这类错误映射为非法输入或集合缺失。未知空间名、空间种类不匹配等错误虽然可读，但都集中在引擎的 `InvalidConfig` 里。适配层需要做显式映射，才能与稠密、稀疏、多向量路径保持一致的错误码和文案。

### 2.7 存储规模与外部接口边界

`NamedStore` 与 `KindStore` 一样是内存精确扫描、WAL 全量回放的实现，没有 ANN 索引、量化或 Compaction。它适合契约对齐、开发和中小规模本地集合，不应被描述成生产级大规模索引。

C API 与语言绑定中没有具名向量的入口，当前能力只在 Rust API 上可用。设计中不要把“引擎 Rust API 支持”扩展成“全语言绑定支持”；若以后需要 C 接口，应单独立项。

## 3. 契约设计

### 3.1 类型形状

在 `vector-contracts` 增加具名布局描述，并保持已有稠密路径不变：

* 新增 `VectorSpaceSpec`，描述一个空间的 `kind`、`vector_size` 和 `distance`。空间种类只允许 Dense 与 Sparse。
* `CollectionSpec` 增加 `spaces: Option<BTreeMap<String, VectorSpaceSpec>>`。当 `spaces` 有值时，集合是具名布局；`dimension`、`distance`、`quantization`、`hnsw` 等单空间字段不得再被解释。`vector_name` 继续只表示单槽位名称，不能与 `spaces` 同时设置。
* `VectorPoint` 增加 `named: Option<BTreeMap<String, VectorData>>`。具名点必须携带与集合声明完全一致的键集合，且每个值与对应空间的种类和维度一致。
* `VectorQuery` 增加 `target_space: Option<String>` 与 `named_vector: Option<VectorData>`。具名查询必须同时设置二者；目标名称必须已声明，查询向量种类必须匹配该空间。
* `VectorStorage` 增加 `supports_named_spaces()`，默认 `false`。宿主在建集合前用它做能力门禁。

`VectorKind` 不变；具名集合的总体 `kind` 保持 `Dense` 默认值，仅作为单空间字段的占位值。所有种类判断都以 `spaces` 中的空间声明为准。

### 3.2 校验规则

契约层统一执行以下规则，后端只做引擎侧二次校验：

* 空间名非空且唯一；空间集合非空。
* 每个空间维度为正；Sparse 空间的 `vector_size` 表示词表上界。
* Multi 空间不支持；量化与 HNSW 参数在具名集合上直接报错。
* 写入时每个点的空间键集合必须与声明完全一致；空映射按非法输入处理。
* 查询必须指定一个已声明名称；未声明名称、种类不符、维度不符都返回非法输入。
* `exact`、`hnsw_ef`、`quantization` 等稠密控制参数在具名查询中一律拒绝。

## 4. 后端改造

### 4.1 simvec 引擎

* 在具名创建路径调用 `validate_kind()`，让非法配置在落盘前失败。
* 增加独立的具名布局能力查询，返回宿主可读的能力状态；`supported_kinds()` 不变。
* 具名集合的 `collection_config()` 与 `collection_info()` 返回可辨识的具名布局，不再给出会误导单空间校验的占位维度。具体做法是让通用字段保持显式空缺语义，宿主必须读取空间映射。
* `NamedStore::search()` 补齐 `search_mode` 等稠密参数的显式拒绝。
* `require_slot()` 与未知空间错误改为更贴近契约的语义，由适配层统一映射。

### 4.2 simvec 适配层

* `CollectionSpec.spaces` 映射到 `simvec::CollectionConfig::with_named()`。
* `VectorPoint.named` 映射到 `simvec::VectorPoint::with_named()` 与 `with_named_vector()`。
* `VectorQuery.target_space` / `named_vector` 映射到 `simvec::SearchQuery::with_named()`。
* `fetch()` 与 `scroll()` 必须保留具名映射；当前适配器把非具名点映射回 `VectorData` 的路径不能吞掉 `named`。
* 能力查询委托引擎，而不是在适配器内硬编码 `true`。

### 4.3 Qdrant

Qdrant 原生支持具名向量，但它把 Dense 与 Sparse 分别放在 `vectors` 与 `sparse_vectors` 两个集合字段中。适配器需要：

* 建集合时把 `spaces` 拆成两个 REST 字段；同名空间不能同时出现在两类字段中。
* 写点时把具名映射拆成 Qdrant 的向量对象；查询时把 `target_space` 映射为 `using`。
* 读取集合信息时合并两个字段，还原为统一的 `spaces` 映射，而不是像现在这样只取第一个名称并压平成单个 `vector_name`。
* 沿用现有版本与能力探测；具名能力不足时 fail fast，不退回默认稠密查询。

### 4.4 内存后端

第一版内存后端对具名布局返回明确的 unsupported。它已经有单空间的 Sparse/Multi 参考语义，但没有必要为一个 postponed 的寻址能力复制一套映射存储。若将来需要契约参考实现，再在同一模块中按相同规则补齐。

## 5. 宿主路径

### 5.1 编码

现有 `vector-encoding/v1` 只输出单个 `VectorData`。具名集合需要单独的具名输出 schema：插件返回名称到编码结果的映射，并声明整体 schema 与每个空间使用的编码器。编码仍是同步纯函数，禁止网络访问和索引写入。

摄取前按 schema 校验整批输出：名称集合、每个空间种类、维度、有限数值全部通过后才能写向量；任一点非法则整批失败。

### 5.2 配置与指纹

`RagIndexConfig` 增加具名空间声明；source 指纹必须包含空间名、种类、维度和距离。指纹变化时旧向量失效，普通增量同步要求显式 reindex，不能自动兼容或猜权重。

### 5.3 查询与融合

具名查询每次只命中一个空间。若业务需要标题空间与正文空间同时召回，由 `wf-rag` 分别发起两次查询并把候选列表送入现有融合；存储层不提供“一次查询多个名称”的隐式行为。跨空间分数不可直接相加，Weighted 只在显式声明分数可比时启用。

检索指标增加目标空间名与具名 schema，沿用 latency、candidates、hits 字段。

## 6. 验证

* **配置校验**：空空间、空名称、零维度、Multi 空间、ANN、量化、单空间字段与具名布局混用全部在建集合前失败。
* **能力矩阵**：simvec 与 Qdrant 声明具名能力，内存显式不支持；宿主在无能力后端上得到可读错误。
* **写入与查询**：整点写入、缺空间写入、未知目标空间、种类/维度不匹配、payload 过滤、分页、删除、计数、重开回放。
* **跨后端一致性**：同一组具名集合在 simvec 与 Qdrant 上返回相同点集与相同顺序；分数只比较排序，不比较绝对值。
* **错误映射**：未知点、未知空间、跨种类查询参数在三个后端都映射到同一组契约错误。
* **宿主路径**：具名编码 schema 拒绝用例、指纹变化后的 reindex 要求、跨空间候选列表进入融合但不直接相加分数。

## 7. 实施顺序

1. 先在 `vector-contracts` 固定具名类型与校验规则，明确 `vector_name` 与 `spaces` 的职责边界。
2. 修 simvec 引擎边界：配置校验、能力探测、配置回读、查询参数拒绝与错误语义。
3. 接入 simvec 适配层并补契约对齐测试，再接入 Qdrant 的名称拆分与合并。
4. 最后接入 `wf-rag` 的具名编码 schema、source 指纹、查询分派与融合约束。
5. 只有出现真实混合集合需求并完成后，才把具名布局作为可选功能开放给配置；本文不随当前稀疏/多向量实现一起上线。

## 8. 不做事项

* 不把具名向量实现为 `VectorKind::Named` 或第四种打分算法。
* 不在 `VectorStorage` 内实现一次查询多个空间的隐式融合。
* 不支持 Multi 空间、ANN 索引、量化、部分空间更新与 C API 绑定。
* 不改当前稠密、稀疏、多向量集合的默认行为，不引入旧向量格式兼容或自动迁移。
* 不把 simvec 的精确扫描实现描述为生产级大规模索引能力。
