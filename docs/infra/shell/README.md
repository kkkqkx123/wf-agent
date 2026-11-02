# wf-shell

Infra 层的 Shell / 终端执行引擎，为上层（`wf-tools` 的 shell 工具、`wf-runtime` 的运行时装配、未来的终端消费者）提供统一的底层 shell 执行原语：shell 探测、无状态单命令执行、后台会话引擎（pipe / PTY 后端 + 增量输出）、shell 事件订阅接口以及命令安全策略。

依赖：`wf-common`、`wf-types`、`wf-sandbox`。特性 `lua` 透传给 `wf-sandbox`；`test-util` 仅供跨 crate 测试使用。

## 核心概念

- **终端会话（TerminalSession）**：store 中的持久记录，生命周期长于单条命令。持有会话级元数据（idle/busy 状态、task_id、cwd/env、终端模式、累计输出缓冲），以及 `current` 当前命令句柄。命令结束后会话仍在，后续命令可复用同一会话。
- **命令（ShellSession）**：会话内一次独立子进程执行（pipe 或 PTY 后端），输出写入会话共享的输出缓冲。
- **两种后端**：`Pipe`（std 子进程，管道 stdin/stdout/stderr，字节原样保留）与 `Pty`（`portable-pty` 真终端，CRLF 归一化）。PTY 始终编译在内，运行时由 `pty_enabled` 配置开关控制；仅当 `pty_enabled` 且会话请求 `interactive`/`force_pty` 时使用 PTY，其余组合回退 pipe。
- **环境继承规则**：所有 spawn 路径统一走 `spawn` 模块 —— 子进程继承父进程环境，再叠加会话/命令 env（覆盖同名变量，含 `PATH`）。挂载沙箱策略时改走 `wf-sandbox` 执行网关：先清空继承环境（恢复最小 PATH/HOME/TMPDIR）再叠加显式 env，避免宿主密钥泄漏。PTY 后端无法挂 `pre_exec` 钩子，跳过内核级沙箱强制（仍经过命令安全门）。

## 模块一览

| 模块 | 职责 |
|---|---|
| `shell_detector` | 平台 shell 探测与解析。`ShellType`（Bash/Zsh/Fish/Sh/Cmd/Powershell/Pwsh/GitBash/Wsl），路径解析优先级：自定义覆盖 > 默认硬编码路径 > `which`/`where` > 可执行名。 |
| `runner` | 无状态单命令执行 `run_command`：经解析后的 shell 运行，捕获 stdout/stderr，强制超时，可选一次性 stdin 输入，可选沙箱策略。阻塞执行放在 tokio blocking 池。 |
| `spawn` | 唯一的进程拉起点：把 `(command, cwd, env, shell_type)` 变为已配置的 shell 进程（进程组设置），供 runner 与会话引擎共用，保证 env/cwd/kill 语义不漂移。含优雅终止（SIGTERM → 超时后 SIGKILL）。 |
| `engine` | 后台会话引擎的门面模块，re-export `store` / `terminal_session` / `session` / `backend` / `background_task` 等公共 API。核心入口是 `BackgroundShellStore`。 |
| `store` | `BackgroundShellStore`：会话池 + 完成监视线程 + 公共引擎 API。能力：`spawn` / `spawn_with_options`、`get_or_create`（复用空闲会话）、`execute_in_session`（后台化时解除超时上限）、`send_input`、`resize`（PTY）、`kill` / `kill_with`、`release_sessions_for_task`（按任务批量释放）、`sweep_idle_sessions`、`set_lifecycle_sink`、`from_config`。全局上限 64 个会话。 |
| `terminal_session` | 会话记录（Idle/Busy 状态机）、命令执行、终止与事件派发；命令完成事件保证在全部输出事件之后（有界 drain 等待）。 |
| `session` | 会话内单条命令 `ShellSession`：包装子进程后端 + 输出读线程，缓存退出状态并通过条件变量唤醒等待者。 |
| `backend` | 两种子进程后端 `PipeBackend` / `PtyBackend` 与进程组级 kill 助手。 |
| `output_buffer` | 会话输出的分段环形缓冲 + 增量读游标。所有位置为绝对字节偏移；超过 `MAX_OUTPUT_BYTES`（256 KiB）只保留尾部，被截断的读窗口返回截断标记而非静默为空。段边界落在字符边界上。 |
| `spill` | 输出溢出 spill 文件：内存 ring 放不下后，后续 chunk 追加写入文件，页式读取仍可取到已滚出内存的绝对区间（默认阈值 256 KiB，`0` 关闭）。文件随会话删除。 |
| `utf8` | `Utf8ChunkDecoder`：跨 chunk 携带不完整的多字节 UTF-8 尾部（最多 3 字节），PTY 路径在原始字节上做 CRLF 归一化。 |
| `line_dispatcher` | 把输出流按行切分，将完整非空行转发给事件派发器（跨 chunk 保留半行）；非阻塞入队，慢消费者不会反压读线程。 |
| `drain` | 读线程与完成监视线程的协调原语：`MonitorWakeup`（输出 EOF 时唤醒监视线程，近零延迟检测退出；带超时等待兼作 50ms 兜底轮询）与 `OutputDrain`（读线程计数，等待者据此等输出完全落盘）。 |
| `event_sink` | `ShellEventSink` trait（会话创建 / 命令开始 / 逐行输出 / 命令完成 / 会话终止 / Flush 屏障）。派发异步化：`EventDispatcher` 队列 + 专用转发线程，慢 sink 不阻塞输出管线。队列按事件类别限流（输出 4096 条、生命周期 256 条，超限丢弃并计数；Flush 不限流），内存有界。由 `wf-runtime` 实现并桥接到 `wf_core::EventBus`。 |
| `lifecycle` | 业务无关的会话生命周期事件（Started/CommandCompleted/Terminated），只携带 session_id / task_id / cwd / 状态，供上层（如文件 checkpoint 的范围采样）订阅，与逐行输出事件开关相互独立。 |
| `background_task` | `BackgroundTaskRegistry`：跨工具调用的后台任务注册表（进程全局），按 session_id 记录命令、归属 execution、是否被转后台及最终状态（Running/Completed/Failed/Cancelled）。store 拥有进程与字节，该注册表只管任务状态与后台交接。 |
| `command_safety` | 命令安全策略：委托 `wf-sandbox` 的引号感知链式解析器拆分子命令（与沙箱静态分析保持一致）；正则识别危险替换（`${var@...}`、赋值中的八进制/十六进制/Unicode 转义、间接展开 `${!...}`、herestring 注入、zsh glob 修饰符等）；`CommandPolicy` 承载 allowed/denied/read/write 命令清单。默认 allowlist 覆盖常用开发命令，不在清单内的命令仍执行但会被标记（审批属应用层职责），策略显式拒绝的命令被拒。 |
| `config` | `ShellToolConfig`：全部配置项（工作目录、超时、命令清单、shell 覆盖、PTY 开关与默认尺寸、优雅终止超时、默认 env、会话复用/空闲回收、输出事件开关、事件 sink、沙箱策略、spill 目录与阈值）。 |
| `error` | `ShellError` / `ShellResult`。 |

## 线程与完成模型

- 每个 store 一个懒启动的监视线程（而非每命令一线程），负责检测进程退出并 finalize busy 会话；输出读线程在 EOF 时通过 `MonitorWakeup` 即时唤醒，退出检测接近零延迟；少数 EOF 滞后于退出（子进程持有管道）的场景由 50ms 兜底轮询覆盖。
- store drop 时监视线程退出；后台任务注册表为进程全局单例。

## 使用入口

```rust
use wf_shell::{ShellToolConfig, BackgroundShellStore};

let store = BackgroundShellStore::from_config(&ShellToolConfig::default());
let result = store.get_or_create(&Default::default(), Some("task-1"))?;
store.execute_in_session(&result.session_id, "cargo test", 300_000)?;
```

无状态路径：`runner::run_command`；事件订阅：实现 `ShellEventSink`（逐行输出）或 `SessionLifecycleSink`（会话边界）后通过 `ShellToolConfig` 注入。

## 测试

- 单元测试集中在 `store_test.rs`（会话池、监视、释放、空闲回收等）。
- 集成测试在 `tests/shell_integration.rs`。
