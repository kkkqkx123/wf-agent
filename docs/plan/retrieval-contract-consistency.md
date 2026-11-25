# 检索契约语义一致性修复

## 范围

本次只处理 `vector-contracts`、`keyword-contracts` 与其直接消费者 `rag-core` 中已经影响查询正确性的契约问题。包拆分、编译裁剪和错误类型重构不在本次范围，见文末说明。

## 问题清单

| 问题 | 证据 | 结论 |
|---|---|---|
| 关键词后端没有兑现共享 `SearchFilter` 语义 | `sqlite_index`、`tantivy_index`、`elastic_index` 的 `count` 只使用 collection、namespace、source；`fields`、`conditions`、`should`、`min_should`、`has_ids` 被忽略 | 契约承诺大于实现能力，需要收窄关键词契约 |
| 关键词搜索的候选截断破坏 top-k | 三个关键词后端先取 `max(limit * 3, 10)` 个候选，再在 Rust 中按完整过滤器筛掉不合格命中 | 过滤后可能不足 limit，且真正满足条件的后续命中没有机会进入结果 |
| 向量过滤的数组语义在后端间不一致 | 内存后端把数组视为无文本值；simvec 按数组元素相等匹配；Qdrant 的 `match` 也是元素匹配 | Equal/NotEqual/Contains 的数组行为需要统一为“任一元素匹配” |
| Qdrant 的 `IsNull` 精确回退有固定上限 | 搜索时把 limit 抬到至少 10000 后本地过滤；count 走分页 | 超过截断范围会漏命中，搜索与计数应使用同一分页精确路径 |
| Qdrant 可配置近似计数但 trait 未声明 | `exact_count = false` 时 count 返回近似值，trait 文档仍然写“统计匹配的点” | 需要把近似能力写入契约说明，避免调用方误用 |

## 修改方案

### 阶段一：收窄关键词查询契约

在 `keyword-contracts` 中新增独立的 `KeywordScope`，只表达后端能够原生执行的作用域：namespace 与 source。`KeywordQuery` 使用 `KeywordScope` 替代 `SearchFilter`，`KeywordIndex::count` 同步改为按 scope 计数。这样关键词契约不再承诺自己无法保证的通用元数据过滤语义。

`rag-core` 的适配层承担通用过滤职责：`keyword_chunks` 接收 RAG 侧的完整 `SearchFilter`，基于关键词命中返回的 payload 做精确匹配后再转换。混合检索先用 scope 查询关键词后端，再在适配层应用配置中的 fields 与 conditions。候选放大只发生在 RAG 适配层，后端不再做固定倍数的隐式截断。

三个关键词后端删除“取放大候选后本地再过滤”的逻辑，直接使用查询 limit；counts 直接使用 scope。相关单测改为验证 scope 语义，并补充适配层的通用过滤测试。

### 阶段二：统一向量数组过滤与 Qdrant 精确回退

在 `SearchFilter` 的共享语义中，数组字段的 Equal、NotEqual、Contains 统一为“任一元素匹配”，并保持标量语义不变。内存后端按该语义实现；simvec 的现有元素匹配与之对齐。

Qdrant 把 `IsNull` 与 `Contains` 标记为需要精确复核的条件。搜索和计数统一走分页 scroll：逐页取回点，按共享过滤器精确判断，再按统一分数排序和截断。这样移除搜索侧固定 10000 的上限，并与计数路径保持一致。同时在 `VectorStorage::count` 文档中明确后端可在配置下返回近似计数。

### 阶段三：远端验证

按 `docs/dev/devs同步与远程编译指南.md` 打包同步到 devs，使用远端 `check.sh` 做一次完整编译检查；再在远端运行相关 crate 的定向测试。编译与测试日志取回本地归档，不回传编译产物。

## 执行结果

- 阶段一与阶段二代码修改已完成；`keyword-contracts` 查询契约收窄为 `KeywordScope`，`rag-core` 适配层负责通用条件复核，Qdrant 对近似操作符改为分页精确复核。
- 编译与测试全部在 devs 执行：`cargo check --locked --workspace --all-targets --all-features` 通过；`vector-contracts`、`keyword-contracts`、`rag-core --all-features`、`wf-api` 测试通过。
- 远端日志保存在 `~/devs-logs/check-7a30d0ac.log` 与 `~/devs-logs/verify-final-7a30d0ac.log`，编译产物未回传。

## 不在本次范围

- `vector-contracts` 同时包含契约与后端实现、`keyword-contracts` 后端仍留在 `rag-core`：这是依赖方向上的权衡，改动会波及宿主装配，单独处理。
- `KeywordFormat` 依赖 `wf-types` 配置枚举且携带打开策略：属于格式策略边界问题，不影响查询正确性。
- `keyword-sqlite` 与 `vector` 空 feature：属于编译裁剪问题，需要配合宿主 feature 链路单独改造。
- `KeywordError` 与 `VectorError` 的错误粒度差异：属于可观测性改进，不影响本次语义修复。
