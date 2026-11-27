# 09 进程生命周期与双节点增量测试

> 范围：`wf-server` 的真实 SIGTERM、SSH 隧道子进程、PTY 子进程收敛，以及基于隧道与节点重启的双节点新场景
> 关系：承接 `docs/plan/wf-server-test-coverage-next-round.md` 中“暂不补充”的四项；不重复 `08` 已覆盖的直连备份与恢复链路

## 结论先行

真实信号、SSH 与双节点脚本不进入默认 `cargo test` 门禁，也不堆进 OpenAPI 契约测试；唯一进入 Rust 集成门禁的是单个真实 PTY 子进程用例。四类场景共同依赖真实进程、信号、端口或子进程收敛，适合两层分工：一层是 Rust 集成测试，验证库内能稳定观测的帧、状态与子进程回收；另一层是独立端到端脚本，验证真实二进制收到 SIGTERM 后没有遗留子进程，并给出可供文本审核的归一化报告。

新增载体集中为三个测试入口和一个公共辅助：

| 载体 | 位置 | 职责 |
|---|---|---|
| 单个真实 PTY 集成用例 | `crates/app/wf-server/tests/pty_process.rs` | 真实子进程的帧、写入回显、游标重放、服务关停后回收 |
| 进程生命周期脚本 | `e2e/run-process-lifecycle.sh` | 真实 SIGTERM、SSH 隧道子进程、PTY 子进程收敛 |
| 隧道双节点脚本 | `e2e/run-remote-backup-over-tunnel.sh` | 经 SSH 隧道的备份与索引读回、执行节点重启后重新探活 |
| 公共辅助 | `e2e/lib/process_lifecycle.sh` | 进程存活、归一化投影、报告提升，不复制产品逻辑 |

单元测试只补纯函数缝隙，不再新增模拟信号或模拟 SSH 的测试替身。`tunnel.rs` 已有的 argv、端口占用与关停回收单测，`api/pty.rs` 已有的握手鉴权单测，`server.rs` 已有的 drain 上限单测保持原样；它们约束参数和纯逻辑，真实链路由本篇新增载体约束。

## 现有覆盖与缺口

| 场景 | 已有守卫 | 缺口 |
|---|---|---|
| SIGTERM | `main.rs` 的 Unix signal 分支，`ServerHandle::shutdown` 的顺序与 drain 上限单测 | 没有真实进程从信号到退出的证据，也没有验证退出时 SSH 与 PTY 子进程是否都被回收 |
| SSH | `backends/tunnel.rs` 的 argv 形状、端口冲突、关停清理单测；状态视图字段映射 | 没有让监督器真正执行 argv，也没有异常退出后的 `interrupted_reason` 与关停后子进程消失的端到端证据 |
| PTY | `wf-api/src/pty` 的状态、缓冲、订阅与帧映射单测；握手鉴权单测 | 没有真实 `portable_pty` 子进程的输出、写入、游标重放与关停收敛 |
| 双节点 | `tests/backup_recovery_nodes.rs` 与 `run-remote-backup.sh` 的直连备份恢复 | 没有走服务端拥有的 SSH 隧道，也没有执行节点关停重启后管理端重新探活的链路 |

`08` 仍然只负责直连链路。本篇新增的双节点脚本复用其判据语义，但请求路径经隧道转发，并增加节点重启分支。

## 全局约束

- 不连接真实 SSH 服务，不访问外网，不使用密钥材料。SSH 行为由版本化的假 `ssh` 固件承担，只在回环地址转发 TCP。
- 不依赖 `pkill` 或进程名匹配。脚本只使用自己记录的 PID；子进程必须由服务端关停流程回收，脚本只负责在失败清理时补杀服务端主进程。
- 不比较原始日志或原始 WebSocket 字节。原始产物留在 `outputs/actual`，人工审核的是归一化文本报告，提升后进入 `outputs/golden`。
- 两个脚本都启动默认 feature 的服务端二进制。`remote-node` 与 `backends` 编译期互斥，因此不在本篇切换 feature 组合；执行节点同样使用默认二进制，仅用环境变量打开恢复写入路由。
- 所有等待使用带上限轮询，不用固定睡眠表达“已经收敛”。假 `ssh` 的启动仲裁仍由产品代码的短健康窗口负责，测试只观察结果。
- 日志统一用 `--log-level info --log-format json` 启动，投影器只保留稳定消息，时间戳、PID、端口、绝对路径不进入黄金报告。

## 场景一：SIGTERM

### 放置

放在 `e2e/run-process-lifecycle.sh` 的 `sigterm` 段。这里必须启动真实二进制：单测里的 `ServerHandle::shutdown` 是有意调用关停，不能证明 Unix 信号被正确接入；`main.rs` 的 signal 分支只能通过向进程发信号触发。

### 前置

- 启动管理节点：开启后端域，设置 `WF_BACKEND_TUNNEL_STOP_GRACE_SECS=0` 缩短关停等待，设置探针间隔一秒，传入假 `ssh` 所在的 `fixtures` 目录作为 `PATH` 前缀。
- 启动一个持久 PTY 会话，命令指向版本化固件，保证关停前子进程仍在运行。
- 注册一个回环后端并启动隧道，记录假 `ssh` 写出的 PID 文件。
- 读取 `/api/v1/server/status`，确认返回的 PID 与脚本启动的 PID 一致，避免误杀同机其他进程。

### 步骤与判定

1. 向服务端 PID 发送 `SIGTERM`。
2. 轮询进程退出，`wait` 的退出码必须为零。非零退出说明信号走了默认终止路径而不是关停分支。
3. 进程退出后健康端点不可连接，原 PID 不再存活。
4. 日志投影必须包含信号接收消息和 PTY 回收消息；PTY 回收消息带计数，计数至少为一。
5. 假 `ssh` PID 文件中的进程不再存活；创建 PTY 时记录的 PID 也不再存活。
6. 不在此处断言 drain 窗口内的 `stopped=true` 或精确 drain 时长。前者有竞态，后者已有 `ServerHandle` 单测约束；进程脚本只关心最终收敛。

报告落到 `e2e/outputs/actual/process-lifecycle/sigterm.report.txt`，人工确认后提升为同名黄金文件。

## 场景二：SSH 隧道子进程

### 放置

- 参数形状、端口冲突与异常退出原因仍由 `backends/tunnel.rs` 单测守卫。
- 真实执行 argv、状态视图、显式停止和异常退出由 `e2e/run-process-lifecycle.sh` 的 `ssh` 段守卫。
- 经隧道承载真实 HTTP 流量的部分放到双节点脚本，避免同一脚本同时承担进程收敛与备份状态机。

### 固件行为

`e2e/fixtures/process-lifecycle/bin/ssh` 是一个可执行固件，由 `python3` 解释，行为限定为：

- 从 argv 中解析 `-L local:127.0.0.1:remote`，把收到的完整 argv 追加到指定日志目录；日志只写参数，不写任何凭据。
- 用当前进程号写出 `ssh-<local_port>.pid`，供脚本判定服务关停后子进程是否仍存活。
- 在 `127.0.0.1:<local_port>` 监听，把每个连接透明转发到 `127.0.0.1:<remote_port>`；每个连接独立处理，服务端重启后无需重启转发器。
- 模式由环境变量控制：`forward` 保持转发，`exit-later` 在写日志并越过监督器的启动健康窗口后以固定非零码退出，用于异常退出断言；不能立即退出，否则启动路径会先报错，测不到运行后的复核状态。
- 不解析 `ControlPath`，不创建 socket 文件；该参数只参与 argv 文本投影。

固定输出中的 `ControlPath`、PID、端口与目标主机都用占位符替换，避免本机目录和动态端口造成黄金漂移。

### 判定

- 创建后端后启动隧道，返回包络给出 PID 与 argv。PID 与固件写出的 PID 一致；argv 投影保留 `ssh -N`、端口转发规格、`ControlMaster=yes`、`ControlPersist=no` 与目标形态。
- 后端列表中的 `tunnel.running=true`，`pid` 非空，PID 文件对应进程存活。
- 显式停止后返回 `stopped=true`，状态视图不再报运行中，PID 对应进程消失。
- 用 `exit-later` 固件启动另一条目，等待一个探活与复核周期后，状态视图报未运行，且 `interrupted_reason` 包含固件退出码。服务端不自动重启该隧道。
- SIGTERM 段的关停回收复用同一 PID 文件判定，不重复启动失败隧道。

报告落到 `e2e/outputs/actual/process-lifecycle/ssh-tunnel.report.txt`。

## 场景三：PTY 子进程

### 放置

`crates/app/wf-server/tests/pty_process.rs` 只放一个 `#[tokio::test]`。PTY 管理器是进程级全局单例，`shutdown_all` 会影响同进程内所有会话；单文件单用例避免并行测试互相杀进程。该用例使用真实 `portable_pty` 与真实路由器，通过公开的 `serve_api_with_config` 绑定回环临时端口并拿到关停句柄，用 `tokio-tungstenite` 作为 WebSocket 客户端。

`e2e/run-process-lifecycle.sh` 的 `pty` 段只做操作系统级证据：创建会话、记录 PID、SIGTERM 后判定 PID 消失。WebSocket 帧契约留在 Rust 集成层，不在 shell 里引入额外客户端依赖。

### 固件行为

`e2e/fixtures/process-lifecycle/pty-worker.sh` 保持最小：

- 启动即输出一行 `PTY_READY`。
- 从标准输入读到一行后输出 `PTY_ECHO:<原始行>`，随后继续驻留，不自行退出。
- 不处理 `TERM`；服务端通过 `child.kill()` 强制收敛，避免固件优雅退出掩盖回收问题。

创建会话的请求体从 `e2e/fixtures/process-lifecycle/pty-create.json` 读取，脚本运行时把 `command` 占位符替换为固件绝对路径，`cwd` 指向隔离工作区，`persistent=true` 避免空闲 sweep 干扰。

### 判定顺序

1. 创建包络返回 `status=running`、`pid` 非空，操作系统中该 PID 存活。
2. WebSocket 上收到文本 `PTY_READY` 和一条二进制控制帧；控制帧携带绝对游标和重放起点。就绪行来自重放还是实时帧不作为失败条件。
3. 向 WebSocket 发送文本行，收到包含 `PTY_ECHO:` 的文本帧；终端回显造成的重复输入行不作为失败条件，只检查标记存在。
4. 断开后带游标重连，旧标记不重复出现；新输出仍可到达。该断言约束游标语义，不依赖输出缓冲的真实字节数。
5. 调用服务端句柄关停后，轮询确认子进程消失，并直接查询全局管理器确认会话已移除；服务端端口已关闭，不再发 HTTP 请求。
6. 若环境无法打开 PTY，测试明确失败并指出环境能力，不静默跳过；默认门禁外的 shell 脚本可在启动前检查固件可执行位。

报告由集成测试的机器断言与 shell 脚本的 `pty-liveness.report.txt` 共同构成，后者只包含创建、存活、关停后消失三个事实。

## 场景四：双节点新场景

### 放置

新增 `e2e/run-remote-backup-over-tunnel.sh`，与 `08` 的直连脚本并存。它复用现有最小流程固件，只验证经隧道的备份与索引读回，以及执行节点重启后的重新探活；备份恢复状态机仍由 `08` 覆盖，不在本篇重复。最小流程没有脚本节点，因此该用例不依赖本机脚本执行器能力。

### 节点夹具

| 组件 | 配置 | 说明 |
|---|---|---|
| 执行节点 | 默认二进制、SQLite 存储文件、打开 `WF_RECOVERY_RESTORE_ENABLED`、临时端口 | SQLite 让节点重启后执行记录与修订表仍在，重启分支才有意义 |
| 管理节点 | 默认二进制、开启后端域、备份目录与短周期、假 `ssh` 在 `PATH` 首位 | 所有对执行节点的请求经隧道本地端口进入 |
| 注册条目 | `host=127.0.0.1`，`server_port=执行节点端口`，`local_port=隧道端口` | 假 `ssh` 把隧道端口转发到执行节点端口；不传密钥路径 |
| 备份作用域 | `backup_directory` 指向执行节点工作目录 | 与 `08` 一致，保证目录作用域匹配 |

### 隧道链路用例

1. 先直连执行节点注册脚本并触发一次执行，等待完成。
2. 在管理节点注册后端并启动隧道；后端列表报可达，假 `ssh` 日志中出现转发连接。
3. 经管理端触发备份并复验，再从管理端读取备份索引，确认新记录可见。备份请求必须经隧道本地端口进入执行节点。
4. 报告只记录状态名与布尔结论，不记录执行 ID、备份 ID 与时间戳。

### 节点重启用例

1. 备份验证后向执行节点发 `SIGTERM`，等待退出码为零、端口不可连接。
2. 管理端探活转为不可达；假 `ssh` 进程仍在，因为转发监听器不依赖远端服务。该区分很重要：隧道运行与后端可达是两个独立事实。
3. 用同一端口和同一 SQLite 文件重启执行节点，等待管理端重新探活为可达，执行体重新可见。
4. 管理节点随后也以 `SIGTERM` 关停，确认其监督的隧道子进程一并消失。
5. 结束前关闭管理端，确认假 `ssh` PID 消失。该断言把双节点场景与 SSH 子进程收敛串起来。

### 可选门禁用例

执行节点仍以 `WF_RECOVERY_RESTORE_ENABLED=1` 启动，与远端节点部署形态一致；本脚本不额外断言恢复路由的开闭，该项由远端恢复链路用例单独守卫。

## 固件清单

| 文件 | 形态 | 用途 | 登记位置 |
|---|---|---|---|
| `e2e/fixtures/process-lifecycle/bin/ssh` | 可执行 Python 脚本 | 假 SSH：记录 argv、写 PID、回环转发、可配置为启动后异常退出 | `e2e/fixtures/README.md` 增加一行，注明仅用于进程生命周期与隧道双节点脚本 |
| `e2e/fixtures/process-lifecycle/pty-worker.sh` | 可执行 shell 脚本 | PTY 子进程：就绪标记、输入回显、驻留直到被杀 | 同上 |
| `e2e/fixtures/process-lifecycle/backend-tunnel.json` | JSON 模板 | 后端注册条目；端口与目录由脚本注入，不写死本机路径 | 同上 |
| `e2e/fixtures/process-lifecycle/pty-create.json` | JSON 模板 | PTY 创建请求；`command` 为运行时替换占位符 | 同上 |
| 现有 `workflow-minimal.json` | 复用 | 双节点脚本产生一次真实执行，不依赖脚本执行器 | 不重复登记 |

`e2e/lib/process_lifecycle.sh` 提供四类函数：`wait_pid_gone` 做带上限的进程存活轮询，`project_log` 把 JSON 日志压成事件行，`project_json` 生成稳定字段行，`promote_report` 只在显式参数下复制候选报告。脚本不得在此辅助库中直接断言业务状态。

## 输出与文本审核

### 原始产物

| 目录 | 内容 |
|---|---|
| `e2e/outputs/actual/process-lifecycle/` | 服务端日志、注册与隧道 JSON、PTY 创建 JSON、三个 `.report.txt` 候选 |
| `e2e/outputs/actual/remote-tunnel/` | 双节点日志、备份与状态 JSON、两个 `.report.txt` 候选 |

原始 JSON 保留完整包络，便于失败时定位字段；它们不进入黄金目录。

### 归一化规则

- JSON 报告只保留脚本关心的标量：状态名、布尔值、计数桶；ID、时间戳、端口、PID、绝对路径全部替换或删除。
- 日志报告从 `--log-format json` 行中抽取消息文本，去掉时间、span、目标字段，只保留固定短语。
- argv 报告保留参数顺序，因为顺序是契约；仅把 `ControlPath` 的值、PID、端口和目标主机替换为占位符。
- 帧报告把二进制控制帧压成事件名加占位游标；文本帧只抽取标记行。
- 报告行顺序由脚本固定，映射为行为顺序；不做排序，以免丢掉“先信号后回收”这类时序事实。
- 黄金文件由人工从候选复制，脚本不自动覆盖；黄金变化视为行为变化，需在提交说明中写明原因。

### 黄金示例

`e2e/outputs/golden/process-lifecycle/sigterm.report.txt`：

```text
listen=ok
status_pid=matched
signal=SIGTERM
exit_code=0
listener_after=closed
process_after=gone
log_signal=SIGTERM received, shutting down
log_pty=stopped PTY sessions on shutdown
ssh_child_alive=gone
pty_child_alive=gone
```

`e2e/outputs/golden/process-lifecycle/ssh-tunnel.report.txt`：

```text
register=ok
start=ok
argv=ssh -N -o BatchMode=yes -o ExitOnForwardFailure=yes -p <SSH_PORT> -L <LOCAL>:127.0.0.1:<REMOTE> -o ControlMaster=yes -o ControlPath=<CONTROL_PATH> -o ControlPersist=no <USER>@<HOST>
tunnel_state=running
pid_file=alive
stop=stopped=true
child_after_stop=gone
abnormal_interrupted=tunnel exited: exit status: <CODE>
```

`e2e/outputs/golden/process-lifecycle/pty-liveness.report.txt`：

```text
create=ok
status=running
child_alive=yes
child_after_shutdown=gone
session_after_shutdown=absent
```

`e2e/outputs/golden/remote-tunnel/dual-node-tunnel.report.txt`：

```text
seed_execution=completed
tunnel_probe=reachable
backup=verified
backup_index=visible
forwarded_connections=>=1
```

`e2e/outputs/golden/remote-tunnel/node-restart.report.txt`：

```text
before_stop=ok
exit_code=0
backend_reachable=unreachable
tunnel_child=alive
after_restart=reachable
execution_visible=yes
admin_exit_code=0
ssh_child_after_admin_exit=gone
```

## 实施顺序

1. 增加两个固件与两个 JSON 模板，并在 `e2e/fixtures/README.md` 登记；固件设置可执行位，内容中不写本机路径。
2. 增加 `e2e/lib/process_lifecycle.sh` 与两个脚本，先完成参数解析、端口选择、启动、健康等待与清理骨架，再逐段打开判定。
3. 增加 `crates/app/wf-server/tests/pty_process.rs`，只包含一个真实子进程用例；确认全局 PTY 管理器关停后没有遗留 child。
4. 先运行 `cargo test -p wf-server --test pty_process`，再同步构建产物并依次运行两个 shell 脚本。首次运行只生成候选报告，人工核对后再提升到黄金目录。
5. 更新 `docs/tests/README.md` 索引和 `e2e/README.md` 目录说明，注明两个脚本不进入默认门禁；提交说明中记录黄金报告的变化原因。

## 验收标准

- `pty_process` 用例连续三次通过，关停后目标 PID 均不再存活，无残留会话。
- 两个 shell 脚本在干净工作区连续三次全绿；失败时保留服务端日志、候选报告与 PID 文件供排查。
- SIGTERM 后服务端退出码为零，假 `ssh` 与 PTY 子进程均被服务端关停流程回收，脚本不靠补杀子进程获得通过。
- 双节点脚本证明备份请求与索引读回确实经过隧道本地端口，且执行节点重启后管理端重新探活并恢复可见性。
- 黄金报告全部为纯文本，字段稳定，不含时间戳、随机 ID、本机绝对路径或真实端口。
- 不新增版本号、兼容分支或旧数据迁移；链路变化直接修原模块与原脚本。
