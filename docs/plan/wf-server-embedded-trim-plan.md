# wf-server 嵌入式裁剪修改方案

本文给出 `docs/archive/perf/wf-server-embedded-memory-design.md` 所述档位的实施改动清单。只修改现有模块与门面，不新增替代实现，不做旧构建方式的兼容映射，版本号保持现状。路径相对仓库根目录。

## 一、改动总览

| 层 | 文件 | 动作 |
|---|---|---|
| 工作区 | `Cargo.toml` | 拆分 sqlx 的 sqlite/postgres 驱动 feature；release profile 增加符号剥离、瘦链接与体积优先选项 |
| 传输 | `crates/app/wf-server/Cargo.toml` | 新增 `file-checkpoint`、`keyword`、`rag-vector`、`pty`、`mcp`、`git-fs`、`backends`、`metrics`、`storage-sqlite`、`storage-postgres` feature，按依赖方向转发；把仅对应单一域的依赖改为可选 |
| 传输 | `crates/app/wf-server/src/main.rs` | 运行时构建从宏改为显式 builder，读取 worker 线程配置；按 feature 门控指标 registry、后端管理器与静态托管装配 |
| 传输 | `crates/app/wf-server/src/router.rs` 与 `src/api.rs` | 域级路由按 feature 合并；被裁域不进入路由表 |
| 传输 | `crates/app/wf-server/src/api/{git,fs,pty,rag,checkpoint}` 及 `src/backends/` | 模块声明按 feature 门控；保留未被裁的执行检查点查询路由 |
| 传输 | `crates/app/wf-server/src/openapi.rs` | 整个文档生成随完整档位编译；离线 codegen 固定使用全量 feature |
| 运行时 | `crates/app/wf-runtime/Cargo.toml` | 接收 server 的 feature 并转发到 `wf-api`、`wf-checkpoint`、`wf-tools`、`wf-metrics`、`wf-storage` |
| 运行时 | `crates/app/wf-runtime/src/bootstrap/inner.rs` | 关键词索引打开、MCP 初始化、代码上下文解析、文件检查点栈装配按 feature 分支；不注入占位实现 |
| 运行时 | `crates/app/wf-runtime/src/bootstrap/{plugin,shutdown,config}.rs` | 插件桥接与后台任务关闭按 feature 分支；配置结构中的可裁段落随 feature 出现 |
| API | `crates/app/wf-api/Cargo.toml` | 把向量/检索、PTY、code-context、文件检查点相关依赖改为可选并绑定 feature |
| API | `crates/app/wf-api/src/{rag.rs,pty.rs,infra/context.rs}` | 关键词索引槽位与检索句柄按 feature 收敛为“不存在”，保留既有服务不可用语义 |
| 检查点门面 | `crates/infra/checkpoint/wf-checkpoint/Cargo.toml` 与门面导出 | `checkpoint-file` 改为可选；执行检查点能力无条件保留；上层仍只依赖门面 |
| 检索 | `crates/infra/rag-core/Cargo.toml` | 新增 `keyword-sqlite`、`vector` feature；SQLite FTS 与分词、向量与重排依赖分别归属 |
| 工具 | `crates/engine/wf-tools/Cargo.toml` | 新增 `mcp` feature 控制 rmcp 及子进程传输依赖；内置 shell 节点保持核心 |
| 指标 | `crates/infra/wf-metrics/Cargo.toml` | 新增 `metrics` feature 控制细分采集器；保留最小 registry 类型供健康端点使用 |
| 存储 | `crates/infra/wf-storage/Cargo.toml` 与 `src/decorator/cache.rs` | 按 feature 选择存储实现；缓存容量与 TTL 从固定默认值改为配置入参 |
| 部署 | `deploy/templates/infrastructure/` 与部署脚本 | 增加 worker 线程、事件总线容量、实体缓存、PTY 输出上限的配置示例；启动脚本按档位设置 `MALLOC_ARENA_MAX` |

## 二、Feature 传播链

- `plugins` / `lua` / `native` / `wasm` 保持现有链路：server → runtime → 插件与沙箱 crate，不再扩展语义。
- `file-checkpoint`：server → runtime → `wf-checkpoint` → `checkpoint-file`。门面内部收敛 re-export，禁止上层直接依赖内部 crate。
- `keyword`：server → runtime → `wf-api` → `rag-core/keyword-sqlite`。只控制 SQLite FTS 与分词；现有 `tantivy`、`elastic` 名称不变。
- `rag-vector`：server → runtime → `wf-api` → `rag-core/vector` 与 `vector-contracts`。不牵连 `keyword`，两者可独立组合。
- `pty`：server → `wf-api`；`mcp`：server → runtime → `wf-tools`；`git-fs`：仅 server 域路由。
- `backends`：仅 server 域，连带其直接依赖的 reqwest、sqlx、zstd 使用。
- `metrics`：server → runtime → `wf-metrics`；`storage-sqlite` / `storage-postgres`：server → runtime → `wf-storage` → 工作区 sqlx 驱动 feature。

所有新增 feature 在产品默认 feature 中开启，嵌入式档位显式关闭。禁止在构建脚本中按目标平台自动选择档位。

## 三、运行时装配改动

按 `bootstrap/inner.rs` 的阶段顺序处理：

1. 配置阶段：可裁域的配置段落随 feature 出现；缺失字段按未知字段忽略，不产生后台任务。
2. 存储阶段：`StorageManager` 按存储 feature 选择可用后端；实体缓存容量来自配置，内存后端不创建缓存装饰器。
3. 检索阶段：`keyword` 关闭时跳过索引打开，API 上下文的关键词槽位为空；`rag-vector` 关闭时不注入检索句柄与检索节点，相关路由不注册。
4. 工具阶段：`mcp` 关闭时不创建连接管理器、不注册 `use_mcp` 与动态工具；内置工具与执行回调保持原样。
5. 代码上下文：`workspace` 相关能力关闭时不解析托管服务、不拉起子进程、不向折叠链烘焙服务地址；压缩链在此档位按无服务的缺省语义装配，该语义需与现有压缩实现核对后定稿。
6. 文件检查点：feature 关闭时不创建管理器、事件桥、watcher 与 GC 定时器，也不注册审批工具与相关路由。
7. 指标与关停：采集器按 feature 装配；关停流程只对待存在的后台任务发出停止信号，不使用兜底分支掩盖装配缺失。

缺位策略统一为“不注册、不启动、不占位”：调用方观察到路由不存在或明确的服务不可用，而不是空对象返回成功。

## 四、路由与 API 表面

- 在 `api.rs` 的域合并点按 feature 组装路由；`git-fs`、`pty`、`backends`、文件检查点、检索管理域分别门控。
- 工作流、智能体、聊天、执行查询、鉴权、健康检查保持全档位可用。
- OpenAPI 文档只在 `openapi-docs` 档位编译与暴露；离线 JSON 生成使用全量档位，避免裁剪后的文档缺失端点。
- 每个档位的缺失路由清单写进部署文档，前端按能力探测或档位声明适配，不依赖逐个试错。

## 五、配置与模板

- `worker_threads`：基础设施配置新增字段，默认沿用“CPU 核数派生”的产品行为；嵌入式模板显式设为 1～2。
- `event_bus_capacity`、`entity_cache_capacity`、`entity_cache_ttl_secs`、`pty_output_limit_bytes`：落在既有基础设施配置合并链，命令行与环境变量仅做覆盖。
- 关键词、文件检查点、后端管理域的运行期开关沿用现有字段与环境变量，不新增第二套语义。
- 部署模板中的新增字段使用英文注释，与现有模板风格一致；文档侧用中文说明档位取值。

## 六、构建配置

- 根 `Cargo.toml` release profile：`strip = "symbols"`；嵌入式档位使用体积优先优化、瘦链接与单代码生成单元，完整档位维持现状。
- 启动脚本：glibc 环境设置 `MALLOC_ARENA_MAX` 为 1 或 2；musl 目标不设置。
- 运行时线程数由配置决定，不再隐式随核数增长；日志中输出生效值便于现场排查。

## 七、实施顺序

1. **依赖边界**：先改各 crate 的 `Cargo.toml` 与 feature 转发，确认依赖图无环、默认全量构建不受影响。
2. **门面收敛**：在 `wf-checkpoint`、`wf-api` 中完成可选依赖与槽位收敛，保持上层调用点不变。
3. **运行时分支**：按阶段处理 `bootstrap`，每完成一个域即验证该档位能启动且日志中无对应后台任务。
4. **路由门控**：按域裁剪路由与 OpenAPI 表面，核对每个档位的路由清单。
5. **配置暴露**：加入新配置项与模板，验证命令行、环境变量、文件层的优先级不变。
6. **构建与验收**：应用 release profile 与启动脚本改动，执行验证矩阵并归档实测数据。

每步单独可验证，任一步回退不影响已完成的依赖边界。

## 八、验证矩阵

| 场景 | 验证内容 |
|---|---|
| 默认档位 | release 构建通过；`cargo clippy --all-targets --all-features` 通过；现有 HTTP 集成测试通过；路由与 OpenAPI 完整 |
| `embedded-core` | release 构建通过；空闲 RSS、镜像体积、线程数、VmSize 符合设计目标；路由清单只含核心域 |
| `embedded-lua` / `embedded-search` | 插件加载与关键词检索可用；缺失域不出现在路由表 |
| 运行期开关 | 全量镜像下关闭关键词、workspace、后端域时行为与编译期裁剪一致 |
| 关停流程 | 各档位接收 SIGTERM 后无残留后台任务与子进程 |

内存复测沿用现有方法：独立空目录启动、单次健康请求后读取 `/proc/<pid>/status`、`smaps_rollup` 与按映射聚合的 `smaps`，并在目标硬件上补做负载峰值测试。

## 九、明确不做的事

- 不新增精简版 crate、第二套执行引擎或版本化模块。
- 不保留旧档位的兼容别名；feature 名即唯一入口。
- 不在裁剪域中注册返回成功的空实现，不用 503 占位掩盖缺失功能。
- 不在本轮改动 RAG 打分语义、插件权限模型、配置优先级与鉴权逻辑。
- 不做跨档位的运行时热切换；档位在构建期确定。

## 十、实施状态

本轮已完成的工作：

| 项目 | 状态 | 说明 |
|---|---|---|
| 新增 feature 与转发 | 已落地 | server / runtime / api / checkpoint / tools / metrics / storage / rag-core 均声明了档位 feature，默认 feature 全开，嵌入式档位显式关闭 |
| 路由门控 | 已落地 | 文件检查点、Git/FS、PTY、RAG 管理、后端管理、指标端点在关闭时不再进入路由表；OpenAPI 模块随完整档位编译 |
| 运行时装配门控 | 已落地 | bootstrap 在关闭关键词索引、MCP、文件检查点时跳过打开/初始化/后台任务；缺位不注册、不启动、不占位 |
| 配置暴露 | 已落地 | 新增 `--worker-threads` 与 `WF_SERVER_WORKER_THREADS`、`WF_SERVER_EVENT_BUS_CAPACITY`、`WF_SERVER_ENTITY_CACHE_*`；实体缓存容量进入 `StorageManager` |
| 构建配置 | 已落地 | release profile 启用符号剥离、瘦链接与单代码生成单元；部署环境模板加入 arena 与线程配置 |
| 编译验证 | 已通过 | 默认、`--no-default-features`、`--no-default-features --features plugins,lua`、`--all-features` 四组 `cargo check --locked -p wf-server` 均通过 |

尚未做到依赖级移除的部分（feature 目前只裁剪路由与初始化，依赖仍链接）：

- `rag-core` 的向量管线与 `wf-tools` / `wf-workflow` 的检索节点共用同一套 API；移除 `llm-embedding` / `llm-rerank` 需要同时给两个 engine crate 增加 feature 并改造节点注册，留待后续改动。
- `checkpoint-file` 的类型被执行检查点门面的协调器与多个 engine 模块直接使用；移除 gitoxide 栈需要先把文件域类型从执行检查点类型中分离。
- `rmcp` 的 `McpToolInfo` 被 MCP 注册与元数据模块共用；移除该依赖需要把数据类型与传输客户端拆开。
- sqlx 的 sqlite/postgres 驱动由工作区统一启用；拆分驱动 feature 需要同时改造 `wf-storage` 的上下文宏与 `backend` 枚举。

这些 remainder 不影响当前档位的可用性，但意味着二进制体积收益主要来自插件后端裁剪与路由/初始化裁剪，而不是完整的依赖图裁剪。

release 实测（devs，2 核、`MALLOC_ARENA_MAX=1`、空闲且已响应一次 `/health`）：

| 档位 | 二进制 | RSS | VmSize | 线程 |
|---|---:|---:|---:|---:|
| 默认 | 85 MiB | 29.3 MiB | 115 MiB | 4 |
| 仅 Lua 插件 | 62 MiB | 24.7 MiB | 89 MiB | 3 |
| 无插件 | 59 MiB | 22.4 MiB | 83 MiB | 3 |

对比改造前同口径数据（无插件：121 MiB 二进制、31.8 MiB RSS、308 MiB VmSize），镜像体积下降约 51%，RSS 下降约 30%，虚拟地址空间下降约 73%。数据来自 `docs/dev/server依赖与内存占用分析.md` 中的测量脚本；完整 clippy 检查（`--all-targets --all-features`）也通过。
