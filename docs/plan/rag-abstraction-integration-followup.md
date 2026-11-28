# RAG 抽象收尾：集成缺口与遗留清理问题清单及修改方案

> 背景：仓库已有四包 RAG 分层、实例化的扩展注册表与统一调用句柄骨架。静态核对确认《RAG 抽象与插件扩展点复核：实现轮问题清单与修改方案》中"存储搬迁、注册形态收敛、死点删除"三线已落地，但"实例贯通、调用抽象上移、补全端口"三线仍有断点，导致插件扩展在部分链路不生效、问答能力无法在宿主装配。
> 范围：`wf-api`（上下文装配、`RagHandles`、能力配置）、`wf-workflow`（检索节点）、`wf-tools`（工具后端）、`wf-types`（声明式配置）、`wf-rag`（调用句柄形态与补全端口）。HTTP、CLI、前端与工作流节点 schema 的形态不在本轮改动目标内。
> 方式：只改原模块，不建替代模块；宿主注入端口，门面保持无状态；不做旧数据兼容。代码与注释保持英文，文档保持中文。

## 0. 结论速览

* 存储契约对称与插件死点清理已达成目标形态，无需再动。
* 剩余断点集中在三处：工作流节点未拿到宿主扩展实例、节点仍直调自由函数、生产链路没有任何补全端口实现。
* 本轮按"实例贯通、句柄上移、补全端口接通、配置暴露、遗留清理"五阶段推进，每阶段以真实调用链可达且编译通过为完成标准。
* `rag-advisory-demo` 经核实为有意脱离父 workspace 的独立 cdylib 示例，非遗留代码，不清理。

## 1. 问题清单

### 1.1 集成断点

1. 工作流节点的扩展实例未贯通。`ApiContext::rag_node_handles()` 由 `RagNodeHandles::new()` 构造句柄，而 `new()` 内部新建空 `RetrievalExtensions`。宿主在 bootstrap 装配好的共享实例没有注入节点，导致插件注册的 analyzer、后处理器、路由选择器在 RAG 节点上全部不生效，而工具与 API 侧却能看到。同一配置下两类消费方行为不一致。
2. 工作流节点仍直调自由函数。`handler/rag.rs` 的查询路径直接调用 `pipeline::query` / `query_multi` / `query_hybrid`，没有走已建好的 `StandardRetriever` 句柄；工具侧 `RagService` 已改用句柄。统一调用抽象未向上传导到引擎。
3. 生产链路没有补全端口实现。仓库所有 `impl CompletionLlm` 都位于测试模块，`synthesize`、`StandardQa`、`answer_with_*`、HyDE、分解、反馈改写只在单测里被调用。宿主装配的 `RagHandles` 只持有 store、embedder、keyword、extensions 四个句柄，没有补全能力，问答链在生产中不可达。
4. `CompletionLlm` 端口仍以 `complete(&str)` 为必需方法，`complete_request` 只有默认实现。结构化请求虽已定义，但未成为端口唯一入口。

### 1.2 调用抽象形态缺口

1. `StandardRetriever` 覆盖 `retrieve` / `retrieve_multi` / `retrieve_hybrid` / `retrieve_routed`，但没有带指标返回的形态，而自由函数 `query_with_metrics` 已存在。
2. `StandardQa` 只有变体重写一条 `answer` 路径，HyDE 与子问题分解仅有自由函数，没有句柄形态。
3. 声明式配置 `RagCapabilitiesConfig` 没有问答开关、补全 profile 与提示词模板字段，宿主无法从配置调优提示词。

### 1.3 遗留清理

1. `wf-api/Cargo.toml` 保留空特性 `keyword = []`，不转发任何契约侧特性，为搬迁期遗留；同时 wf-api 内若干 keyword 打开与探测调用依赖特性门控却无对应特性声明，导致该 crate 单独编译时失败。
2. 门面保留 `probe_elastic_keyword`，它是 elastic 打开前的版本探测 helper，仍被宿主真实调用，属于有效能力而非转发入口，本轮保留。

## 2. 修改方案

### 2.1 阶段一：实例贯通修复

* `ApiContext::rag_node_handles()` 在构造句柄后显式注入宿主的共享 `RetrievalExtensions`，使节点、工具、API 三类消费方持有同一个实例。
* `RagNodeHandles::new()` 的空实例保留为独立构造路径的默认值，宿主装配路径必须覆盖。
* 完成标准： bootstrap 装配的节点与工具持有同一实例；插件注册的 analyzer 在节点查询路径可见。

### 2.2 阶段二：检索句柄上移

* `handler/rag.rs` 的查询分支改为构造 `StandardRetriever` 并调用 `retrieve` / `retrieve_multi` / `retrieve_hybrid`。
* 节点的 config 覆盖（top_k、分数阈值、字段、融合、候选放大）在构造句柄前完成，保持与现有自由函数调用完全一致的行为。
* 自由函数保留为句柄默认实现，不删除。
* 完成标准：节点查询路径不再直接调用 `pipeline::query*` 系列；节点测试保持通过。

### 2.3 阶段三：补全端口与问答句柄生产接通

* 在宿主侧新增 `CompletionLlm` 适配器，基于共享 `LlmGateway` 与声明的补全 profile，把结构化请求转换为一次文本生成调用，取回文本内容并按请求约束截断。适配器放在宿主 crate，遵守依赖 DAG，不让 `wf-rag` 依赖 `wf-llm`。
* `CompletionLlm` 端口收敛：`complete_request` 成为必需方法，`complete(&str)` 改为基于结构化请求的默认实现，测试桩改为实现结构化方法。
* `RagHandles` 增加可选补全句柄与问答配置字段，由 bootstrap 在问答能力开启时装配；`RagNodeHandles` 相应可透传补全句柄。
* 在宿主提供基于 `StandardQa` 的问答入口，使 `synthesize` 链路真实可达；`StandardQa` 补齐 HyDE 与子问题分解形态，`StandardRetriever` 补齐带指标检索形态。
* 完成标准：宿主可装配生产补全端口；问答入口经 `StandardQa` 执行到合成；`complete_request` 为端口唯一必需方法。

### 2.4 阶段四：配置暴露

* 声明式配置新增问答开关、补全 profile 标识与提示词模板字段；模板以纯字符串结构承载，映射为门面的 `PromptTemplates`，避免底层 crate 依赖倒置。
* `RagCapabilitySettings` 承接新配置，并在装配问答句柄时传入。
* 完成标准：宿主可通过配置开启问答、指定 profile 与覆盖提示词模板；数值与名称校验覆盖新增字段。

### 2.5 阶段五：遗留清理

* 删除 `wf-api/Cargo.toml` 的空特性 `keyword = []`，并清理 wf-runtime 与 wf-server 中的同名纯转发链及其条件编译块。
* 以有真实转发内容的 `keyword-tantivy` / `keyword-elastic` 特性替代原空洞开关，向契约包与门面转发后端特性，恢复该 crate 单独编译能力。
* 核对门面重导出与探测函数，确认无不可达声明入口残留。
* 完成标准：全特性编译通过；无空特性转发；无声明后无消费的注册入口。

### 2.6 阶段六：子模块依赖与格式收尾

* `simvec` 子模块的 `crc32fast` 仅用于测试代码构造遗留 CRC32 信封（生产路径使用自研 CRC32C），把它从 `[dependencies]` 移到 `[dev-dependencies]`，消除全特性编译时的未使用依赖告警，且不改变测试语义。
* 规范化 `wf-rag` 中两处既有格式差异（`probe_elastic_keyword` 签名折叠、`rebuild.rs` 导入顺序），使全工作区 `cargo fmt --check` 通过。
* 完成标准：全工作区编译无 `crc32fast` 未使用告警；`cargo fmt --all -- --check` 无差异。

## 3. 实施顺序与依赖

1. 阶段一与阶段二先做，风险低且能立即消除实例不一致。
2. 阶段三依赖阶段二的句柄形态，并在阶段四配置就绪前用默认配置接通。
3. 阶段四在阶段三之后，把硬编码装配参数改为配置驱动。
4. 阶段五最后做，避免清理影响仍在迁移的装配代码。

## 4. 验证范围

* 每阶段对受影响 crate 执行编译检查，最终对工作区执行全特性编译检查与格式检查；子模块 `simvec` 单独验证测试代码编译。
* 定向验证：未知名字早失败、执行失败降级、实例共享、节点与工具行为一致、问答零命中短路、提示词模板覆盖。
* 远端服务在线测试仍按环境变量门控，无环境时跳过不作为通过依据。

## 5. 不做事项

* 不改变向量与关键词的融合算法、重排适配、引用渲染与评测语义。
* 不新增图谱、多模态、摄取缓存与后台任务。
* 不改动 HTTP、CLI、前端与工作流节点的对外 schema。
* 不为旧插件名字或旧索引做兼容。
