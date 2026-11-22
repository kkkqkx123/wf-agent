# 断连保活与关闭语义（设计 + 落地状态）

> **状态更新**：保活与关闭路径的全部代码修改已落地并通过测试。本文前半部分是设计依据（保留事实链），凡已实现处就地标注 `【已落地】` 与 `文件:行号`；实现与设计有偏差处直接改写为现状。三项核心拍板已定：配额**按主体**、`keep` **全平台可配（默认 cancel）**、停机通知**仅 503 + bounded drain（不引入 `server.closing` 帧）**。8 个开放点已逐项决策并处置（7 项落地、attach 鉴权保持现状、部署作业化留独立任务），详见第 13 节。

本文范围是"连接消失时执行怎么办"和"关闭服务时到底关什么"。写权限与远端部署已在 `docs/plan/remote-deploy-write-path.md` 落地，本文只在与它交界处补充衔接约束。所有现状判断附 `文件:行号`。

## 0. 结论

* **保活不是新机制，是接线**。引擎已有"后台执行 + 预注册 id + 终态落结果槽 + 可查询可取消"（`crates/engine/wf-agent/src/executor/spawn.rs:17-24`），事件已持久化并支持按游标回放（`crates/app/wf-server/src/api/system/events.rs:686-712`）。缺的 dispatch、attach 两个端点和孤儿回收已补齐 `【已落地】`（`crates/app/wf-server/src/api/agent/loops/detached_handlers.rs:76,171`、`crates/app/wf-server/src/detached.rs:283`）。
* **关闭要拆成三个动词**。`detach`（只解绑观察者）、`stop`（终止执行）、`close`（关服务）今天被压缩在"连接消失"一件事上。SSH 连接属于后端条目而不是会话，任何"关会话"的动作都不该碰它。关闭路径的有界排空、执行收敛、隧道顺序已修复 `【已落地】`（`crates/app/wf-server/src/server.rs:72`、`crates/app/wf-api/src/infra/context.rs:738`）。
* **保活必须与容量记账同批上线**。许可挂在执行实体上、终态才释放（`crates/engine/wf-agent/src/registry.rs:68-71`）。断连不再中止执行之后，许可也不再随连接释放，反复断开重发就能占满并发位。没有配额，保活等于把"断连即释放"换成"断连即泄漏"。按主体配额已实现 `【已落地】`（`crates/app/wf-server/src/detached.rs:114,151`）。

## 1. 范围与前提

**在范围内**：执行（agent loop、工作流）的断连语义、SSE 订阅的生命周期、关闭服务的顺序、PTY 与隧道在关闭时的归属。

**不在范围内**：执行过程的持久化格式（已有）、检查点续跑策略（`docs/plan/execution-retention-and-lifecycle.md` 已定）、远端进程的部署与升级（已落地）。

**一个必须写清的前提**：保活的承诺是"**连接断开不影响执行**"，不是"进程重启后执行继续"。重启后不复活运行时是既有结论（`docs/plan/execution-retention-and-lifecycle.md:57`），只能从检查点续跑（`crates/app/wf-api/src/agent/agent_execution/checkpoint.rs`）。后者需要远端独立进程 + 状态外置，是另一个量级的改造。

## 2. 问题从哪来

今天"客户端断开"同时意味着三件事：观察者消失、SSE 连接关闭、执行被中止。第三件是隐式的：`ExecutionEventStream` 在 drop 时 abort 驱动任务（`crates/app/wf-api/src/infra/stream.rs:16-21` 的字段注释写得很明确："Dropping the stream aborts the driver"）。

这个设计在单用户本地场景是优点——不会留下无人观看的长任务。但一旦出现远端节点、移动端切网、浏览器标签页休眠，它就变成缺陷：用户合上笔记本，远端一个正在推进的任务被本地连接状态杀掉。

同一个服务里已经有反例：PTY 会话在连接断开时只解绑订阅者（`crates/app/wf-server/src/api/pty.rs:462-463` 的注释 "the attachment guard detaches the subscriber"）。基础设施与先例都在，只是执行路径没用它。

## 3. 现有基座

| 能力 | 现状 | 证据 |
|---|---|---|
| 后台执行（脱离请求） | `spawn_agent_loop`：预生成 id、先注册占位实体、后台 task、终态写结果槽 | `crates/engine/wf-agent/src/executor/spawn.rs:17-24` |
| 存活任务可中止 | 注册表存 `JoinHandle`，`abort_task` 用于取消 | `crates/engine/wf-agent/src/registry.rs:63-66` |
| 容量门 | `register` 取许可，超出 `max_concurrent` 拒绝，许可挂在实体上 | `registry.rs:68-71` |
| 事件可回放 | SSE 支持执行过滤 + `since` 游标 + 有界重放 | `api/system/events.rs:686-712` |
| 终态可查 | 结果槽 + 执行记录 + summary/timeline 接口 | `registry.rs:60-61`、`api/agent/loops.rs` 的 summary 路由 |
| PTY 空闲回收 | `idle && unwatched && !persistent`，600s 阈值、60s 扫描、退出会话上限 25 | `crates/app/wf-api/src/pty/state.rs:21-22,232-237` |

## 4. 缺的三件

1. **没有"派发即返回"的入口**。`stream()` 直接 await 出流并返回，调用方拿到的是流本身，没有"提交后拿 id 再订阅"的两段式。
2. **没有 attach 端点**。`pause`/`resume`/`cancel`/`append` 都作用于存活实体，没有"只观察一个正在跑的执行"的接口。路由清单里没有订阅类路径（`api/agent/loops.rs` 的 routes）。PTY 有 connect-token + connect 的两步式接入，执行没有对应物。
3. **没有回收者**。`cleanup_terminated_executions` 只有手动端点（`api/agent/loops.rs` 的 `cleanup-completed` 路由），`ApiContext` 里的实现（`crates/app/wf-api/src/infra/context.rs:711`）没有自动调用者。保活一开，实体与结果槽会一直累积。

## 5. 对象分离

把今天混在一起的三样东西拆开定义，后续所有设计都建立在这个划分上：

* **Run**：执行本身，归注册表，生命周期由状态机与容量门决定。
* **Subscription**：观察者，归连接，断开即消失，可有多个。
* **Result**：终态，归结果槽与持久执行记录，与订阅者无关。

今天 `ExecutionEventStream` 同时承担三者，所以"订阅者走"必然等价于"执行死"。拆开之后，"执行死不死"由策略决定，"订阅在不在"由连接决定，两者不再互相绑定。

## 6. 两个端点 【已落地】

`【已落地】` 路由注册于 `crates/app/wf-server/src/api/agent/loops.rs:43-44`，OpenAPI 与前端 schema 已同步（`crates/app/wf-server/src/openapi.rs:93-94,783-784`、`apps/web-app/src/lib/api/schema.d.ts:627,801,10436,12713,17874,18826`）。

**dispatch**（`POST /api/v1/agent-loops/{id}/dispatch`）：`handle_dispatch_loop`（`detached_handlers.rs:76`）预生成执行 id（`:89`），先过配额门 `admit`（`:92-98`，失败返回 503），再以 `StreamDetachPolicy::Keep` 起后台流（`:109-113`）。服务端 spawn 一个任务持有该流并 drain 到终态（`:127-144`），因此执行生命周期与请求连接解耦。响应为 `DispatchView { execution_id, detached: true }`（`:146-149`），不带事件。返回后即可 query / cancel。

**attach**（`GET /api/v1/agent-loops/{id}/attach`）：`handle_attach_loop`（`detached_handlers.rs:171`）先机会式 `sweep_idle`（`:181`），对已跟踪的执行加订阅者（`:183-185`），再用 `wf_api::infra::events::history` 回放最近事件并按 `since` 游标过滤、取 200 条（`:190-208`），接实时订阅（`:209`），遇终态事件停止并追加一个合成帧 `{"type":"execution.terminal"}`（`:232-239`）。`SubscriberGuard` 在流 drop 时减订阅者（`:282-289`），使空闲回收能看到"无订阅"。

设计要点与落地的偏差（如实记录）：

1. **"已结束"可处理** — 已落地。回放 + 合成终态帧，不 404（`:232-241`）。
2. **鉴权沿用一次性 ticket** — **决策：不引入，保持 API key 头**。当前直接沿用 API key 头（`principal_for`，`:49-55`），key 不进 URL；本轮拍板不做一次性 ticket（额外攻击面 + 改动量），记录为已接受风险。
3. **游标语义复用 `since`** — 已落地，但 `since` 是时间戳字符串，经 `decode_sse_since` 解析（`:195`），回放窗口上限 200 条（`:204`）。

## 7. detach 策略 【已落地】

`stream_detach_policy = cancel | keep`，默认 `cancel`。类型定义与解析在 `crates/app/wf-api/src/agent/agent_execution.rs:26,40,48`；配置解析在 `crates/app/wf-server/src/server_config.rs:182-197`（环境变量 `WF_SERVER_STREAM_DETACH_POLICY` / 文件层 `stream_detach_policy`，非法值启动失败）；运行时由 `router.rs:338-341,421-424` 注入 `StreamRuntime`。

| 取值 | 断连时 | 适用 |
|---|---|---|
| `cancel` | 中止执行（默认） | 本地单用户、交互式问答 |
| `keep` | 解绑订阅者，执行继续 | 远端节点、长任务、移动端 |

**【已落地】keep 模式 consumer 断开不中止执行**：`ExecutionEventStream::from_agent_stream_detached`（`crates/app/wf-api/src/infra/stream.rs:66-67`）走 `keep_on_detach=true` 的内部构造（`:70`），forwarder 在 receiver closed 后继续 drain 引擎流（`:209,224`）；`cancel` 保持原行为。两条语义各有单测覆盖（`stream.rs:366,388`）。`stream_with_policy`（`lifecycle.rs:88`）是两策略的统一入口，`stream()` 仍默认 `cancel`（`:79`）。普通 `stream` handler 用配置策略（`loops/lifecycle.rs`），dispatch 固定用 `keep`（`detached_handlers.rs:109-113`）。

**服务端是上限**：策略只在服务端配置，不暴露客户端按请求覆盖的口子（dispatch 忽略调用方偏好，固定 `keep`）。允许客户端单方面钉住服务端资源等于开放一个 DoS 面——这条不能配。**拍板**：`keep` 为**全平台可配**（默认 `cancel`），不按运行 profile 硬限制。

## 8. 孤儿回收 【已落地】

判定式沿用 PTY 的"空闲 + 无人看 + 非 persistent"（`pty/state.rs:232-237`），实现在 `crates/app/wf-server/src/detached.rs`：

* **不能用"没有事件"判空闲**。一次长工具调用可能几分钟不出事件，但它在正常推进。判据用 `DetachedRun.last_event_ms + subscribers==0 + !persistent`（`detached.rs:36-47`），`reap_candidates`（`:233`）按 `idle_secs`（默认 1800s，`server_config.rs:46`）筛。`last_event_ms` 在 dispatch 的后台 drain 每次事件时 `touch`（`detached_handlers.rs:136`）。单测覆盖"最后事件时间而非 detach 时间"（`detached.rs:390`）与 persistent 豁免（`:397`）。
* **兜底**仍有执行自身墙钟上限，保活不会无限占用。
* 回收动作在 `detached::reap`（`detached.rs:283-309`）：先 `settle`（调用方 `cancel_execution`）再 `registry.remove`，顺序不能反；二次检查仍 idle 才回收（`:295`）。单测 `reap_settles_before_removing`（`:406`）锁定该顺序。

**【已落地：常驻 reaper + 机会式兜底】** `spawn_reaper`（`detached.rs`）启动一个周期任务（扫描间隔 `REAPER_SCAN_SECS`，默认 60s），按 `reap` 的"先 settle 后 remove"语义回收空闲 detached run；由 `ServerHandle` 持有其 `CancellationToken`，shutdown 时先 cancel 再 await，且检测到 host 停机标记即自行退出。两个 serve 路径（`router.rs` 的 `serve_api_with_config` / `serve_full_with_config`）都会启动它。`sweep_idle`（`detached_handlers.rs:265-280`）在 attach 时仍做机会式兜底，dispatch 后台任务终态时也会 `registry.remove` 自身条目（`:143`）。三者共同保证孤儿不会长期滞留。

## 9. 容量记账 【部分落地】

这是最容易被漏掉的一条，也是保活能不能开的前提。

| 项 | 设计 | 落地现状 |
|---|---|---|
| 并发许可 | 挂在实体上，终态释放（`registry.rs:68-71`） | 不变；释放时机从"断连"推迟到"终态" |
| 每主体 detached 上限 | 默认 4，超限拒绝新的 detach | **已落地**：`admit`（`detached.rs:114,151`）按主体计数，超限 503；默认 `4`（`server_config.rs:48`），范围 1–64；单测 `quota_is_per_principal_and_refusals_do_not_disturb_existing`（`detached.rs:330`） |
| 每主体在跑执行上限 | 与容量门配合 | **未单独做**：沿用引擎既有 `max_concurrent` 容量门；未新增"在跑执行数"独立计数 |
| 指标 | detached 执行单独计量 | **已落地**：`detached.runs.active` gauge、`detached.quota.rejections`、`detached.runs.reaped` counter（`wf-metrics/.../constants.rs` 的 `detached_metrics`、`collectors/resource.rs`），经 `emit_active_gauge`/`emit_quota_rejection`/`emit_reaped`（`detached.rs`）写入，复用既有 Prometheus/JSON 导出 |

**主体口径（拍板：按主体）**：`principal_from_api_key`（`detached.rs:270-277`）对 API key 取 SHA-256 前 8 字节 hex 加前缀 `key_`，无 key 落入 `anonymous` bucket。原因是当前 HTTP middleware 只校验 key、不注入 principal（`request_guard`），故用 key 的稳定摘要作主体标识，既不落明文也不依赖会话。配额拒绝不影响已有执行（单测锁定）。

没有配额，保活就是把泄漏点从"连接"挪到"执行"——这一条已通过配额、常驻回收与 detached 指标三者兜住。

## 10. 关闭语义

### 10.1 现状（按动作分）

| 动作 | 今天做了什么 | 证据 |
|---|---|---|
| 客户端断连 | SSE 计数减一、订阅取消；执行流被 drop 即 abort | `api/system/events.rs` 的流收尾、`infra/stream.rs:16-21` |
| 取消执行 | 停实体 + 中止两类任务句柄 | `infra/context.rs:605-610` |
| 关闭本地服务 | 标停（仅状态位）→ 取消探活/备份循环并杀全部 `ssh -N` → 发优雅停机 → axum 排空 → runtime 关闭 | `crates/app/wf-server/src/server.rs:45-56`、`backends/manager.rs:740-743`、`backends/tunnel.rs:208-216` |
| 关闭远端服务 | 本阶段起有代码路径：`deploy` 域的 `stop` / `uninstall`，后者一次删净安装根目录 | `crates/app/wf-server/src/backends/deploy.rs:1545,1617` |

### 10.2 四个缺口（本轮处理结果）

1. **排空没有上限** → **已修复**。有界 drain（`server.rs:77-94`），默认 10s 可配，单测锁定。
2. **停机标记不拒新请求** → **已修复**。`is_server_stopping()`（`api/system/server.rs:117`）暴露停机位；`shutdown_guard_middleware`（`middleware.rs`）在停机期间对新执行启动（run/stream/dispatch/execute/rerun、chat POST）与新 SSE/WS（events/stream、attach、ws）返回 503，放行只读查询、health/status 与 cancel/pause/resume（收敛所需）。路径判定是纯函数 `is_new_workload`，由单测锁定（middleware.rs tests）。弥补了拍板"仅 503 + bounded drain"中缺失的 503 一半。
3. **agent 执行不在关闭收敛范围内** → **已修复**。`shutdown_executions()`（`context.rs:738`）覆盖 agent loop + workflow，落 `Cancelled`。
4. **隧道先于排空被杀** → **已修复**。`backends.shutdown()` 移到排空之后（`server.rs:88-91`）。

### 10.3 关闭顺序 【本轮落地 1/3/4，拍板已定】

每层有超时，任何一层超时都记日志继续往下，不阻塞后层。**实际实现顺序**（`ServerHandle::shutdown`，`server.rs:72-94`）：置停机标记（`:73`）→ 发 shutdown 信号（`:74`）→ 有界排空（`:77-94`）→ 关 backend 隧道（`:88-91`）。Runtime 侧另做执行收敛（`context.rs:738`、`shutdown.rs:51`）。

1. **拒新 + 通知（拍板：只做停机标记 + 503，不引入 `server.closing` 帧）**。`mark_server_stopping()`（`server.rs:73`）置原子位，`is_server_stopping()`（`api/system/server.rs:117`）供 `shutdown_guard_middleware`（`middleware.rs`）消费：停机期间对新执行提交与新 SSE/WS 返回 503，放行只读与 cancel/pause/resume。缺口 2 已修复。客户端通过 503 与 bounded drain 感知停机，不新增协议帧，避免旧前端解析未知帧。
2. **有界排空【已落地】**。`shutdown_drain_secs` 默认 10s（0–120 可配，`server_config.rs:63-65,225-240`），`with_drain`（`server.rs:55`）注入；`shutdown` 用 `timeout(drain, &mut task)` 包裹（`:77`），超时 abort 并告警（`:78-94`）。单测 `shutdown_is_bounded_by_drain_budget`（`server.rs:188`）验证挂起 SSE 不拖过预算。修复了缺口 1。
3. **收敛执行【已落地】**。`ApiContext::shutdown_executions`（`context.rs:738`）遍历 agent loop 与 workflow，非终态者先 `stop()` 再 `abort_task`，最后 `execution_tasks.abort_all()` + `stop_persistence_bridge()`；Runtime 停机改调它（`shutdown.rs:51`）。active-shutdown 标记使 settle 归类为 `Cancelled` 而非 `Failed`（`wf-common/src/shutdown.rs`、`coordinator/execution.rs:454-455`）。实体 `stop()` 落 `Cancelled` 有单测（`entity.rs:786`）。修复了缺口 3。
4. **关 PTY【已落地】**。`PtyManager::shutdown_all()`（`crates/app/wf-api/src/pty/state.rs`）在排空之后、关隧道之前调用（`server.rs` 的 `#[cfg(feature = "pty")]` 块），杀掉所有会话子进程（含 persistent），清空注册表。
5. **杀隧道【已落地，顺序已修正】**。在排空完成之后才 `backends.shutdown()`（`server.rs:88-91`），不再先于优雅停机。关闭前先等 `tunnel_stop_grace_secs`（默认 2，0–30 可配）让在飞备份收尾，再逐个 kill（`backends/tunnel.rs` 的 `shutdown(grace)`、`backends/config.rs`）。修复了缺口 4。
6. **关存储**：事件持久化 flush、检查点、sqlite。由 `shutdown_executions` 中的 `stop_persistence_bridge()`（`context.rs`）覆盖持久化桥，其余存储关闭沿用既有路径。

### 10.4 关闭"什么"：归属决定命运

| 对象 | 归属 | 关闭本地服务时 | 关闭会话时 |
|---|---|---|---|
| 执行（Run） | 本地进程 | 取消并落终态 | 按 detach 策略：解绑或中止 |
| 订阅（SSE） | 单个连接 | 推 closing 帧后关闭 | 关闭 |
| PTY 会话 | 本地进程，可持久 | 杀非 persistent | 只解绑 |
| SSH 隧道 | **后端条目**，进程级复用 | 回收 | **绝不触碰** |
| 远端 `wf-server` | 远端部署 | **不动** | 不动 |
| 本地 `wf-server` | 本机 | 自己退出 | 不动 |

关键判断：**SSH 连接属于"后端条目"，不属于"会话"**。每个后端只有一个常驻转发（`backends/tunnel.rs:22-26` 的 `live` 映射），它服务该后端下的所有会话与备份轮。任何"关闭会话"去杀隧道，都会误伤同后端的其他会话和正在跑的备份。

远端同理：关本地不该停远端。远端是独立部署、独立生命周期，停它必须是显式动词。本阶段已明确**不维护 `desiredState`**：当前部署没有开机自启，也没有收敛循环，显式 `stop` 后不会"停了又起"；部署是否成功由独立 `probe` 的 `verdict`/`checks` 验证（`docs/plan/remote-deploy-write-path.md`）。若未来引入自启，再重新评估是否需要期望状态，那是独立任务。

## 11. 可配与不可配

原则：**行为可配，不变量不可配**。

前四项**已落地**【已落地】，均支持文件层 `server.toml` 与环境变量两层，非法值启动失败。配置样例见 `config-examples/server/server.toml`（已同步至 `deploy/templates/server/server.toml`，`sync-deploy.sh --check` 通过）。

| 配置项 | 环境变量 | 默认 | 允许范围 | 状态 |
|---|---|---|---|---|
| `stream_detach_policy` | `WF_SERVER_STREAM_DETACH_POLICY` | `cancel` | `cancel` / `keep` | 已落地，全平台可配 |
| `run_detach_idle_secs` | `WF_SERVER_RUN_DETACH_IDLE_SECS` | 1800 | 300–86400 | 已落地（`server_config.rs:46,200-205`） |
| `run_detach_max_per_principal` | `WF_SERVER_RUN_DETACH_MAX_PER_PRINCIPAL` | 4 | 1–64 | 已落地（`server_config.rs:48,207-212`） |
| `shutdown_drain_secs` | `WF_SERVER_SHUTDOWN_DRAIN_SECS` | 10 | 0–120 | 已落地（`server_config.rs:65,225-240`） |
| `tunnel_stop_grace_secs` | `WF_BACKEND_TUNNEL_STOP_GRACE_SECS` | 2 | 0–30 | **已落地**（`backends/config.rs`，在 `backends.toml` 配置） |

**不可配**（写死在代码里并配测试）：

1. 关闭会话不得回收隧道。
2. 关闭本地服务不得停止远端服务。
3. 关闭过程中的执行落 `Cancelled`，不落 `Failed`。
4. 远端命令不接受自由 shell 字符串（部署域已按此实现，见 `backends/deploy.rs` 的通道构造）。
5. 任何排空与回收都必须有超时上限。
6. 回收必须先落终态再删实体。

## 12. 落地顺序与验收

顺序按"收益 ÷ 风险"排，每步独立可验证。**截至本轮全部前端/后端开放点已完成，仅部署作业化按拍板留作独立任务**：

1. **关闭路径修复**（缺口 1/2/3/4）— **已完成**：有界 drain + 503 拒新 + 执行收敛 + PTY 收敛 + 隧道顺序与 grace（第 10.3 节）。
2. **dispatch + attach + `stream_detach_policy`** — **已完成**：`keep` 全平台可配默认 `cancel`；CLI `wf agent-loop dispatch|attach`、Web 执行页"后台运行/重新连接"入口已接（第 6、7 节）。
3. **孤儿回收 + 容量记账** — **已完成**：按主体配额 + 常驻 reaper + 机会式兜底 + detached 独立指标（第 8、9 节）。
4. ~~`server.closing` 帧与前端配合~~ — **取消**：拍板采用 503 + bounded drain，不引入协议帧。
5. **部署作业化** — **按拍板留作独立任务**：`install`/`upgrade` 仍是同步 HTTP；停机落 `interrupted` 的交界另行处理。

验收要点（单测已覆盖的部分见文中 `文件:行号`，其余留待开发板/集成验证）：

* **关闭**：开着 SSE 与一个在跑执行时发 SIGTERM，观察是否在排空上限内退出（`shutdown_is_bounded_by_drain_budget` 已覆盖）、执行是否落 `Cancelled`（`entity.rs:786` 已覆盖）、隧道是否全部回收（待集成验证）。
* **保活**：断开客户端后执行继续、attach 能重放并收到终态帧（handler 单测待补）、超空闲阈值后被回收且释放许可（`detached.rs` 回收单测已覆盖）。
* **配额**：单主体反复 detach 到上限后被拒绝，且拒绝不影响已有执行（`detached.rs:330` 已覆盖）。
* **部署交界**：`install` 进行中关闭本地服务，远端进程应保持运行（依赖作业化，未做）。

## 13. 拍板结果与开放点处置

**三项核心拍板（首轮）**：

1. detached 执行配额口径 → **按主体**（API key 摘要分桶，无 key 落 anonymous）。已落地（`detached.rs:270`）。
2. `stream_detach_policy=keep` 开放范围 → **全平台可配，默认 cancel**。已落地（`server_config.rs:182`）。
3. 停机通知方式 → **仅 503 + bounded drain，不引入 `server.closing` 协议帧**。已落地（第 10.3 节）。

**本轮对 8 个开放点的决策与处置**：

| 开放点 | 决策 | 处置 |
|---|---|---|
| 停机 503 拒新 | 做 | **已落地**：`shutdown_guard_middleware` + `is_server_stopping` |
| 常驻 reaper | 做 | **已落地**：`spawn_reaper` + ServerHandle 生命周期接线 |
| detached 指标 | 做 | **已落地**：3 个 gauge/counter 复用 resource collector 导出 |
| attach 鉴权 | **保持 API key 头**，不引入一次性 ticket | **记录为已接受风险**：key 走请求头不进 URL，与既有事件流一致；ticket 属额外攻击面与改动量，本阶段不做 |
| `tunnel_stop_grace_secs` | 做 | **已落地**：默认 2s，0–30 可配 |
| PTY 关闭收敛 | 做 | **已落地**：`PtyManager::shutdown_all` 接入关闭顺序 |
| Web/CLI 入口 | **CLI + Web 都加** | **已落地**：`wf agent-loop dispatch\|attach`；Web 执行页"后台运行/重新连接"入口 |
| 部署作业化 | **留作独立任务** | 本轮不做；`install`/`upgrade` 仍同步 HTTP |

**仅存的独立任务**：部署作业化（`install`/`upgrade` 异步化、停机落 `interrupted`）。它与保活都要动"长操作的生命周期"，用户拍板单独成阶段，避免耦合面过大。

## 14. 本轮一并修复的既有问题

这些问题与保活/关闭改动无关，是验证过程中暴露的既有缺陷，本轮按"一并修复"处置。修复已在本文对应的 patch 中（`wf-agent-keepalive-shutdown.patch`）。

| 问题 | 根因（file:line 事实） | 修复 | 验证 |
|---|---|---|---|
| `wf-runtime` 的 6 个 keyword bootstrap 测试在 `--no-default-features` 下失败 | `bootstrap/tests.rs` 的 `mod keyword;` 无条件编译，但 `keyword_index()` 等功能由 `keyword` feature 提供 | `bootstrap/tests.rs:192` 加 `#[cfg(feature = "keyword")]` | `--no-default-features` 全绿 |
| `test_bootstrap_registers_mcp_manager_and_use_mcp` 在无 mcp feature 下失败 | `service_wiring.rs` 该测试依赖 `mcp_manager()`，无 feature 门控 | `service_wiring.rs:128` 加 `#[cfg(feature = "mcp")]` | 同上 |
| `test_llm_profiles_merge_with_programmatic_values_by_id` 断言 `explicit-a` 却得到 `file-a` | `bootstrap/config.rs:432,444` 调用 `merge_llm_profiles`/`merge_llm_provider_definitions` 时把 `file_*` 放在了 base 位，而这两个函数是 **base 优先**（`wf-config/src/llm.rs:114` `merge_records_by_id` 用 `entry().or_insert()`）；显式值应优先却被文件值覆盖 | 交换实参顺序：`std::mem::take(&mut config.llm.profiles)` 放 base 位，`file_profiles` 放 overlay 位；provider_definitions 同 | 该测试通过；`wf-config` 390✓ 回归无回退 |

`merge_auth_configs` 是 **overlay 优先**语义（`llm_auth.rs:143`），`config.rs:420` 的调用本就正确，未改。

**验收**（本轮实测）：`cargo test -p wf-runtime --lib --no-default-features` = 154✓ / 0 失败；加 `--features keyword,mcp` = 162✓ / 0 失败；`cargo test -p wf-config --lib` = 390✓。

~~**未修的独立既有问题**：默认特性集下 `wf-plugin` 的 lua 桥接层仍用 mlua 0.9 的带生命周期 API……留待独立的 lua 后端升级任务处理。~~ → **本轮已一并修复**，见第 15 节。

## 15. wf-plugin lua 桥接层迁移至 mlua 0.12（本轮完成）

用户追加要求"把 wf-plugin lua 桥接层完整修复，之后凡是发现的关键问题必须立刻修复"。本轮完成迁移，**默认特性集全工作区编译通过，不再需要 `--no-default-features` 规避**。

**根因**：`Cargo.toml:147` 锁 `mlua = "0.12"`（`Value`/`Table` 无生命周期参数；`Function::call`、`Table::get` 均降为单个泛型；hook 回调返回 `VmState`；`set_hook` 返回 `Result`），但 `wf-plugin/src/lua/` 仍按 0.9 API 编写。同仓 `wf-sandbox` 早已适配，可作为正确写法参照。

**迁移改动（4 个文件，file:line 为修复后位置）**：

| 文件 | 改动 | 处数 |
|---|---|---|
| `lua/bridge.rs` | `build_lua_context`/`to_lua_value`/`create_lua_handler_table`/`set_table_str`/`set_table_value` 去掉 `Value<'lua>`/`Table<'lua>` 生命周期参数 | 5 |
| `lua/handlers.rs` | `t.get::<&str, mlua::Value>` → `t.get::<mlua::Value>`（outputs/result） | 2 |
| `lua/plugin.rs` | `hook.call::<_, ()>` → `hook.call::<()>`；`get::<_, V>` → `get::<V>`（Option<Table>/Function/Option<String>）；`pairs`/`sequence_values` 签名未变不动 | 14 |
| `lua/pool.rs` | hook 回调 `Ok(())` → `Ok(mlua::VmState::Continue)` | 1 |

**过程中发现并立即修复的关键问题**：`lua/pool.rs` 的 `set_protection_hook` 原代码丢弃 `lua.set_hook(...)` 的返回值（0.9 返回 `()`，0.12 返回 `Result<()>`，编译器报 `unused Result`）。该 hook 是 Lua 超时/内存保护的唯一执行点，吞掉安装失败会让死循环脚本无法被中断——属正确性缺陷。修复为直接返回 `set_hook(...)` 的结果（`pool.rs:93`），安装失败现在经 `arm_hook` 的 `map_err` 上抛。

**回归测试**：新增 `crates/infra/wf-plugin/tests/mlua_bridge_smoke.rs`（3 例），填补仓库此前 lua 桥接层无端到端测试的缺口：
1. value 双向转换保持类型（string/number/float/bool/array/object + 顶层 null）；对象内 `null` 字段因 Lua `set(key, nil)` 即删键的固有语义无法往返，非迁移缺陷，不作处理。
2. `LuaNodeHandler` 跑真实 Lua 脚本，验证 `call::<R>`/`get::<V>` 的返回类型解析正确。
3. 保护 hook：正常执行放行（`VmState::Continue`）、过期 deadline 触发超时错误。

**验收（本轮实测）**：`cargo check --workspace`（默认特性）= 0 错误；`cargo test -p wf-plugin --all-features` = 160✓ + 3✓（smoke）；`cargo test -p wf-sandbox --all-features` = 212✓；`cargo test -p wf-runtime --lib`（默认特性）= 176✓（含此前 8 个失败测试）。
