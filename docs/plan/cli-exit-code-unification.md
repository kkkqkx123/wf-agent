# CLI 退出码：现状评估、统一方案与实施结果

> 状态：**已实施完成**。O1~O6 全部拍板并完成代码改造、单测与端到端验证，详见第八、十一、十二节。权威码表见 `docs/app/cli/00-exit-codes.md`。
>
> **阅读提示**：第二~七节是**改造前的历史取证记录**（当时的码表 0/1/2/3/4 与行号均为旧值，仅用于说明改造动机），不代表当前实现。当前事实以第八节定稿、权威码表 `docs/app/cli/00-exit-codes.md` 及源码 `crates/foundation/wf-common/src/exit_code.rs` 为准；第十节记录了落地后的再次核对与残留修复。

## 〇、结论摘要

改造前 CLI 退出码存在一个权威映射点（`CliError::exit_code()`），三个产品二进制（wf-headless / wf-mini / wf-tui）都已收敛到它，方向是对的。但改造前存在三类实质问题：

1. **文档与实现不一致**：头部注释只声明 0/1/2/3/4 四类，实际有 7 个 `CliError` 变体，`Terminal` / `Io` / `Common` 静默归入 1，从未被文档列出。
2. **同一类故障映射到不同码**：最典型是"服务器不可达"——通用转换里是 `Business`(1)，连接探活与管理命令里是 `Configuration`(3)，两处代码注释还互相矛盾。脚本无法据此稳定判断。
3. **存在完全独立的第二套体系**：debugger 用 `anyhow` + 裸 `i32` 退出码，约定"信息类恒 0、校验类 0/1"，与主线 0/1/2/3/4 不兼容。

统一方向（已落地）：确立**唯一退出码契约表**（`wf-common::exit_code`，对齐 sysexits.h），让"码"表达调用方需要的**可操作语义**（成功 / 参数错误 / 数据校验 / 输入缺失 / 目标不存在 / 服务不可达 / 内部故障 / IO / 临时失败 / 协议 / 权限 / 配置 / 中断），消除"按调用点猜测映射"的分散逻辑；clap 解析错误与 debugger 均已纳入同一张表。

---

## 一、取证范围与方法

- 范围：`crates/app/cli/`（headless / mini / tui / cli-shared）全部 `main.rs` 与 `CliError` 构造点，以及 debugger 入口；相关设计文档。
- 方法：全局搜索 `exit` / `ExitCode` / `exit_code` / `process::exit`，逐个打开构造点核对上下文；再与文档声明比对。
- 事实一律附 `file:line`。本文不含完整代码片段（遵循 AGENTS.md「Plan/Design Document」约定）。

## 二、唯一权威映射点（改造前快照，非当前实现）

`crates/app/cli/wf-cli-shared/src/error.rs:33-46` 的 `CliError::exit_code()` 是主线的唯一映射（以下码值与行号均为改造前旧值）：

| 变体 | 退出码 | 定义位置 |
|------|:---:|------|
| `Arguments(String)` | 2 | `error.rs:18` / 映射 `error.rs:37` |
| `Configuration(String)` | 3 | `error.rs:20` / 映射 `error.rs:38` |
| `Terminal(String)` | 1 | `error.rs:22` / 映射 `error.rs:39` |
| `Business(String)` | 1 | `error.rs:24` / 映射 `error.rs:40` |
| `Interrupted(String)` | 4 | `error.rs:26` / 映射 `error.rs:41` |
| `Io(std::io::Error)` | 1 | `error.rs:28` / 映射 `error.rs:42` |
| `Common(CommonError)` | 1 | `error.rs:30` / 映射 `error.rs:43` |

头部注释 `error.rs:3-8` 只声明了 0/1/2/3/4 四类；`Terminal` / `Io` / `Common` 从未列入。单测 `error.rs:132-139` 只断言了 5 个变体，同样遗漏这三者——即"归 1"的行为没有测试保护。

## 三、三个入口的实际收敛路径

- **wf-headless**：`main.rs:10` 由 clap 解析参数，`main.rs:13` 对 `FromArgMatches` 错误调用 clap 的 `err.exit()`（clap 自定义 usage 错误码，默认 2）；`main.rs:20-23` 对运行结果错误走 `std::process::exit(i32::from(err.exit_code()))`。
- **wf-mini**：`main.rs:16-19` 同样用 clap `err.exit()`；运行期有 5 处独立 `exit(err.exit_code())`（`main.rs:30/41/49/57/62`），分别对应无头子命令、模式解析、单次运行、TUI 会话运行、会话构造失败。
- **wf-tui**：`main.rs:9` 用 `Cli::parse()`（clap 自行处理解析错误并退出），`main.rs:10-14` 对运行结果走 `exit(err.exit_code())`。其 `Cli` / `CliError` 均由 `wf-tui/src/lib.rs:8-11` 从 `wf-cli-shared` 重导出，与主线同源。

**结论**：三个二进制的运行期错误已共享同一映射；但**参数解析错误由 clap 独立决定**（`err.exit()` / `parse()`），不经 `CliError`。目前 clap usage 错误恰好也是 2，与 `Arguments`=2 数值一致，但这是两套独立来源的巧合对齐，不是单一契约。

## 四、错误分支清单

### 4.1 退出码 2 — `Arguments`（参数错误）
构造点集中在"本地输入校验"：工作区解析失败（`connection.rs:67-68`、`run.rs:117/194`、`cmd/directories.rs:28`）、变量/输入 JSON 校验（`run.rs:216/323`、`cmd/execution.rs:165/169/187`）、文件格式解析失败（TOML/JSON，`cmd/template.rs:273/276/301/310/315/336/345/350`、`cmd/script.rs:139`、`cmd/tool.rs:144`、`cmd/trigger.rs:208`、`cmd/llm.rs:213`、`cmd/workflow.rs:674/676`）、导出格式/聚合类型非法（`cmd/query.rs:58/142`、`cmd/metrics.rs:58`）、交互选择越界（`cmd/llm.rs:559/564/593/596/601`）。

### 4.2 退出码 3 — `Configuration`（配置错误）
- **远端/服务类**：亲和性探测失败/不匹配（`connection.rs:117/146`）、API 版本读取失败/不匹配（`connection.rs:160/166`）、健康检查 Http 失败（`connection.rs:183-194`）、客户端 `Invalid` / `Unauthorized` / `NotConfigured`（`error.rs:115/117/120`）。
- **后端注册表类**：服务不可达回退提示 `server_unavailable_hint()`（`backends.rs:386/403/414/457/490/508/554/575/592/627/644/661`，判定见 `backends.rs:320-328`）、非法注册表文件（`backends.rs:87/95/113`）。
- **本地文件/凭据类**：读文件失败（`cmd/tool.rs:141`、`cmd/template.rs:269/292/327`、`cmd/workflow.rs:669`、`cmd/llm.rs:210`、`cmd/trigger.rs:205`、`cmd/script.rs:136`）、配置路径不可用（`cmd/config.rs:15/19/34`）、审批记录路径/取消（`cmd/plugin.rs:35/79/87/101`）、密码读取（`cmd/llm.rs:351/354`）、远端备份文件坏（`cmd/remote.rs:673-677`）。

### 4.3 退出码 1 — `Business`（业务失败，兼作默认桶）
- **终态业务失败**：`run.rs:385`（headless 下被追问）、`Terminal::Failed` / `Unknown` 经 `run_stream.rs` 收敛。
- **远端业务码**：`SESSION_BUSY` / `SESSION_NOT_FOUND` / `LOCATION_NOT_FOUND` / `FORBIDDEN` / `TICKET_INVALID` / `SERVICE_UNAVAILABLE` / 兜底（`error.rs:89-109`）。
- **传输/数据类（与"配置"语义冲突，见 6.1）**：`C::Http` / `C::Stream` / `C::NotFound`（`error.rs:113/114/116`）、通用 JSON 序列化错误（`error.rs:76-83`）。
- **其余变体汇入**：`Terminal` / `Io` / `Common`（`error.rs:39/42/43`）。

### 4.4 退出码 4 — `Interrupted`（中断）
会话 SIGINT 与服务端 `Interrupted` 终态（`run_stream.rs:113/117/177/181`）、日志跟随取消（`cmd/execution_logs.rs:146/189`）、watch 取消（`cmd/execution_stream.rs:347/381`）、用户放弃（`cmd/checkpoint.rs:48`）。

### 4.5 终端故障 — `Terminal`（当前归 1）
TUI 初始化/清屏失败（`tui.rs:486/490`、`tui/shell_draw.rs:390`）、事件轮询/读取失败（`tui/event_loop.rs:153/155`）、绘制失败（`tui/event_loop.rs:342/368/382`）。

### 4.6 第二套体系 — debugger（独立，不兼容）
`crates/app/debugger/src/main.rs:4-11`：`run()` 返回 `i32`，非 0 时 `process::exit(code)`；加载/IO 错误由 `anyhow` 的 `Err` 交给 Rust runtime，默认 exit 1。退出码产出于 `engine/runner.rs:19/27/29/32/39`（仅 0/1，语义为"门禁是否通过"）。文档约定见 `docs/app/debugger.md:80-94`：「信息展示类恒 0；校验类以失败/违规数决定 0 或 1」。

## 五、错误码表（改造前快照，非当前实现）

| 码 | 主线语义（声明） | 实际承载 | debugger 语义 |
|:---:|------|------|------|
| 0 | 成功 | 成功 | 信息展示类 / 校验通过 |
| 1 | 业务失败 | 业务失败 + 终端故障 + IO + CommonError + 传输失败 + 序列化错误 | 校验失败/违规 + anyhow 加载/IO 错误 |
| 2 | 参数错误 | `Arguments` + clap usage 错误 | —（未使用） |
| 3 | 配置错误 | 参数标志 + 服务不可达 + 亲和性/版本 + 本地文件读取 + 凭据 | —（未使用） |
| 4 | 中断 | SIGINT / 取消 / 放弃 | —（未使用） |

## 六、逐项合理性评估

### 6.1 同类故障跨码映射（最高优先级）
`error.rs:110-113` 注释明确："Transport failures (connection refused, timeout, ...) are operational, not configuration: exit 1 instead of 3 so scripts can distinguish 'server down' from 'bad flags'." 但同一类 `ClientError::Http`（服务器不可达）在 `connection.rs:183-194` 与 `backends.rs:320-328→386` 等处被映射为 `Configuration`(3)。结果：**"服务没起来"在会话执行路径退 1，在探活/管理路径退 3**，脚本无法稳定判断——这恰恰破坏了注释想要达成的区分。

### 6.2 `Configuration` 语义过载
码 3 同时承载"用户参数问题"（亲和性不匹配、API 版本不匹配）与"环境问题"（服务不可达、读文件失败），调用方无法对 3 做统一处理（重试？改配置？）。

### 6.3 `Business` 成为默认桶
序列化错误（`error.rs:81`）、流错误（`error.rs:114`）、未找到（`error.rs:116`）与真正的业务失败同码。内部故障与业务结果不可分。

### 6.4 头部文档、单测与实现三方脱节
文档清单（4 类）与实现（7 变体）不一致；单测遗漏 `Terminal`/`Io`/`Common`；多处文档各自定义退出码且措辞不同（`docs/cli/01-功能清单.md:115`、`docs/cli/05-opencode-mini模式与无头模式设计.md:270-272`、`docs/app/cli/README.md:48`、`docs/app/cli/01-wf-cli-shared.md:86`），维护时易漂移。

### 6.5 参数解析错误是独立来源
clap 的 `err.exit()` / `Cli::parse()` 直接退出，不经 `CliError`。数值对齐（均为 2）属巧合，若未来调整 `Arguments` 的码，解析路径不会跟随。

### 6.6 第二套体系与类型不统一
debugger 的 0/1 + 裸 `i32` 与主线的 0/1/2/3/4 + `u8→i32` 不兼容；同一仓库两种退出码语义。

### 6.7 合理之处（保留）
- 三个产品二进制的运行期错误已共享单一映射点，方向正确。
- `Arguments`=2 与 clap usage 错误对齐，符合 Unix 惯例。
- `Interrupted`=4 语义集中且一致。
- 诊断走 stderr、业务走 stdout 的约定清晰（`main.rs` 各入口 `eprintln!`，`docs/app/cli/README.md:44`）。

## 七、问题根因归类

| 根因 | 表现 |
|------|------|
| 缺少单一语义权威 | "码"没有明确定义为"调用方可操作语义"，而是按抛出点就近选变体 |
| 变体职责重叠 | `Business` / `Configuration` 都收纳了 IO 与传输错误 |
| 文档非单一来源 | 4 处文档 + 代码注释各写各的 |
| 体系分叉 | debugger 独立演进，未纳入 CLI 公共层 |

## 八、统一方案建议

### 8.1 目标契约表（已定稿，对齐 sysexits.h）

开放点 O1~O6 已全部拍板，码值严格对齐 `sysexits.h`。权威码表由 `docs/app/cli/00-exit-codes.md` 与 `crates/foundation/wf-common/src/exit_code.rs` 维护，本节只列决议摘要。

| 码 | sysexits | 语义 | 判定标准（调用方可操作） |
|:---:|---|---|------|
| 0 | `EX_OK` | 成功 | — |
| 64 | `EX_USAGE` | 用法/参数错误 | 用户改命令即可重试（含 clap 解析错误） |
| 65 | `EX_DATAERR` | 数据/业务校验失败 | 改输入数据或语义后重试（含调试器门禁违规） |
| 66 | `EX_NOINPUT` | 输入不可读/缺失 | 提供或修复输入文件 |
| 67 | `EX_NOUSER` | 目标实体不存在 | 检查目标标识（`SESSION_NOT_FOUND`） |
| 69 | `EX_UNAVAILABLE` | 服务不可达 | 启动/修复服务端后重试；本地中转与远程等价 |
| 70 | `EX_SOFTWARE` | 内部故障 | 报 bug，不重试（终端/序列化/CommonError） |
| 74 | `EX_IOERR` | 输出 IO 错误 | 检查输出目标/权限 |
| 75 | `EX_TEMPFAIL` | 临时失败可重试 | 稍后重试（`SESSION_BUSY`） |
| 76 | `EX_PROTOCOL` | 远端协议/响应错误 | 升级客户端或服务端（未知业务码） |
| 77 | `EX_NOPERM` | 权限拒绝 | 检查凭据/允许列表（`FORBIDDEN`/`TICKET_INVALID`） |
| 78 | `EX_CONFIG` | 配置错误 | 修正配置后重试（亲和性/版本/NotConfigured） |
| 130 | 非 sysexits | 用户中断 | 不重试（128 + SIGINT） |

> O3 落地：远端业务码通过 `wf_common::exit_code::remote_exit_code` 单点细分为 75/67/66/77/69/76；本地中转与远程服务等价，仅按业务码决定退出码。

### 8.2 落地手段
1. **单一权威**：码常量与远端业务码映射集中在 `wf-common::exit_code`，CLI 与 debugger 都依赖它，禁止散落魔数。
2. **单一映射**：所有入口统一调用 `wf_cli_shared::exit_with_error` 收敛；`parse_cli_or_exit` 把 clap 的 usage 错误从内建 2 改为 `EX_USAGE`(64)，`--help`/`--version` 保持 0。
3. **消除按点猜测**：`ClientError::Http`/`Stream` 统一在 `From<ClientError>` 映射为 `Unavailable`；删除 `connection.rs` / `backends.rs` 里把服务不可达写成 `Configuration` 的分散逻辑。
4. **纳入 debugger**：debugger 复用 `wf-common::exit_code` 常量（不引入 wf-cli-shared 的重依赖链）；门禁违规计数保留在 JSON 包络字段中，退出码只表达 0/65。
5. **文档单一来源**：新建 `docs/app/cli/00-exit-codes.md` 为权威码表，其余文档改为引用。

## 九、改造点清单（按依赖顺序）

| # | 文件/位置 | 动作 |
|:--:|------|------|
| 1 | `error.rs:3-8,33-46` | 定义具名码常量/枚举，补全 7 变体映射与注释 |
| 2 | `error.rs:85-123` + `connection.rs:177-197` + `backends.rs:320-328` | 统一 `ClientError` 映射，消除 Http 在 1/3 间摇摆 |
| 3 | `error.rs:76-83` | 序列化错误从 `Business` 迁到"内部故障" |
| 4 | 三个 `main.rs` | 统一收敛到单一 helper / `ExitCode`；处理 clap 解析错误来源 |
| 5 | `error.rs:132-139` 单测 | 补齐全部变体断言 + 契约表一致性测试 |
| 6 | debugger `main.rs` + `runner.rs` | 纳入统一契约，门禁语义移入包络 |
| 7 | 四处文档 | 合并为单一来源，其余改为引用 |

## 十、验收标准

- 任一 `CliError` 变体的退出码在代码、单测、文档三处一致。
- "服务不可达"在会话、探活、管理命令三条路径退出码相同。
- 脚本可用退出码稳定区分：参数错 / 配置错 / 服务不可达 / 业务失败 / 中断 / 内部故障。
- debugger 与主 CLI 共享同一套码；门禁结果通过包络字段而非退出码数字表达。
- `cargo test -p wf-cli-shared` 与 `cargo clippy --all-targets --all-features` 通过。

## 十一、决策定稿（已拍板）

| 编号 | 决策 | 结论 |
|:---:|------|------|
| O1 | 服务不可达独立码 | **新增** `EX_UNAVAILABLE`(69)，本地中转与远程服务等价 |
| O2 | 内部故障独立码 | **新增** `EX_SOFTWARE`(70)，终端/序列化/CommonError 归入 |
| O3 | 远端业务码细分 | **细分**：`remote_exit_code` 映射 75/67/66/77/69/76，未知码 76 |
| O4 | debugger 纳入统一 | **本期纳入**，复用 `wf-common::exit_code` 常量；门禁 0/65，违规计数留包络 |
| O5 | clap 解析错误 | **统一到 64**（EX_USAGE），由 `parse_cli_or_exit` 收敛，help/version 保持 0 |
| O6 | 对齐 sysexits.h | **严格对齐**，全部码值取自 sysexits.h；中断沿用 130（128+SIGINT）惯例 |

## 十二、实施结果

已完成全部改造并通过编译、单测与端到端验证。

**代码改动**

| 文件 | 改动 |
|------|------|
| `crates/foundation/wf-common/src/exit_code.rs`（新增） | sysexits 常量 + `remote_exit_code` 单点映射 + 2 个单测 |
| `crates/foundation/wf-common/src/lib.rs` | 导出 `exit_code` 模块 |
| `crates/app/cli/wf-cli-shared/src/error.rs` | 新增 `Unavailable`/`Internal`/`RemoteBusiness` 变体；全变体重映射；`From<ClientError>` 细分；新增 `exit_with_error`/`parse_cli_or_exit` helper；4 个单测 |
| `crates/app/cli/wf-cli-shared/src/connection.rs` | 健康检查 Http → `Unavailable`；新增 `probe_error` 统一探测映射 |
| `crates/app/cli/wf-cli-shared/src/backends.rs` | 12 处服务不可达 `Configuration` → `Unavailable` |
| 三个 `main.rs`（headless/mini/tui） | 统一走 `parse_cli_or_exit` + `exit_with_error` |
| `crates/app/debugger/src/main.rs` | clap usage→64、加载错误→66、门禁结果透传 |
| `crates/app/debugger/src/engine/runner.rs`、`cli/commands.rs` | 裸 0/1 全部改为 `exit_code` 常量（OK=0/DATAERR=65） |

**文档改动**

- 新增权威码表 `docs/app/cli/00-exit-codes.md`；`docs/cli/01-功能清单.md`、`docs/cli/05-...md`、`docs/app/cli/README.md`、`docs/app/cli/01-wf-cli-shared.md`、`docs/app/debugger.md` 全部改为引用单一来源。

**验证结果**

| 验证项 | 结果 |
|------|------|
| `cargo test -p wf-common --lib exit_code` | 2 passed |
| `cargo test -p wf-cli-shared --lib error` | 4 passed |
| `cargo test -p wf-debugger --lib` | 71 passed |
| `cargo build` 四个二进制（headless/mini/tui/debug） | 全部成功 |
| `cargo clippy` 涉及 crate | 通过（仅 wf-tui 既有 `globset` 未使用依赖警告，与本次无关） |
| 端到端：无效参数 | 64 ✓ |
| 端到端：`--help` | 0 ✓ |
| 端到端：debugger 缺文件 | 66 ✓ |
| 端到端：服务不可达 | 69 ✓（stderr 显示 "service unavailable"） |

**关键成果**：原先"服务不可达在会话路径退 1、探活路径退 3"的矛盾彻底消除，三条路径统一退 69；脚本现可稳定区分参数错(64)/配置错(78)/服务不可达(69)/业务失败(65)/中断(130)/内部故障(70)。

---

## 十三、落地后再核对与残留修复（后续补记）

统一改造落地后再次全量核对，发现三处残留未收尾问题，已在本次一并修复。完整任务清单、逐处 file:line 取证、代码 diff 与开放点见 `docs/plan/cli-exit-code-followup.md`。

| # | 残留问题 | 证据 | 修复 |
|:--:|---|---|---|
| 1 | 中断码跨路径不一致：工作流流式执行的 `Interrupted` 事件被映射为 `Business`(65)，其余中断路径均为 `Interrupted`(130) | `cmd/execution_stream.rs` 的 `stream_terminal` | 改为 `CliError::Interrupted`，并补退出码单测 |
| 2 | 过时注释：`run.rs` 文档注释仍写 Business→exit 1、Interrupted→exit 4 | `run.rs` 的 `run_session` 注释 | 改为引用 `wf_common::exit_code`，不再硬编码数字 |
| 3 | e2e 负例只断言"非零"，不校验具体码，无法防止错码回归 | `e2e/run-e2e.sh` 的 `run_negative_case` | 新增 `run_exit_code_case` 精确断言 helper，对 help(0)/usage(64) 加契约用例 |

说明：开放点 O3（远端业务码细分）与 O6（对齐 sysexits）的决议不变；本次属一致性收尾，未改动权威码表。
