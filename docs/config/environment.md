# 环境变量

## WF_* 覆盖（基础设施域）

| 环境变量 | 作用 |
|---|---|
| `WF_STORAGE_TYPE` | 存储类型（sqlite/postgres/memory） |
| `WF_STORAGE_SQLITE_DB_PATH` | SQLite 数据库路径 |
| `WF_AGENT_MAX_ITERATIONS_CAP` | agent 迭代硬上限 |
| `WF_AGENT_DEFAULT_MAX_ITERATIONS` | agent 默认迭代数 |
| `WF_AGENT_MAX_CONCURRENT` | agent 最大并发 |
| `WF_AGENT_MAX_SUB_AGENT_DEPTH` | 子 agent 最大递归深度 |
| `WF_AGENT_MAX_PAUSE_DURATION_MS` | agent 最大暂停时长 |
| `WF_WORKFLOW_LOOP_MAX_ITERATIONS_CAP` | 工作流循环硬上限 |
| `WF_WORKFLOW_LOOP_DEFAULT_MAX_ITERATIONS` | 工作流循环默认上限 |
| `WF_WORKFLOW_MAX_NAVIGATION_MULTIPLIER` | 死循环检测倍数 |
| `WF_EXEC_NODE_TIMEOUT_MS` | 节点超时默认值 |
| `WF_EXEC_MAX_EXECUTION_TIME_MS` | 工作流总超时默认值 |
| `WF_METRICS_ENABLED` | 指标总开关 |
| `WF_LOG_LEVEL` | 日志级别（`trace`/`debug`/`info`/`warn`/`warning`/`error`/`off`） |
| `WF_LOG_FORMAT` | 日志格式（`full`/`compact`/`json`/`pretty`） |
| `WF_FILE_CHECKPOINT_ENABLED` | 文件检查点开关 |
| `WF_FILE_CHECKPOINT_WORKSPACE_ROOT` | 文件检查点工作区根 |
| `WF_FILE_CHECKPOINT_MANUAL_WATCH` | 手动 watcher 开关 |
| `WF_FILE_CHECKPOINT_MANUAL_POLL_MS` | 手动 watcher 轮询间隔 |
| `WF_FILE_CHECKPOINT_GC_INTERVAL_SECS` | GC 自动运行间隔 |
| `WF_SANDBOX_MODE` | 沙箱模式（disabled/lenient/strict） |

基础设施域的 `WF_*` 覆盖 fail-fast：非法值启动失败；空字符串按未设置处理，由下一优先级来源填充。布尔值只接受 `true`/`1`/`yes`/`on` 与 `false`/`0`/`no`/`off`，整数超出目标字段范围或为负数时启动失败；环境覆盖结束后会再次执行 limits 的跨字段校验。服务中间件的鉴权与限流环境变量沿用同一解析器，但采用告警并保留旧值的策略，拼写错误不会静默变成关闭。日志级别与格式只接受上述枚举值，不解析 EnvFilter 表达式；日志写 stderr，由服务管理器负责捕获与留存。

## 服务与前端变量

| 变量组 | 作用 |
|---|---|
| `WF_SERVER_BIND_ADDR`、`WF_SERVER_STATIC_DIR`、`WF_SERVER_STATIC_INDEX`、`WF_SERVER_STATIC_FALLBACK`、`WF_SERVER_STATIC_IMMUTABLE_MAX_AGE_SECS` | 监听地址与静态站点；非法监听地址或静态参数启动失败 |
| `WF_SERVER_WORKER_THREADS`、`WF_SERVER_EVENT_BUS_CAPACITY`、`WF_SERVER_ENTITY_CACHE_CAPACITY`、`WF_SERVER_ENTITY_CACHE_TTL_SECS` | 服务进程容量与 Tokio worker 数 |
| `WF_DIRECTORY_ROOTS`、`WF_TRUSTED_DIRECTORY_ROOTS`、`WF_DIRECTORY` | 可服务目录根、可信执行根与 CLI 默认目录 |
| `WF_USER_GIT_ENABLED` | 工作区 Git/文件/PTY 域的开关；`false`/`0`/`no` 关闭 |
| `WF_KEYWORD_*` | keyword 索引的启用、后端、路径、分词器、ES 连接与批量参数 |
| `WF_BACKEND_*` | 后端注册、探测、备份与远端地址；`WF_BACKEND_REMOTE_API_KEY` 只从环境读取 |
| `WF_REMOTE`、`WF_REMOTE_TUNNEL_PORT`、`WF_API_KEY` | CLI 远端连接配置 |
| `WF_MINI_*`、`WF_TUI_*`、`WF_REASONING`、`WF_USAGE`、`WF_QUIET_TOOLS`、`CLI_OUTPUT_FORMAT` | CLI/TUI 显示、历史、审批超时、动画与输出模式 |
| `WF_PLUGIN_*` | 插件扫描路径、工件与启用开关 |

具体变量名以各模块中的常量定义为准；没有对应文件字段的纯前端旋钮不进入基础设施装配链。

## 服务旧名与非 WF_ 变量

| 环境变量 | 作用 |
|---|---|
| `WF_SERVER_BIND_ADDR` | 服务监听地址；同时支持 `--addr` flag |
| `AUTH_ENABLED` | API Key 鉴权开关 |
| `API_KEYS` | 逗号分隔 API Key 列表（密钥只在这里） |
| `WF_SERVER_PASSWORD` | 服务端会话密码（只从环境读取） |
| `RATE_LIMIT_ENABLED` | 限流开关 |
| `RATE_LIMIT_WINDOW_MS` | 限流窗口 |
| `RATE_LIMIT_MAX_REQUESTS` | 限流最大请求数 |
| `DB_PASSWORD` | Postgres 密码（通过 `${DB_PASSWORD}` 插值） |
| `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` | LLM profile 的 `${...}` 插值来源 |

## 文件内插值

- `${VAR}` / `${VAR:default}` / `${VAR:-default}`：配置文件解析前展开，仅大写变量名匹配。此机制是文本替换，不做 TOML/JSON 转义；含引号、反斜杠或换行的值不应作为裸字符串注入。
- `${VAR}` 未解析时可保留原样；严格模式（`strict_env`）下启动失败。
- `{{parameters.*}}`：工作流执行期替换，不属于配置加载。

## 运行环境默认

`RuntimeEnvironment` 区分 Development / Production。缺失基础设施文件时使用对应环境的优化默认值（存储、指标、预设）；宿主 `config.toml` 的值作为领域默认值优先于代码默认值，环境覆盖与程序化覆盖仍最高。`wf-server` 固定使用 Production 默认值。
