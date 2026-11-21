# CLI 退出码契约（单一来源）

本文件是全部 wf 命令行工具退出码语义的唯一权威来源，覆盖 `wf`、`wf-headless`、`wf-mini` 以及离线调试器 `wf-debug`。代码中对应的常量定义在 `crates/foundation/wf-common/src/exit_code.rs`，任何工具不得硬编码退出码数字。

## 设计原则

退出码表达调用方需要的**可操作语义**，而不是错误发生在哪个调用点。同类故障无论在本地中转还是远程服务、在会话执行还是连接探活，都必须退同一个码。数值对齐 `sysexits.h`。

## 码表

| 码 | sysexits | 语义 | 调用方应对 | 典型来源 |
|---:|---|---|---|---|
| 0 | `EX_OK` | 成功 | — | 所有成功路径；调试器信息展示类、校验通过 |
| 64 | `EX_USAGE` | 参数/用法错误 | 改正命令行后重试 | `CliError::Arguments`、clap 解析错误、`--directory` 格式错误 |
| 65 | `EX_DATAERR` | 数据或业务校验失败 | 改输入数据/语义后重试 | `CliError::Business`、远端业务码未单列时、调试器 check/assert/agents 违规 |
| 66 | `EX_NOINPUT` | 输入不可读或缺失 | 提供/修复输入文件 | trace/snapshot 读取失败、schema 不支持 |
| 67 | `EX_NOUSER` | 目标实体不存在 | 检查目标标识 | 远端 `SESSION_NOT_FOUND` |
| 69 | `EX_UNAVAILABLE` | 服务不可达（本地中转与远程等价） | 启动/修复服务端后重试 | 健康检查失败、连接拒绝超时、`SERVICE_UNAVAILABLE`、`Http`/`Stream` 传输故障 |
| 70 | `EX_SOFTWARE` | 内部故障 | 报缺陷，勿重试 | 终端后端失败、序列化错误、`CommonError` 转发 |
| 74 | `EX_IOERR` | 输出 IO 错误 | 检查输出目标/权限 | 写 stdout 或日志文件失败 |
| 75 | `EX_TEMPFAIL` | 临时失败，可重试 | 稍后重试 | 远端 `SESSION_BUSY` |
| 76 | `EX_PROTOCOL` | 远端协议/响应错误 | 升级客户端或服务端 | 未知远端业务码、`Invalid` 响应 |
| 77 | `EX_NOPERM` | 权限拒绝 | 检查凭据/允许列表 | 远端 `FORBIDDEN`/`TICKET_INVALID`、`Unauthorized` |
| 78 | `EX_CONFIG` | 配置错误 | 修正配置后重试 | 亲和性/API 版本不匹配、`NotConfigured`、本地配置读失败 |
| 130 | （非 sysexits） | 用户中断 | 不重试 | SIGINT / 用户取消；沿用 POSIX `128 + SIGINT` 惯例 |

## 远端业务码细分

远端稳定业务码（`wf-client::ClientError::Remote`）由 `wf_common::exit_code::remote_exit_code` 单点映射；本地中转与远程服务等价对待，仅按业务码决定退出码：

| 远端业务码 | 退出码 |
|---|---:|
| `SESSION_BUSY` | 75 |
| `SESSION_NOT_FOUND` | 67 |
| `LOCATION_NOT_FOUND` | 66 |
| `FORBIDDEN` | 77 |
| `TICKET_INVALID` | 77 |
| `SERVICE_UNAVAILABLE` | 69 |
| 其它/新增码 | 76 |

新增业务码默认退 76，提示调用方升级；如需专门语义，先在共享模块登记映射，再在文档补充。

## 收敛点

- 运行期错误：各入口统一调用 `wf_cli_shared::exit_with_error`，它打印到 stderr 并按契约退出。
- 参数解析：各入口调用 `wf_cli_shared::parse_cli_or_exit`（调试器因依赖约束改用共享常量自行处理），把 clap 的 usage 错误从内建的 2 改为 64；`--help`/`--version` 仍正常退出 0。
- 调试器门禁：违规计数与结论保留在 JSON 包络字段中，退出码只表达"校验通过/失败"（0/65）。

## 约定

- 业务输出走 stdout，诊断走 stderr；退出码只补充机器可判定的结果类别，不承载细节。
- 严禁在入口或命令处理器中硬编码退出码数字，一律引用 `wf_common::exit_code` 常量。
