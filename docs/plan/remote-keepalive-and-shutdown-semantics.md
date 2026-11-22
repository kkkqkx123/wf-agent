# 断连保活与关闭语义（下一阶段设计）

本文只做设计，不改代码。范围是"连接消失时执行怎么办"和"关闭服务时到底关什么"。写权限与远端部署已在 `docs/plan/remote-deploy-write-path.md` 落地，本文只在与它交界处补充衔接约束。所有现状判断附 `文件:行号`。

## 0. 结论

* **保活不是新机制，是接线**。引擎已有"后台执行 + 预注册 id + 终态落结果槽 + 可查询可取消"（`crates/engine/wf-agent/src/executor/spawn.rs:17-24`），事件已持久化并支持按游标回放（`crates/app/wf-server/src/api/system/events.rs:686-712`）。缺的是 dispatch、attach 两个端点和一套孤儿回收。
* **关闭要拆成三个动词**。`detach`（只解绑观察者）、`stop`（终止执行）、`close`（关服务）今天被压缩在"连接消失"一件事上。SSH 连接属于后端条目而不是会话，任何"关会话"的动作都不该碰它。
* **保活必须与容量记账同批上线**。许可挂在执行实体上、终态才释放（`crates/engine/wf-agent/src/registry.rs:68-71`）。断连不再中止执行之后，许可也不再随连接释放，反复断开重发就能占满并发位。没有配额，保活等于把"断连即释放"换成"断连即泄漏"。

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

## 6. 两个端点

**dispatch**：提交即返回执行 id，执行进入后台。语义直接复用 `spawn_agent_loop` 的预注册——返回后立刻可 query、可 cancel。响应带 id 与"已接受"标记，不带事件。

**attach**：按执行 id 订阅事件，先重放持久化的事件窗口，再接实时流，最后以终态帧收尾。要点：

1. **必须能处理"已经结束"**：回放 + 终态帧，不能 404。否则前端每次重连都要先分辨"结束"与"不存在"，重连逻辑会退化成猜。
2. **鉴权沿用一次性 ticket**，不把 key 放进 URL（与既有事件流一致）。
3. **游标语义复用现有 `since`**，不新增一套。客户端存最后一个事件的时间戳，重连即续传。

## 7. detach 策略

`stream_detach_policy = cancel | keep`：

| 取值 | 断连时 | 适用 |
|---|---|---|
| `cancel` | 中止执行（今天的默认） | 本地单用户、交互式问答 |
| `keep` | 解绑订阅者，执行继续 | 远端节点、长任务、移动端 |

**服务端是上限**：客户端可以在请求里声明偏好，但策略为 `cancel` 时声明无效。允许客户端单方面钉住服务端资源等于开放一个 DoS 面——这条不能配。

## 8. 孤儿回收

直接借 PTY 的判定式，只替换"空闲"的口径（`pty/state.rs:232-237` 是现成写法）：

* **不能用"没有事件"判空闲**。一次长工具调用可能几分钟不出事件，但它在正常推进。用"没有事件"判会杀掉正在干活的执行。
* 用"**最后事件时间 + 无人订阅**"判据，阈值比 PTY 大一个量级（建议默认 30 分钟，可配），`persistent`（用户显式标记的长任务）豁免。
* 兜底还有执行自身的墙钟上限（`crates/app/wf-api/src/agent/agent_execution.rs:14` 的 90s 默认值，或 loop 配置的 `max_execution_time`），所以保活不会无限占用。
* 回收动作是 `cancel` + `cleanup`，顺序不能反：先落终态再删实体，否则结果槽会出现"实体没了但结果还在"或反之。

## 9. 容量记账

这是最容易被漏掉的一条，也是保活能不能开的前提。

| 项 | 现状 | 保活后必须有 |
|---|---|---|
| 并发许可 | 挂在实体上，终态释放（`registry.rs:68-71`） | 不变，但释放时机从"断连"推迟到"终态"，因此需要配额 |
| 每主体 detached 上限 | 不存在 | 建议默认 4，超限拒绝新的 detach |
| 每主体在跑执行上限 | 不存在 | 与容量门配合，防单主体占满共享位 |
| 指标 | 有执行数指标 | detached 执行单独计量，否则容量问题看不见 |

没有这三样，保活就是把泄漏点从"连接"挪到"执行"。

## 10. 关闭语义

### 10.1 现状（按动作分）

| 动作 | 今天做了什么 | 证据 |
|---|---|---|
| 客户端断连 | SSE 计数减一、订阅取消；执行流被 drop 即 abort | `api/system/events.rs` 的流收尾、`infra/stream.rs:16-21` |
| 取消执行 | 停实体 + 中止两类任务句柄 | `infra/context.rs:605-610` |
| 关闭本地服务 | 标停（仅状态位）→ 取消探活/备份循环并杀全部 `ssh -N` → 发优雅停机 → axum 排空 → runtime 关闭 | `crates/app/wf-server/src/server.rs:45-56`、`backends/manager.rs:740-743`、`backends/tunnel.rs:208-216` |
| 关闭远端服务 | 本阶段起有代码路径：`deploy` 域的 `stop` / `uninstall`，后者一次删净安装根目录 | `crates/app/wf-server/src/backends/deploy.rs:1545,1617` |

### 10.2 四个缺口

1. **排空没有上限**。axum 的 graceful shutdown 只停止接受新连接并等待既有连接结束（`server.rs:74-83`），而 SSE 长连接不会自行结束，服务端也没有任何地方在这条路径上主动关流。等待时长实际由外部决定（systemd `TimeoutStopSec` 默认 90s 强杀）。表现是"重启一次服务，前端卡到超时"。
2. **停机标记不拒新请求**。`mark_server_stopping` 只写一个原子位（`api/system/server.rs:110-112`），除 `/server/status` 外没有消费者。排空期间新请求照收。
3. **agent 执行不在关闭收敛范围内**。`ctx.shutdown()` 只中止工作流驱动任务（`infra/context.rs:731-734` 的 `execution_tasks.abort_all()`），agent 循环任务不在其中，进程退出时被硬杀，可能留下"运行中"的执行记录。
4. **隧道先于排空被杀**。`backends.shutdown()` 在发送优雅停机信号之前执行（`server.rs:47-53`），正在进行的备份拉取会中途断开，而此时监听仍在，新请求进来只能看到"不可达"。

### 10.3 建议的关闭顺序

每层有超时，任何一层超时都记日志继续往下，不阻塞后层：

1. **拒新**：置停机位，中间件对新的执行提交与新的 SSE 连接返回 503（与 `backends` 域禁用时的 503 同构）。
2. **通知**：向所有 SSE 连接推一帧 `server.closing`，让客户端主动重连或进入"等待恢复"，而不是干等。
3. **有界排空**：给 axum 一个排空超时（建议 10s，可配），超时后强制结束剩余连接。这是缺口 1 的正面修复。
4. **收敛执行**：对所有非终态执行走一次 cancel，让它们以 `Cancelled` 落库而不是 `Failed`——这正是 `is_active_shutdown` 标记的用途（`crates/foundation/wf-common/src/shutdown.rs:1-9`，消费点 `crates/engine/wf-agent/src/coordinator/execution.rs:454-455`）。
5. **关 PTY**：杀掉非 persistent 会话的子进程，清理已退出终端（复用 `pty/state.rs:232-237` 的判定）。
6. **杀隧道**：此时已无在飞请求依赖转发。
7. **关存储**：事件持久化 flush、检查点、sqlite。

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

| 配置项 | 默认 | 允许范围 | 为什么可配 |
|---|---|---|---|
| `stream_detach_policy` | `cancel` | `cancel` / `keep` | 单用户本地与远端节点是两种产品语义 |
| `run_detach_idle_secs` | 1800 | 300–86400 | 长任务与短问答的容忍度不同 |
| `run_detach_max_per_principal` | 4 | 1–64 | 与容量门配合，防占位 |
| `shutdown_drain_secs` | 10 | 0–120 | 部署环境的 systemd 超时差异大 |
| `tunnel_stop_grace_secs` | 2 | 0–30 | 让在飞备份有机会收尾 |

**不可配**（写死在代码里并配测试）：

1. 关闭会话不得回收隧道。
2. 关闭本地服务不得停止远端服务。
3. 关闭过程中的执行落 `Cancelled`，不落 `Failed`。
4. 远端命令不接受自由 shell 字符串（部署域已按此实现，见 `backends/deploy.rs` 的通道构造）。
5. 任何排空与回收都必须有超时上限。
6. 回收必须先落终态再删实体。

## 12. 落地顺序与验收

顺序按"收益 ÷ 风险"排，每步独立可验证：

1. **关闭路径修复**（缺口 1/3/4）：不涉及新接口，收益最大。
2. **dispatch + attach + `stream_detach_policy`**，先只在单用户 profile 打开 `keep`。
3. **孤儿回收 + 容量记账**，与第 2 步同批上线，否则第 2 步不能开。
4. **`server.closing` 帧与前端配合**：属协议新增，需要处理旧前端忽略该帧的情况。
5. **部署作业化**：当前的 `install`/`upgrade` 是同步 HTTP，分钟级操作会占住一个请求；作业化之后与关闭流程的交界处才清晰——作业在停机时应落 `interrupted`，而不是 `failed`。

验收要点：

* **关闭**：开着 SSE 与一个在跑执行时发 SIGTERM，观察是否在排空上限内退出、执行是否落 `Cancelled`、隧道是否全部回收（`tunnel.rs` 已有回收单测可扩）。
* **保活**：断开客户端后执行继续（事件持续增长）、attach 能重放并收到终态帧、超空闲阈值后被回收且释放许可。
* **配额**：单主体反复 detach 到上限后被拒绝，且拒绝不影响已有执行。
* **部署交界**：`install` 进行中关闭本地服务，远端进程应保持运行，作业状态落 `interrupted`。

## 13. 待拍板

1. `keep` 是否只对本地单用户 profile 开放，还是远端节点也允许（影响信任模型与配额口径）。
2. detached 执行的配额口径：按主体、按目录，还是全局共享容量门。
3. 排空上限超时后，是"强制断开 SSE"还是"转成后台执行再断"——后者更友好但会把保活引入关闭路径。
4. `server.closing` 帧是否值得引入协议变更，还是只靠 503 + 客户端重试。
5. 部署作业化与保活谁先做：两者都要动"长操作的生命周期"，合并做可以少一次改造，但耦合面更大。
