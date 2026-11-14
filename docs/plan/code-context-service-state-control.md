# 代码上下文服务（CCE）状态控制：问题清单与改造方案

本文针对 wf-agent 对接外部 code-context 服务（CCE server）的现状，给出问题清单、改造方案、已落地改动与剩余缺口。
引用路径相对仓库根目录，只描述结论与意图，不贴实现代码。

## 一、现状（改造前）

服务状态的"控制"只有三处：配置声明、引导期一次性解析、运行期一个私有监督任务。

- 配置：`tools.toml` 的 `[code_context]` 段经 `wf-config/src/processor/tools.rs` 校验/转换后得到 `wf-integration` 的 `CodeContextConfig`（主开关 + 传输 + fold/retrieval 两套策略）。
- 引导：`wf-runtime/src/bootstrap/code_context.rs` 一次性解析。外部模式做一次 `/api/health` 探测后原样透传；托管模式挑回环端口、写一份只含 host/port 的临时 TOML、拉起子进程、就绪后把配置归一化成"外部地址"形态，并保留 `RunningSidecar`。
- 消费者：检索/导航工具在注册时 clone 一份配置进闭包；折叠链把服务地址**烘焙进工作流节点配置**。
- 运行期：只剩 `transport/sidecar.rs` 的监督任务持有一份私有状态（重启计数、退避、就绪）。
- 关闭：`bootstrap/shutdown.rs` 单点调用 `sidecar.shutdown()`，`child.kill()` 后删除临时文件。

## 二、问题清单

### A. 状态建模

1. **状态被复制而非共享。** 折叠地址在引导期烘焙进模板（`bootstrap/inner.rs`），工具 handler 持死配置快照。服务晚启动、托管启动失败后恢复、外部服务重启，消费者都看不到；唯一出路是重启 wf-agent。
2. **重压链与首压链不一致。** `create_fold_summary_repress_workflow` 既不接服务地址也丢弃配置的 `FoldPolicy`（回退默认值），重压阶段永远只走本地折叠。
3. **能力暴露与可用性解耦。** 六个 code-context 工具定义是静态常量、无条件注册；服务停用时模型仍看到它们，每次调用都报错。
4. **无对外状态面。** 服务就绪/降级/重启次数/断路器位置不可查询，wf-api、wf-server、TUI、Web 均无入口（对比：存储健康有 `wf-api/src/infra/diagnostics.rs`，事件系统有 `events/health.rs`）。也没有 service 维度的 metrics。

### B. 健康判据

5. **"就绪"不等于"可用"。** 探测只判断"拿到任意 HTTP 响应"，而 CCE 的 `/api/health` 在全部子系统不可达时仍返回 200 + `healthy:false`。因此 embedding/qdrant/bm25 全挂的 sidecar 也会被判为就绪。
6. **探测一次就结束。** 外部模式引导后不再探测；托管模式只在启动期探测。启动之后的退化只能靠调用失败发现。

### C. 故障放大

7. **没有熔断/负缓存。** 服务不可用时每次调用都要等满 `timeout_ms`（默认 60s）；检索工具直接向模型返回错误，模型会重试。
8. **折叠链的重试会放大故障。** 失败批次被二分拆批重试（`fold_operation.rs`），服务已死时最坏约 14 次串行 60s 调用，而压缩链总超时只有 240s——本该"静默跳过"的场景被升级成压缩链超时失败。折叠阶段也没有自己的时间预算。

### D. 进程生命周期

9. **只会强杀。** 停止走 `child.kill()`（SIGKILL），而 CCE 自身实现了 SIGINT/SIGTERM 优雅停机，强杀会跳过存储落盘。
10. **文档中的兜底不成立。** `RunningSidecar` 声称"未显式 shutdown 时依赖 kill_on_drop 兜底"，但子进程由分离出去的监督任务持有，drop `JoinHandle` 只是 detach：既不停止进程也不清理临时文件；父进程被强杀时 CCE 变成孤儿。
11. **启动失败泄漏临时文件。** 就绪失败路径只杀进程，`temp_files` 不清理；锁/配置文件在异常退出时残留。
12. **重启用例不可配置。** 重启预算硬编码为 3；耗尽后永久放弃且无任何可观测信号或恢复入口。
13. **端口选取存在竞态。** 先绑定再释放、子进程再绑定，冲突表现为启动期致命失败。

### E. 隔离与身份

14. **托管模式把数据写进临时目录并与他人共享。** 生成的配置只钉 host/port，其余用 CCE 默认值；CCE 会把相对路径锚定到**配置文件所在目录**，也就是系统临时目录。结果是 `metadata.db`、向量、BM25 索引全部落在 `/tmp`：多个 workspace/多个 runtime 共享同一份索引，且存在 SQLite/向量 WAL 并发写风险；数据还会被系统清理。
15. **无单实例保护。** 同一 workspace 起两个 runtime 会同时操作同一份嵌入式存储。
16. **无凭据/多租户支持。** admission 开启的 CCE 需要 Bearer token 与项目作用域，集成层没有任何凭据配置。
17. **无协议兼容握手。** 端点硬编码，没有 `/api/config` 或版本探测；版本不匹配表现为不可解码响应而不是明确失败。

### F. 项目管理

18. **假设索引已存在。** 只有 `default_project_id` 一个裸数字，没有项目创建、索引/增量索引、watch 启停、索引状态、死信重试；服务活着但项目没建过索引时检索永远为空。

### G. 配置与可观测性

19. **无热重载。** 配置改动只能重启进程。
20. **故障难定位。** 子进程 stdout 丢弃、stderr 直通，日志无关联；无请求 id 传递。

## 三、改造方案（已落地）

### 1. 统一的活状态对象：`CodeContextService`

`wf-integration/src/code_context/service.rs` 新增服务状态对象，持有配置、已解析端点、最近一次健康结论（含各子系统标志）、启动失败原因、子进程状态句柄与共享 HTTP 客户端。

- 消费者（工具 handler）不再拿配置快照，而是按**调用范围**要端点：`Fold` / `Navigation` / `KeywordSearch` / `HybridSearch`。范围决定需要哪些子系统，因此"服务活着但 embedding 挂了"时折叠与符号导航照常，语义检索快速失败并给出可读原因。
- 端点由引导期决定一次（托管模式的回环端口在进程启动前就已确定），所以子进程重启不会让烘焙进模板的地址失效；变的是健康与可用性，不变的是地址。
- 三个构造入口表达清楚的状态来源：`disabled` / `external`（地址即配置）/ `managed`（引导期决定的回环地址），外加 `unresolved`（地址都没能确定）。

### 2. 健康判据升级为子系统级

`transport.rs` 新增 `HealthReport{reachable, healthy, vectors, bm25, embedding}` 与 `parse_health`，直接解析 CCE `/api/health`。缺失的子系统条目继承总体结论（单一规则，不做多路兜底）。健康结论缓存 15s，过期由下一次调用触发一次单飞探测。

### 3. 传输层熔断器

`transport/breaker.rs`：按 origin 聚合调用失败的断路器。三通道（折叠/检索/导航）都经由 `post_json`，因此谁先发现服务失联都能让同一个断路器跳闸。

- 连续 3 次不可用即开路，冷却从 5s 起按失败次数翻倍、上限 60s；开路期间调用不触网，直接返回带剩余冷却的可读原因。
- 冷却结束后放行一次作为探针：成功闭合，失败延长冷却。恢复不依赖后台任务。
- 分类严格：只有"服务不可用"（传输失败、超时、5xx、429、408）计入失败；4xx 与不可解码响应属于配置/协议问题，不跳闸、不重试，避免把错误配置伪装成故障。

### 4. 失败类型化

`post_json`/`probe_health` 返回 `ServiceError{kind, reason}`，`kind` 区分 `Unavailable`/`Rejected`/`Protocol`，并保留 `Into<String>` 以便既有降级路径不变。

### 5. 折叠阶段预算

- `FoldPolicy` 新增 `total_budget_ms`（默认 60s），配置项 `foldTotalBudgetMs` 与其对应。
- `execute_fold` 以截止时间为准：每次调用超时取 `min(单次超时, 剩余预算)`；预算耗尽立即以 "fold stage budget exhausted" 跳过。
- 判定为"服务不可用"的失败不再二分拆批重试（拆分只会把超时乘以批数），直接结束并保留已折叠结果。
- 首压链与重压链现在都从同一份有效服务配置烘焙，包含配置的 `FoldPolicy` 与 `total_budget_ms`。

### 6. 进程控制正规化

`transport/sidecar.rs` 重写：

- 状态机 `Starting/Ready/Restarting/Stopped` 放在共享句柄里，可被服务状态与诊断读取；附带重启次数、最近错误、pid。
- 停止改为 SIGTERM → 宽限期（默认 5s）→ SIGKILL；非 Unix 退化为强杀。
- `Drop` 取消监督并 abort 任务（子进程随 `kill_on_drop` 结束），同时清理临时文件；`shutdown()` 是优雅路径并等待监督任务退出。
- 临时文件通过 RAII 守卫持有，就绪失败、重启失败、正常关闭都清理。
- 重启预算改为配置项 `managedMaxRestarts`。

### 7. 数据隔离与单实例

- `write_cce_server_config` 现在把 SQLite、向量目录、BM25 索引、日志**全部钉成数据目录下的绝对路径**（相对路径会被 CCE 锚定到临时目录，这正是原先数据乱跑的根因）。
- 数据目录固定为 `{project_root}/.wf/code-context`（`wf-config/src/layout.rs` 新增常量与创建函数，含 `.gitignore`），用 `runtime.lock`（`create_new`）声明独占：第二个 runtime 得到明确的失败原因而不是并发写坏存储。
- 托管组装集中到 `prepare_managed_service`：建目录 → 抢锁 → 写配置 → 返回完整启动规格；任何一步失败都清理已创建的文件。

### 8. 引导期输出可观测

`resolve_code_context_service` 返回共享服务对象 + 可选 sidecar。外部模式在引导期探测并区分"不可达"与"可达但降级"（打印各子系统状态），托管模式在就绪后同样探测一次；失败一律记录到服务的 `startup_error`，调用方据此快速失败而不是逐调用等超时。启动结束时打印一次完整状态快照。

## 四、验证

- `cargo clippy --all-targets --all-features` 通过（无本次改动引入的告警）。
- `cargo test -p wf-integration`（含断路器跳闸/半开、健康解析与范围判定、sidecar 就绪与停机、托管文件准备与单实例锁、服务范围门禁）。
- `cargo test -p wf-execution-shared`、`-p wf-config`、`-p wf-tools`（含 code-context 工具降级与作用域用例）、`-p wf-runtime compression`。

## 五、剩余缺口（本轮未做）

1. **项目/索引生命周期**：仍假设索引已存在。需要补项目创建、索引与增量索引、watch 启停、索引统计、死信重试，并把 `default_project_id` 换成按 workspace 推导的映射。
2. **对外状态面**：`CodeContextService::status()` 已可直接序列化，但尚未挂到 `wf-api` 诊断与 `wf-server` 路由（新增路由需同步 OpenAPI 快照与前端客户端），也未进 TUI/Web 面板与 metrics。
3. **凭据与协议握手**：Bearer token/CA/每项目令牌，以及启动时的 `/api/config` 版本探测。
4. **托管启动失败的自愈**：当前为终态（明确报错、快速失败），重试需要重启进程；如需自愈，应把首次拉起也交给监督任务并暴露"等待恢复"状态。
5. **配置热重载**：服务配置改动仍需重启进程。
6. **能力可见性**：服务停用时是否应从模型工具清单中隐藏 code-context 工具，属于资源层的工具可见性策略，未在本次范围内。
7. **请求关联与日志**：子进程日志未纳入 runtime 日志/追踪关联。
