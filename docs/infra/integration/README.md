# wf-integration

infra 层外部服务集成 crate：负责以统一的 HTTP 传输 + 本地托管进程（sidecar）方式对接外部服务，并内置「代码上下文服务（code context）」的完整客户端适配层。它只做传输与线协议适配，不承载领域策略——预算、默认值等策略留在调用方适配器中。当前唯一的业务通道是 code-context 服务（外部 CCE server），服务两个消费者方向：压缩路径的文件折叠阶段，以及预定义代码检索/导航工具。

## 依赖

`wf-types`、`wf-common`，以及 serde/serde_json/reqwest/tokio（process、net、io-util 特性）/tokio-util/tracing。是 infra 层的叶子型 crate：不依赖其他 infra 包，反被 `wf-config`、`wf-resource`、`wf-execution-shared`、`wf-tools`、`wf-workflow`、`wf-runtime` 等复用。

## 模块结构

### transport — 传输层

| 模块/项 | 职责 |
|---|---|
| `transport.rs` | 传输模型：`TransportMode`（`External` 连接独立部署地址 / `Managed` 由运行时启动并托管本地进程）；`ServiceTransport` 描述一次服务调用的整体超时、地址与托管参数（二进制名、回环端口、启动上限、重启预算），`normalized_base_url` 负责地址归一化。另定义 `CallScope` / `HealthReport` / `ServiceError` 与健康解析 |
| `http_client` / `post_json` | 构建带客户端级超时的 JSON HTTP 客户端；单次调用整体超时，先过断路器再触网，失败按 `ServiceError` 分类返回 |
| `probe_health` | 健康探测：GET `/api/health` 并解析子系统结论，绕过断路器（探测是恢复依据），成功即闭合断路器 |
| `breaker_for` | 按 origin 取断路器，供状态诊断读取相位与剩余冷却 |
| `pick_loopback_port` | 绑定临时监听器挑一个空闲回环端口（best-effort，仅适合单机回环） |
| `transport/sidecar.rs` | 托管进程生命周期：`SidecarSpec`（程序、参数、环境、基础地址、启动上限、重启预算、停机宽限期、临时文件、数据目录独占锁、日志捕获文件与尾部行数）→ `start_sidecar` 只建状态与派发监督任务，返回即"启动中"；`wait_ready` 做有界等待，超时不是终态。监督任务持有数据目录锁（重试期间保持独占、彻底停止时释放）、拉起子进程、轮询就绪，并按相位区分处置：`RetryWait`（拉起但不就绪，按预算重试）、`Restarting`（异常退出后退避重启）、`Stopped`（停机、预算耗尽、锁冲突或二进制缺失）。二进制缺失直接终态并提示配置，不做无意义重试；关闭令牌触发时 SIGTERM → 宽限 → SIGKILL。子进程 stdout/stderr 追加写入日志文件，失败原因附带该文件尾部若干行。`RunningSidecar` 显式 `shutdown()` 走优雅路径，`Drop` 取消监督并 abort（子进程随 `kill_on_drop` 结束）且清理临时文件 |

sidecar 只负责进程生命周期，不涉及线协议；线契约始终是普通 HTTP。

### code_context — 代码上下文客户端

| 项 | 职责 |
|---|---|
| `code_context.rs` | 配置与策略：`CodeContextConfig`（`tools.toml` 的 `[code_context]` 段，含主开关 `enabled`、传输、fold/retrieval 两个通道策略；整体缺失或 `enabled == false` 表示服务停用，所有消费方报告不可用）。`FoldPolicy`（进入折叠的最小 token 门槛、单条 token 预算、单批条目上限、失败重试轮数、折叠阶段总预算）、`RetrievalPolicy`（默认项目 id、默认条数、调用方上限——服务仍是最终执行者并按自身硬限截断）。托管组装集中在 `prepare_managed_service`：建数据目录 → 抢 `runtime.lock` 单实例锁 → 写服务端配置（回环地址 + SQLite/向量/BM25/日志全部钉成数据目录下的绝对路径）→ 返回完整启动规格；`write_cce_server_config` 与 `cce_server_command` 分别负责配置内容与启动行 |
| `code_context/service.rs` | 活状态：`CodeContextService` 持有配置（可整体替换）、已解析端点、最近健康结论（`HealthReport`：可达 + 向量/BM25/embedding 三个子系统）、凭据解析结果、能力握手结论、项目状态、子进程状态句柄与共享 HTTP 客户端。消费者按 `CallScope`（Fold / Navigation / KeywordSearch / HybridSearch / Operations）要端点：范围决定需要哪些子系统，因此"服务活着但 embedding 挂了"时折叠与导航照常、语义检索快速失败。`availability()` 给出装配期可见性（只读缓存、"仅当确认不可用才隐藏"）；`project_id_for` 按"调用显式 → 运行时解析 → 配置覆盖"给项目 id，未解析时惰性重试一次以便服务恢复后无需重启；`apply_config` 是热重载唯一入口（替换策略与端点、重置健康/握手缓存与断路器，并报告是否需要停旧起新）。维护入口（`dead_letters` / `retry_dead_letters` / `acknowledge_dead_letter` / `retry_index`）走同一条门禁但按 `Operations` 范围判定，失败分为"本地前置条件拒绝"与"服务不可用"两类。`status()` 输出可序列化的 `CodeContextStatus` 供诊断使用；健康结论缓存 15s，过期由下一次调用触发单飞探测 |
| `code_context/retrieval.rs` | 检索通道：混合搜索（`/api/search`）与 BM25 关键词搜索（`/api/tools/keyword-search`）的请求构造、响应归约与参数校验。归约只暴露对模型导航安全的字段（文件路径、行区间、片段、分数、类型/来源标记），内部连接键（数字实体 id、分段 id）不越服务边界；片段按 `MAX_SNIPPET_CHARS` 截断并逐条标记 `truncated` 而非静默丢弃。`clamp_limit` 依策略预算收敛调用方条数（默认 → 上限 → 服务硬限） |
| `code_context/navigation.rs` | 符号导航通道：`/api/tools/symbols`、`/api/tools/references`、`/api/tools/definition` 三个端点的请求构造（路径数上限 32、符号树深度 8 / 每层子节点 32）与响应归约（隐藏数字键，剥离内部结构）。提供工具参数校验（`require_path` / `require_line` / `optional_column` / `resolve_paths` 等）与异步调用入口 |
| `code_context/project.rs` | 项目与索引生命周期：`ProjectState`（未解析 / 解析中 / 索引中 / 已就绪 / 失败，失败态保留失败前操作的 id）与 `ProjectStatus` 快照；`IndexSupplyMode` 显式区分"运行时供给"与"部署方供给"。解析 = 列项目 → 规范化 `root_path`（绝对化 + 软链解析 + 平台大小写）后精确匹配 → 未命中则创建；`last_indexed` 为空时触发首次全量索引（独立长超时，不复用单次调用超时），索引失败为终态。增量索引交给服务侧 watch，不在本地新增合并窗口。响应按 `{"projects":[...]}` 与项目记录的十进制字符串 id 解码，形状不符即报解码原因而不是静默空结果 |
| `code_context/dead_letters.rs` | 死信通道：被服务放弃的文件按"文件 × 模块"表达（模块名、重试次数、错误码与文案、是否已截断重试、是否已确认）。三个动作——列表、重试（可为空文件列表表示全部）、确认（可限定模块）——按已确认的上游结构解码，形状不符报协议不匹配而不是返回空死信；`success: false` 是合法答复，不当作传输失败。重试会重跑解析与 embedding，因此走 600s 独立预算；列表与确认用单次调用超时 |
| `code_context/fold.rs` | 折叠通道（压缩路径）：`tool_call_paths` 从消息流恢复 tool-result 对应的文件路径线索；`select_fold_candidates` 选出估算 token 达门槛的 tool-result 消息（纯函数，token 估算由调用方注入；非 tool 消息不动以保全对话结构）；`infer_language_hint` 按扩展名给服务端语言提示（仅线索，未知后缀回退服务端推断）；`FoldClient::fold_batch` 发批量折叠请求（`/api/tools/fold-batch`）；`apply_fold_results` 把结果按 id 写回快照副本（未知 id 忽略）；`build_fold_notice` 生成模型必须看到的压缩边界提示（骨架保留签名与结构、省略函数体，原文不可再得） |

## 语义约定

- **失败类型化但可直接降级**：`post_json` / `probe_health` 返回 `ServiceError{kind, reason}`，`kind` 区分 `Unavailable`（传输失败、超时、5xx、429、408）/ `Rejected`（4xx、请求级拒绝）/ `Protocol`（不可解码）；实现 `Into<String>`，既有降级路径（跳过 / 告警）无需区分。
- **可用性是传输属性**：`transport/breaker.rs` 按 origin 维护断路器，三通道共用。连续 3 次不可用即开路（冷却 5s 起翻倍、上限 60s），开路期间调用不触网；冷却结束后放行一次作探针，成功闭合、失败延长冷却。4xx 与协议错误不跳闸，避免把错误配置伪装成故障。
- **健康探测绕过断路器**：探测是恢复到依据，不能被自己观察到的状态拒绝；探测成功即闭合断路器。
- **凭据只声明环境变量名**：`auth_token_env` 记录承载令牌的变量名，值在服务构建期读取一次并挂在共享 HTTP 客户端的默认头上——检索、导航、折叠与健康探测一处生效。401/403 归类为 `Credential`（不跳闸、不重试），文案指向令牌与项目作用域，且不回显令牌。只持基础地址的通道（压缩折叠链）通过进程级共享客户端认证，凭据不进入任何节点配置。
- **维护通道独立于检索通道**：`CallScope::Operations` 只要求服务进程可用（以进程级的折叠通道作为判据），不看向量子系统，因为死信与重索引正是发生在索引进行中或索引失败时。项目 id 取活状态里已知的值，`Failed` 因此保留失败前操作的 id；供给为外部模式时重索引直接拒绝，归属仍在部署方。
- **握手只做一次**：能力 = 健康可解析 + 项目端点存在，结果缓存到配置重新应用为止；401/403 归凭据问题，端点缺失/不可解码归不兼容，两者都不逐调用失败。
- **服务是最终执行者**：本 crate 侧的策略（折叠门槛、检索上限）只是调用方意图预算，服务端按自身硬限再次收敛。
- **地址稳定、状态活**：托管模式的回环端口在进程启动前确定，因此重启不会让烘焙进工作流节点的地址失效；变化的是健康与可用性。

## 测试

各模块内联 `#[cfg(test)]`：服务端配置写入与托管文件准备、检索/导航请求与响应形状、参数校验、候选选择与结果回写、断路器跳闸/半开、健康解析与调用范围判定、服务范围门禁与启动失败短路、项目解码与路径规范化匹配、供给流程（建项目 → 首次索引 → watch → 就绪，且二次供给不重复索引）、索引期间快速失败、凭据缺失阻断、能力握手不兼容、`apply_config` 重置与重启判定、死信解码与动作请求、失败索引后的维护可用性与人工重索引恢复（脚本化服务记录失败次数）、sidecar 启动超时/二进制缺失/数据目录锁独占与释放/子进程日志捕获（用临时 TCP 响应器模拟就绪）。
