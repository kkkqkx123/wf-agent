# 远端服务维护差异分析（对照 dsh-ssh）

本文对照 `dsh-ssh`（前后端分离 agent 框架 dsh 的 SSH 插件）的远端维护实现，逐条核对 `wf-agent` 当前 `backends` 域的维护能力，给出差异定性与可借鉴项。所有结论附 `文件:行号`，引用路径相对仓库根；参考项目实现按 `dsh-ssh/lib/...` 标注，均已在本机浅克隆后逐行核验，非转述。

## 落地状态

写权限部分已落地，方案未采用本文当时设想的 systemd 单元路线，而是按 Zed / VS Code 的用户态版本目录模型实现：安装落在远端家目录，不带 root、不写系统单元、不碰包管理器；动词为 `probe`/`install`/`start`/`stop`/`upgrade`/`uninstall`，并已提供 HTTP、CLI 与 Web 三个入口。第二轮调整了三处产品语义：产物默认由远端下载发布包（本地上传变为显式选项）、配置与脚本从管理端部署源目录按 `config`/`full` 范围传输、卸载一次性删除整个安装根目录；同时明确**不维护 `desiredState`**，用独立 `probe` 的判定与检查项承担部署验证。设计细节见 `docs/plan/remote-deploy-write-path.md`。

因此本文第 3 节之后的“差距”与“建议”按**落地前状态**保留，用于解释动机；其中“远端进程不受监管”“管理面是否获得写权限”两处已被上述实现取代。断连保活与关闭语义仍未落地，设计见 `docs/plan/remote-keepalive-and-shutdown-semantics.md`。

## 0. 结论摘要

三句话：

* 落地前 `wf-agent` 的远端维护止步于“连接参考 + 探活 + 备份”，**远端进程本身不在任何一方的生命周期内**：不安装、不守护、不升级、不重启，只打印人工命令。这不是遗漏，而是当时 `docs/features/server-handling-and-discovery.md` 自述的边界；写权限落地后，安装、启停、升级、卸载已由管理面驱动，但开机自启与状态收敛仍未做。
* 参考项目在进程维护上做的三件事（守护、版本切换、健康等待）可直接补上，且不必照搬它的“每工作区一个 unit”模型。
* 最本质的差异不在维护，而在**断连语义**：`wf-agent` 侧连接一断，在飞任务随流一起中止；参考项目刻意让远端任务活下去并可续接。这条差异影响产品形态，不能只当实现细节看。

## 1. wf-agent 现状基线

### 1.1 职责切分

远端模式是“执行节点 + 本地管理面”：远端只跑 `wf-server`，本地管理面持有注册表、隧道、备份索引（`docs/features/remote/README.md:3`）。两个 profile 编译期互斥：`remote-node` 与 `backends` 同时启用时启动即退出（`crates/app/wf-server/src/main.rs:133-137`），从根上禁止“远端再管一层远端”。

### 1.2 连接与隧道

* 隧道是**子进程**，不是 SSH 库：每个后端最多一个常驻 `ssh -N`，作为该主机的 ControlMaster（`crates/app/wf-server/src/backends/tunnel.rs:41-72`），`ControlPersist=no`（`tunnel.rs:60`），`kill_on_drop`（`tunnel.rs:115`）。
* 启动前做同名检查与本地端口绑定试探，冲突直接拒绝，不静默换端口（`tunnel.rs:96-105`）。
* 启动后只等 100ms 就 `try_wait` 判活，早退即报错（`tunnel.rs:118-133`）。
* 异常退出只记中断原因，**不自动重启**（`tunnel.rs:167-190`，设计声明见 `crates/app/wf-server/src/backends/types.rs:144-145`）。
* 服务关闭时全量回收转发（`tunnel.rs:208-216`，调用点 `crates/app/wf-server/src/server.rs:45-55`）。

### 1.3 探活与状态

* 后台轮按 `probe_interval_secs`（默认 30s，`crates/app/wf-server/src/backends/config.rs:22`）定时跑，`MissedTickBehavior::Skip`（`manager.rs:787-800`）。
* 一轮内按 16 条分片并发，单条独立超时（默认 5s，`config.rs:24`），先 TCP 连**本地回环端口**，再 `/health`，再 `/api/v1/info` 比对 `apiVersion`（`crates/app/wf-server/src/backends/probe.rs:14-62`）。
* 状态存内存 `DashMap`（`manager.rs:136`），带 `last_probe_ms`（`probe.rs:33`），**进程重启即清空**。

### 1.4 远端进程

* 只打印命令不执行：拉起命令走 slave 连接复用 master（`crates/app/wf-client/src/remote.rs:104-140`）。
* 机器侧有完整打包与单元模板：`deploy/systemd/wf-server.service.template`、`wf-server-personal.service.template`、deb/rpm（`deploy/README.md:210-216`），但安装是一次性的人工步骤（`deploy/scripts/install-server.sh`）。
* 单元策略是 `Restart=on-failure`（两个模板分别为第 30、28 行），个人单元仅以注释提示 `loginctl enable-linger`，不自动开启。

### 1.5 前端可见性

* 管理端提供了完整 CRUD 与隧道开关接口（`crates/app/wf-server/src/api/system/backends.rs:20-53`）。
* Web 前端只有只读映射：`listBackends()` 打 `GET /api/v1/system/backends`，503 与网络错误降级为 `null`（`apps/web-app/src/lib/services/backends.ts:40-55`），注释明确“注册是 CLI 操作”。
* **该函数当前没有任何 UI 调用点**，仅被自身测试引用（`apps/web-app/src/lib/services/backends.test.ts`）；`apps/web-app/src/routes/` 下也没有服务器/远端页面。
* 前端只有一个基址，`VITE_API_BASE_URL` 或同源（`apps/web-app/src/lib/api/client.ts:11`），无多连接管理。

### 1.6 断连语义

* 流式执行由 `ExecutionEventStream` 承载，注释写死语义：**drop 流即中止驱动任务**，避免消费者断开后仍跑满超时（`crates/app/wf-api/src/infra/stream.rs:16-21`）。
* 非流式 `run` 直接在 handler 内 `await`，未 spawn 独立任务（`crates/app/wf-server/src/api/agent/loops/lifecycle.rs:260-282`），执行生命周期与请求 future 绑定。
* 反例同在仓内：PTY 会话在服务端存活，连接断开只是解绑订阅者（`crates/app/wf-server/src/api/pty.rs:463`），积压溢出时以关闭码 4008 让客户端带游标重连。

## 2. dsh-ssh 基线（核验点）

* **远端守护**：优先 `systemctl --user/systemctl` 启停（`lib/remote/service.js:11-21`），无 systemd 时 `setsid nohup` 兜底（`lib/remote/service.js:94-113`）；user unit 由 `loginctl enable-linger` 保活（`lib/remote/deploy.js:273`），无 systemd 时写 `@reboot` crontab（`lib/remote/deploy.js:286`）。
* **重启策略**：`Restart=always` + `RestartSec=3`（`lib/remote/units.js:35-37`），与 wf-agent 的 `on-failure` 不同。
* **版本切换**：`ensureWorkspaceRuntime` 比对工作区记录的 node/dsh 路径与版本，变了就补丁记录、重写启停脚本、重写 unit 的 `ExecStart` 并 `daemon-reload`（`lib/remote/service.js:31-67`）；`refreshWorkspaceRuntime` 只切到已装好的运行时，不顺手安装（`service.js:74-86`）。
* **健康等待**：启动后轮询直到 HTTP 就绪（401 也算就绪，说明端口已监听），单元进入 failed 才提前失败，并附日志尾部（`service.js:119-142`）。
* **探活在远端侧执行**：`curl http://127.0.0.1:<port>` 通过 SSH exec 在远端跑（`lib/remote/shared.js:85-99`），不经本地端口转发。
* **轮询**：`setTimeout` 递归调度，间隔每次重读设置并夹在 5s 下限（`lib/index.js:233-238`），并发上限 2（`lib/index.js:215`），跳过繁忙工作区（`lib/index.js:180`），启动首轮把陈旧可达性重置为 `connecting`（`lib/index.js:162-171`），定时器 `unref` 以免拖住进程退出（`lib/index.js:237`）。
* **前端发现**：一个 `overview` 聚合宿主、工作区、设置（`lib/routes/hosts.js:27-34`）；原生工作区行由后端注册进 `workspaceRegistry` 的锚点目录投影而来（`lib/native.js:15,38-59`），前端用 `MutationObserver` + 5s 定时器扫描 `[data-row-key^="workspace:"]` 原地装饰（`lib/client/86-row-decoration.js:170-173,212,219`）。
* **关闭语义**：`dispose()` 逐个 `detach({ close: false, killSdk: false })`，在飞 turn 留在远端继续，只回收空闲预热 runner（`lib/bridge/lifecycle.js:402-418`，关键行 408）。

## 3. 差异矩阵

| 维度 | wf-agent（证据） | dsh-ssh（证据） | 差异性质 |
|---|---|---|---|
| 远端进程归属 | 落地前只打印命令（`remote.rs:104-140`）；现由部署动词驱动，无自启与收敛 | 安装/启停/重启/删除全由本地驱动（`service.js:88-195`） | 已补主路径，余自启与收敛 |
| 开机自启 | 单元模板存在，需人工 enable（`deploy/README.md:106-110`） | linger + `@reboot` 兜底（`deploy.js:273,286`） | 运维负担 |
| 重启策略 | `Restart=on-failure`（模板第 30/28 行） | `Restart=always`（`units.js:36`） | 语义差异 |
| 升级/版本切换 | `apiVersion` 不符即记不可达（`probe.rs:58-60`），无切换路径 | 运行时目录 + 重写 `ExecStart`（`service.js:31-67`） | 能力缺口 |
| 探活位置 | 经本地回环端口，依赖隧道（`probe.rs:49-56`） | 远端侧 `curl`，不依赖转发（`shared.js:85-99`） | 观测语义差异 |
| 探活节奏 | 30s，分片 16 并发（`config.rs:22`、`manager.rs:856`） | 5s 下限，并发 2（`index.js:233,215`） | 参数与取舍 |
| 状态存储 | 内存 `DashMap`，重启清零（`manager.rs:136`） | 写入宿主记录，重启首轮重置为 connecting（`index.js:162-171`） | 易失性差异 |
| 前端可见性 | 只读服务已就绪但无 UI 调用点（`backends.ts:40-55`） | overview + 原生行装饰（`hosts.js:27`、`86-row-decoration.js:170-173`） | 未落地 |
| 断连后任务 | 流 drop 即中止（`stream.rs:18-20`） | 刻意保活并可续接（`lifecycle.js:408`） | 产品语义差异 |
| 任务续接 | 需显式从检查点恢复（`wf-api/.../agent_execution/checkpoint.rs:70+`） | 写 offset/turn/step，重开自动续（`lifecycle.js:408`） | 能力缺口 |
| 传输与端口 | 回环端口 + HTTP，需端口编排（`docs/.../server-handling-and-discovery.md:24-29`） | SSH 通道承载，无本地端口 | 架构代价 |
| 关闭语义 | 全量杀隧道（`server.rs:45-55`） | 只关连接，远端不动（`lifecycle.js:408`） | 方向相反 |
| 多工作区模型 | 单单元服务多目录，靠 `x-wf-directory` 归属 | 每工作区一个 unit + 锚点目录 | 模型优劣互见 |
| 凭据 | 只存 `key_path`，不存密钥材料（`types.rs:9,15`） | 后端 store / OS 钥匙串 | 基本一致 |

## 4. 关键差异剖析

### 4.1 探活把“服务不可达”和“隧道坏了”混成一个布尔

`probe_one` 第一步就是连 `127.0.0.1:<local_port>`（`probe.rs:49`），隧道一断，全部条目同时转不可达，备份轮也会连带失败（`manager.rs:883-891` 先取远端清单）。而远端进程其实好好的——参考项目把探测命令送到远端执行（`shared.js:85-99`），观测的是服务本身，不是传输。两者不是对错，是**观测对象不同**：wf-agent 现在观测的是“我能不能连上它”，而不是“它活着没有”。在“隧道要人工 `remote up` 重建、不自动重启”（`docs/features/remote/error-handling.md:55`）的前提下，这两种故障在视图上完全无法区分。

### 4.2 远端进程无人负责，且缺口已被文档自述

`server-handling-and-discovery.md:55` 明写“远端服务进程本身不在服务端监管内，拉起、升级、多服务端口规划仍靠人工命令”。机器侧资产并不缺：单元模板、env 模板、deb/rpm、`install-server.sh` 都在（`deploy/`），缺的是**从管理面调用它们的动作**，以及调用后的结果回写。参考项目把这套动作做成一个 job（安装/重装/启动/停止/删除），并把 `desiredState` 存进工作区记录（`service.js:114,153,180,193`）——这正是 wf-agent 注册表里缺的一列。

### 4.3 升级路径缺失，被 `apiVersion` 硬拒放大成“停机升级”

探测把 `apiVersion` 不一致直接判不可达（`probe.rs:58-60`），CLI 连接也以配置错误终止（`docs/.../remote-mode-best-practices-plan.md:17-19`）。这是正确的“不静默降级”，但结果是：**只要一端升级，另一端在人工重装前一直是不可达**。参考项目的做法是把运行时按版本装进独立目录、让工作区记录指向具体版本，再重写单元 `ExecStart`（`service.js:31-67`），于是“升级”退化为一次指针切换加健康等待（`service.js:119-142`）。wf-agent 要复制的是“切换 + 等待”这两步，不是它的目录布局。

### 4.4 断连即中止，与仓内 PTY 的做法自相矛盾

`stream.rs:18-21` 的注释是明确的取舍：消费者走了就别再烧算力。但同一个服务里，PTY 会话在断连后继续存活并允许带游标重连（`pty.rs:463`）。也就是说，项目已经具备“服务端会话 + 订阅者解绑”的基础设施与先例，只是执行路径没有采用它。参考项目的 `detach({ close: false })`（`lifecycle.js:408`）解决的是同一类问题，只是它连“重新接上”的位点也落了盘。这条差异不是“缺一个字段”，而是要先决定产品语义（见 7.2）。

### 4.5 前端发现能力已备好但没接线

后端接口齐全（`api/system/backends.rs:20-53`），前端映射与降级策略也写好了（`backends.ts:40-55`），却没有调用点，也没有路由页。参考项目的 `overview` 聚合（`hosts.js:27-34`）解决的是“设备页与侧栏各自拉三次”的浪费；wf-agent 目前更极端——**一次都不拉**。这里要补的最小闭环是“列表 + 状态 + 隧道开关”，不必先做参考项目那种原地装饰式的原生行投影，因为 wf-agent 的侧栏不是宿主提供的原生列表。

### 4.6 状态易失，视图缺“从未探测”与“陈旧”语义

状态只在内存（`manager.rs:136`），服务重启后所有行都以默认值呈现，`reachable: false` 同时表示“刚探过是死的”和“还没探过”。参考项目在启动首轮把所有未停用宿主显式置为 `connecting`（`index.js:162-171`），这是一种低成本的三态化。wf-agent 已经有 `last_probe_ms`（`probe.rs:33`），补语义不需要新字段。

### 4.7 关闭语义方向相反，但这次 wf-agent 是对的

wf-agent 关服务即杀全部 `ssh -N`（`tunnel.rs:208-216`），理由是“不留孤儿转发”；远端 `wf-server` 不受影响，因为它本来就不由本地拉起。参考项目关闭本地应用时保留远端任务（`lifecycle.js:408`），因为它把远端当作本地会话的延伸。两者不冲突：**转发属于本地进程，必须跟着走；远端任务属于远端，是否跟着走是产品选择**。wf-agent 当前的“转发严格回收 + 远端不受影响”是干净的分层，不需要改。

### 4.8 单单元多目录 vs 每工作区一单元

wf-agent 一个 `wf-server` 服务所有业务目录，归属靠请求头（`deploy/README.md:65-70`）；参考项目一个工作区一个 unit（`service.js:49-55`）。前者省进程、省内存、升级一次到位；后者隔离好、可按工作区停启、可并存多版本。**建议保留 wf-agent 的模型**，把差异吸收到“运行时版本 + 单元重写”这一层，而不是引入 unit 爆炸。

## 5. 可借鉴项

### 5.1 可直接借（低风险，纯本地改动）

| 项 | 做法 | 落点 |
|---|---|---|
| 探测双通道 | 隧道存活与远端健康分开判定；隧道断记 `transport_failed`，隧道通但健康失败记 `unreachable` | `backends/probe.rs`、`backends/types.rs` 的状态视图 |
| 状态三态化 | 用 `last_probe_ms` 区分“从未探测/探测中/已探测”，启动首轮置“探测中” | `backends/manager.rs` 探活轮首轮、`api/system/backends.rs` 视图 |
| 健康等待替代 100ms 判死 | 启动转发后按短间隔重试到端口可用或超时，失败附最后一行 ssh 错误 | `backends/tunnel.rs:118-133` |
| 轮询节流可配 | 分片并发与单条超时已可配，补“跳过正在备份/正忙的条目” | `manager.rs:845-865` |
| 聚合视图 | 列表 + 状态 + 隧道态 + 备份错误合并成一个响应，前端一次取 | `api/system/backends.rs:20-53` |
| 前端接线 | 把已有 `listBackends()` 接到设置页或侧栏区段，先只读 | `apps/web-app/src/lib/services/backends.ts:40-55` |

### 5.2 需设计后借（中风险，跨进程）

| 项 | 做法 | 前提 |
|---|---|---|
| 远端服务监管 | 管理面下发安装/启停/升级/卸载，用户态目录不依赖 root；已明确不维护 `desiredState`，由 `probe` 做一次性验证 | 主路径已落地；自动收敛不在范围内 |
| 版本切换 | 运行时按版本分目录，注册表记录版本；切换 = 改指向 + 重写单元 + 健康等待；`apiVersion` 不符从“不可达”降级为“可升级” | 需要远端具备写单元权限 |
| 开机自启 | 个人单元用 user unit + linger；无 systemd 时 `@reboot` 兜底 | 与 5.2 第一项同批 |
| 断连保活 | 执行改为“服务端持有 + 订阅者解绑”，复用 PTY 已有的 detach 模式与游标语义 | 见 7.2 的产品决策 |

### 5.3 不建议借

* **递归管理面**：参考项目远端本身也是完整 dsh；wf-agent 已用 feature 互斥禁止（`main.rs:133-137`），这条边界应保留。
* **每工作区一 unit + 锚点目录投影**：模型的收益建立在“宿主提供原生工作区列表”之上，wf-agent 没有这个宿主。
* **Electron 双载体前缀**：wf-agent 只有 Web 载体（`client.ts:11`），`dsh-app:` 分支无对应物。
* **5s 全局轮询**：参考项目面向“几十台设备 + 侧栏实时感”；wf-agent 的后端条目是运维资产，30s 足够，降间隔只会放大隧道与远端负载。

## 6. 与既有规划的关系

* `docs/plan/remote-mode-best-practices-plan.md` 已解决版本握手、连接复用、信任分级、同步端点（第 53-65 行自述全部落地），本文的“版本切换”是它的自然延伸，不冲突。
* `docs/plan/remote-backup-hardening.md` 覆盖备份链路的正确性与持久性（第 9-39 行问题清单），**不覆盖**进程生命周期、断连语义与前端发现，本文的 4.1、4.2、4.4、4.5 属于该清单之外的空档。
* `docs/features/server-handling-and-discovery.md:49-55` 的“剩余缺口”已列出远端进程、端口编排、唯一管理者三条，本文对这些条目给出的是“可借什么”，不是重新发现。

## 7. 待拍板开放点

1. **管理面是否获得远端写权限**：已决定获得，但避开 systemd 与 sudo——改为用户态版本目录，因此“无 sudo / 无 systemd”不再需要降级阶梯，代价是没有开机自启。
2. **断连语义**：中止（现状）与保活（参考项目）影响计费、审批交互、沙箱回收与并发上限。保活必须同时定义“谁有权限停它”和“多久算孤儿”。
3. **探测口径**：保留“经隧道”的观测（等价于用户实际路径），还是增加“远端侧”的观测（等价于服务健康）。两者都给最诚实，但要多一条 SSH exec 通道。
4. **状态是否落库**：内存态重启清零，代价是重启后一轮全量探测与备份集中到期（`remote-backup-hardening.md:22` 已记录同类问题）。落库可与 `backends.db` 合并。
5. **前端是否承接多远端**：接线成只读状态区，还是做成可切换的连接目标。后者会牵动 `client.ts` 的单基址假设与鉴权模型。
6. **重启策略**：`on-failure` 是否改 `always`。改了就与“异常退出只记原因、不自动重启”的隧道策略不一致，需要区分“服务自身崩溃”与“运维主动停”。

## 8. 验证方式

* 探活与状态语义：`cargo test -p wf-server backends` 现有用例覆盖端口拒绝、黑洞端口、健康桩（`probe.rs:152-205`）；新增双通道用例可沿用同样的回环桩。
* 隧道启动等待：现有用例只覆盖端口占用与关闭回收（`tunnel.rs:253-289`），健康等待需补“启动后立即退出”与“延迟可用”两条。
* 部署模板漂移：`bash scripts/sync-deploy.sh --check`（`AGENTS.md:68`）。
* 编译面：`cargo check -p wf-server` 默认档、`--no-default-features --features remote-node`、`--all-features` 三组（对照 `remote-mode-best-practices-plan.md:48`）。
* 当前 HEAD（`3b17c295`）实测：`cargo check -p wf-types` 通过；`cargo check -p wf-server --no-default-features --features backends` 通过（37 条警告）；产品默认档在 `wf-plugin` 的 lua 绑定层报 36 条类型错误，原因是该提交把 `mlua` 从 0.9 提到 0.12，而句柄上的生命周期参数尚未迁移。与工具链版本无关，验证本文改动时用最小档即可。
* 跨机行为（人工）：一台上跑 `wf-server`，管理面 `remote up` 后停隧道观察状态区分；重启远端机器后确认没有自启、用 `remote deploy --action probe` 验证判定为 `absent`，且管理面不触发任何重建。

## 9. 核验清单

| 声明 | 证据 |
|---|---|
| 远端进程监管（落地前） | `docs/features/server-handling-and-discovery.md` 的原“剩余缺口”条目；`remote.rs:104-140` |
| 隧道不自动重启 | `tunnel.rs:167-190`；`types.rs:144-145`；`error-handling.md:55` |
| 探活经本地端口 | `probe.rs:49-56` |
| `apiVersion` 不符即不可达 | `probe.rs:58-60` |
| 状态在内存 | `manager.rs:136` |
| 流断开即中止 | `stream.rs:16-21` |
| PTY 断连只解绑 | `pty.rs:463` |
| 前端未接线 | `backends.ts:40-55`；`backends.test.ts` 为唯一引用 |
| 档位互斥 | `main.rs:133-137` |
| 参考项目守护与自启 | `service.js:11-21,94-113`；`deploy.js:273,286`；`units.js:35-37` |
| 参考项目版本切换 | `service.js:31-67` |
| 参考项目远端侧探测 | `shared.js:85-99` |
| 参考项目断连保活 | `lifecycle.js:402-418` |
| 参考项目前端发现 | `hosts.js:27-34`；`86-row-decoration.js:170-173`；`native.js:15,38-59` |
