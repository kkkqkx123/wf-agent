# wf-server 依赖与运行时内存占用分析

本文基于 devs 上的 release 实测，分析 `wf-server` 的依赖构成、空闲态内存去向，以及面向嵌入式部署可裁剪的功能。测试数据来自当前工作树（含尚未提交的关键词索引加固改动），不回传编译产物。

## 结论速览

* 默认 release 二进制 151 MiB，strip 后 110 MiB；`--no-default-features` 后 121 MiB，strip 后 89 MiB。编译期依赖从 482 个降到 401 个。
* 空闲态（已响应一次 `/health`，内存存储，devs 32 核）默认进程 RSS 约 38.7 MiB，其中匿名内存约 9.3 MiB、文件映射约 29.4 MiB；无插件构建约 34 MiB。
* 进程虚拟地址空间约 2.3 GiB，实测确认是 glibc 每线程 arena 预留：34 个线程 × 64 MiB `PROT_NONE`。设置 `MALLOC_ARENA_MAX=2` 后降到约 265 MiB，RSS 不变。
* 空闲态大部分 RSS 是启动时触碰的代码段页，并非活跃数据结构。运行时真正的增长点是执行历史与并发预算、存储实体缓存、文件检查点快照、RAG 向量/关键词索引、PTY 输出缓冲，以及带标签的指标与限流表。
* 嵌入式建议档位：`--no-default-features`（或仅加 `plugins,lua`）+ strip + 显式限制 Tokio worker 线程数 + 关闭关键词索引 + 收紧并发与缓存。若不需要插件，wasmtime/Cranelift/LuaJIT 全部不进镜像。

## 依赖构成

`crates/app/wf-server/Cargo.toml` 有 27 个直接依赖；经 `wf-runtime`、`wf-api` 展开后，默认构建共 482 个 crate，无插件构建 401 个，仅 Lua 插件构建 411 个。按职责分组：

| 分组 | 主要依赖 | 来源与说明 |
|---|---|---|
| HTTP 传输 | axum 0.8、tower/tower-http、tokio、http、futures、tokio-util、percent-encoding | server 直接依赖；约 584 处路由注册，669 条 OpenAPI 路径描述 |
| 序列化与工具库 | serde/serde_json、utoipa、clap、base64、hex、sha2、blake3、thiserror、tracing | server 直接依赖；utoipa 的 schema 代码默认参与编译，`openapi-docs` 只控制路由是否暴露 |
| 存储 | sqlx（同时启用 sqlite 与 postgres）、moka、dashmap、wf-storage、rusqlite（bundled）、zstd | `wf-runtime` → `wf-storage`；24 个实体存储，SQLite 模式每个包一层 1000 条/300 秒的 moka 缓存 |
| 检查点与 Git | checkpoint-base/state/file、gitoxide 系列（gix-*）、rusqlite、zstd | `wf-checkpoint` → `checkpoint-file`，当前无条件编译，即使文件检查点默认关闭 |
| RAG 与向量 | rag-core、jieba-rs（约 5 MB 词典）、rusqlite FTS5、vector-contracts、llm-suite-embedding/rerank | `wf-api`/`wf-runtime` 无条件依赖；tantivy 与 elastic 后端是可选 feature，当前未开启 |
| LLM 栈 | wf-llm、wf-pricing、wf-llm-catalog、llm-suite-client/auth/gateway/discovery/codec 等约 20 个 crate、reqwest 0.13 + rustls/aws-lc | 默认配置只使用内置 provider 与静态表，但整栈始终链接 |
| 工具与脚本 | wf-tools、rmcp（子进程与 HTTP MCP 客户端）、wf-script、wf-shell、portable-pty、notify、wf-integration/code-context | 工具注册、MCP、PTY、shell 会话、代码上下文服务 |
| 插件与沙箱 | wf-plugin、wf-sandbox、mlua/LuaJIT、wasmtime 34 + Cranelift 系列、libloading、ed25519-dalek、cap-std、moka | `wf-server` 默认 feature 为 `plugins,lua,native,wasm`；native 运行时默认禁用但仍然编译 |
| 可观测性 | tracing-subscriber、wf-metrics（17 个采集器） | 默认程序化配置下 metrics 为 `None`，server 另建一个空 registry；部署模板默认启用采集 |

`--no-default-features` 相比默认构建移除 81 个 crate，主要是 wasmtime、Cranelift、wit/cap-std、wf-plugin、mlua/LuaJIT、libloading、ed25519-dalek；仅保留 `plugins,lua` 时只增加 10 个 crate（wf-plugin、mlua、LuaJIT 及签名相关），不引入 wasm 栈。

## 实测方法与环境

devs：32 核、32 GiB 内存、glibc 2.27、stable Rust。三种构建均成功：

```bash
cargo build --release --locked -p wf-server --bin wf-server
cargo build --release --locked -p wf-server --bin wf-server --no-default-features
cargo build --release --locked -p wf-server --bin wf-server --no-default-features --features plugins,lua
```

每个变体在独立空目录启动，参数为 `--addr 127.0.0.1:<port> --log-level warn`，默认内存存储；等待 `wf-server listening` 后取 `/proc/<pid>/status`、`smaps_rollup` 与按映射聚合的 `smaps`，再请求一次 `/health` 复测。以下 RSS 为健康请求后的 `VmRSS`，文件映射页包含二进制与动态库。

| 变体 | 二进制 | 依赖数 | RSS | 匿名 | 文件映射 | 线程 | VmSize |
|---|---:|---:|---:|---:|---:|---:|---:|
| 默认（32 核） | 151 MiB | 482 | 38.7 MiB | 9.3 MiB | 29.4 MiB | 34 | 2313 MiB |
| 仅 Lua 插件 | 125 MiB | 411 | 36.2 MiB | 8.9 MiB | 27.2 MiB | 34 | 2296 MiB |
| 无插件 | 121 MiB | 401 | 34.0 MiB | 8.1 MiB | 25.9 MiB | 34 | 2290 MiB |
| 默认，限制 2 核 | 151 MiB | 482 | 35.2 MiB | 6.6 MiB | 28.5 MiB | 4 | 330 MiB |
| 无插件，限制 2 核 | 121 MiB | 401 | 31.8 MiB | 6.0 MiB | 25.8 MiB | 4 | 308 MiB |
| 默认 + SQLite 存储 | 151 MiB | 482 | 40.2 MiB | 10.5 MiB | 29.7 MiB | 38 | 2578 MiB |

其他观察：

* 默认进程最大的常驻映射是二进制代码段（约 23 MiB），其次是约 6 MiB 堆、约 5.7 MiB 只读数据；LuaJIT 只占约 64 KiB 代码页。空闲态“内存占用”主要由链接进镜像的功能面积决定。
* `MALLOC_ARENA_MAX=2` 在 34 线程下把 VmSize 从 2313 MiB 降到 265 MiB，RSS 保持约 38 MiB；因此 64 MiB 的 `PROT_NONE` 映射来自 glibc 的每线程 arena（大小随 CPU 数增长），不是 wasmtime。嵌入式若用 musl 通常没有该现象；glibc 环境建议显式限制 arena 数。
* `strip` 把三种二进制分别缩到 110 MiB、92 MiB、89 MiB；符号表不影响 RSS，但显著影响镜像大小与烧写时间。
* 关闭关键词索引（同时关闭 workspace 域）后 RSS 约下降 2 MiB，说明空闲态索引本身开销有限；它更大的意义在于避免加载 FTS/中文词典相关代码与后续索引增长。

## 运行时内存热点

按“空闲已占用”与“负载后增长”分开看：

1. 启动期一次性分配：584 条路由与 OpenAPI 描述、工具与资源注册表、内置 prompt/fragment/模板、事件总线（容量 1024）、插件引擎与资源装配、关键词索引打开、17 个指标采集器、Tokio worker 线程。无插件与有插件的 RSS 差约 2 MiB，主要来自插件引擎、桥接对象和更多代码页。
2. 执行历史与并发预算：Agent/Workflow 执行的消息列表、工具结果、压缩窗口、checkpoint snapshot 都随并发线性增长；`limits.toml` 的 `max_concurrent = 0` 会按 CPU 核数派生，32 核上可同时拉起 32 个 agent，嵌入式必须显式设小。
3. 存储缓存：SQLite 模式下 24 个实体存储各有 1000 条、TTL 300 秒的 moka 缓存，且容量在 `StorageManager` 中硬编码为默认值；大量工作流/消息写入后缓存是主要的堆增长点之一。PostgreSQL 模式没有进程内实体缓存。
4. 文件检查点：抓取时把整个工作区文件集合读进 `HashMap<路径, 字节>`，单文件上限默认 10 MiB，再经过 Git 对象存储与三方合并；即使默认关闭，相关依赖仍全部链接。手动 watcher 与 GC 定时器会在启用后常驻。
5. RAG/检索：关键词索引的 SQLite FTS 文件、mixed tokenizer 的 jieba 词典、稠密检索的向量批次与 embedding/rerank 请求缓冲。当前默认走 SQLite FTS（英文分词），混合检索启动开销小，但大语料索引与批量导入会持续增长。
6. PTY 与 shell：每个会话用无界 channel 与不断累积的输出字符串，订阅者再多路复制；长时间运行的交互式会话需要上限和裁剪策略。
7. 管理面：后端注册表的 sqlx SQLite 池、SSH tunnel 子进程、备份打包时的内存缓冲；默认 `backends.enabled = false`，不会启动，但仍编译进镜像。指标采集器使用带标签的映射，标签基数（模型、工具、节点）增长会长期占内存；限流表在 1 万客户端前不清理过期项。
8. 虚拟地址：除 glibc arena 外，Tokio/Rust 线程栈（8 MiB 保留/线程）与 LuaJIT/wasmtime 的地址空间预留也计入 VmSize，但不等于物理内存。

## 可裁剪功能与建议

### 已有开关，建议直接使用

* 插件后端：`plugins` / `lua` / `native` / `wasm` 已独立成 feature。嵌入式优先 `--no-default-features`；需要 Lua 插件时加 `plugins,lua`。`native` 运行时默认关闭但会被编译，无原生插件时不要开启。
* 关键词索引：`WF_KEYWORD_ENABLED=false` 或配置 `keyword.enabled = false`，可在不删代码的情况下省掉索引打开与相关代码路径。
* 文件检查点、后端管理域、静态前端、workspace 域（`WF_USER_GIT_ENABLED=false`）默认或可按配置关闭；关闭后不启动 watcher、GC、tunnel 与后台探测。
* 运行时并发：`limits.toml` 显式设置 agent/workflow 的 `max_concurrent`，不要使用按核数派生的默认值；收紧指标 buffer/retention 与 file-checkpoint 的 `max_file_size`、`manual_watch`、`gc_interval_secs`。

### 建议新增的条件编译边界

以下功能当前都在默认构建中无条件链接；按嵌入式镜像大小排序，建议在原模块内增加 feature 门控，而不是新建替代模块：

| 候选 feature | 裁掉的内容 | 预期收益 |
|---|---|---|
| `rag` / `keyword-sqlite` | `rag-core`、jieba、rusqlite FTS、向量/embedding/rerank 链、RAG 管理路由与检索节点 | 免于加载词典与索引代码，去掉一整条向量栈；仅保留关键词 SQLite 时仍可做混合检索的降级形态 |
| `file-checkpoint` | `checkpoint-file`、gitoxide 全栈、zstd、notify、文件检查点 watcher/GC/审批/provenance 路由 | 大幅缩小镜像，消除快照时的整工作区内存峰值 |
| `workspace-tools` | PTY、shell 会话与事件桥、Git/FS 写路由、MCP 客户端（rmcp 及子进程传输）、code-context 集成 | 得到纯 API 执行服务，去掉大量常驻会话状态与外部进程依赖 |
| `backends` | server 自有后端注册表、探测、SSH tunnel、备份；连带 server 直连的 reqwest/sqlx/zstd | 默认已关闭，编译期去掉可进一步缩小镜像 |
| `observability` | 细分指标采集器、Prometheus/统计端点、utoipa schema 派生 | 减少启动期分配与代码面积；`openapi-docs` 已控制路由暴露，但 schema 代码仍默认编译 |
| `storage-sqlite` / `storage-postgres` | 工作区 sqlx 目前同时启用两种驱动 | 内存存储或仅 SQLite 的嵌入式构建不再链接 PostgreSQL 驱动 |

配套的配置改动：把 `CacheConfig`（当前固定 1000 条/300 秒）、事件总线容量、Tokio worker 线程数、PTY 输出上限暴露为部署配置；`#[tokio::main]` 默认按核数起 worker，嵌入式应允许固定为 1～2。

### 构建配置

在 `Cargo.toml` 的 release profile 中启用 `strip = "symbols"`，并按目标选择 `opt-level = "z"`（或 `"s"`）、`lto = "thin"`、`codegen-units = 1`。若裁剪掉 LuaJIT，可进一步考虑 musl 静态链接；否则 LuaJIT 的动态库依赖会阻碍纯静态部署。

### 推荐嵌入式档位

* 最小 API 节点：`--no-default-features` + strip + worker 线程 1～2 + `MALLOC_ARENA_MAX=1` + 关键词索引关闭 + 并发上限显式配置。实测空闲 RSS 约 32 MiB（2 核口径），镜像约 89 MiB（strip 后）。
* 需要 Lua 插件：在上述基础上加 `plugins,lua`，镜像约 92 MiB，空闲 RSS 增加约 2 MiB，并需随镜像携带 LuaJIT 动态库。
* 需要本地检索：再开启 `rag` 中的关键词 SQLite 子集，避免 tantivy/elastic 与稠密向量栈；把索引目录放到可写分区，并限制导入批量大小。

## 备注

* 本次只做构建与观测，未修改生产代码；devs 上的构建脚本、测量脚本和三个二进制位于 `/data/kkkqkx/wf-agent/`，不在仓库内。
* 负载场景（大量执行、文件快照、RAG 导入）的内存峰值尚未实测；上线前应在目标硬件上按真实并发重测，并重点观察堆增长与 SQLite 页缓存。
* 基于本文数据的裁剪设计见 `docs/archive/perf/wf-server-embedded-memory-design.md`，对应修改方案见 `docs/plan/wf-server-embedded-trim-plan.md`。该方案已落地，release 实测（2 核、`MALLOC_ARENA_MAX=1`）无插件档位为 59 MiB 镜像、22.4 MiB RSS、83 MiB VmSize。
* 改造前后各档位、各开关的统一口径对照表见 `docs/archive/perf/wf-server-mode-resource-baseline.md`。
