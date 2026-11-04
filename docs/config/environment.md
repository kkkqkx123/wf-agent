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
| `WF_OUTPUT_DIR` | 输出目录 |
| `WF_FILE_CHECKPOINT_ENABLED` | 文件检查点开关 |
| `WF_FILE_CHECKPOINT_WORKSPACE_ROOT` | 文件检查点工作区根 |
| `WF_FILE_CHECKPOINT_MANUAL_WATCH` | 手动 watcher 开关 |
| `WF_FILE_CHECKPOINT_MANUAL_POLL_MS` | 手动 watcher 轮询间隔 |
| `WF_FILE_CHECKPOINT_GC_INTERVAL_SECS` | GC 自动运行间隔 |
| `WF_SANDBOX_MODE` | 沙箱模式（disabled/lenient/strict） |

所有 `WF_*` 覆盖 fail-fast：非法值启动失败。

## 冻结旧名（无 WF_ 前缀）

| 环境变量 | 作用 |
|---|---|
| `WF_SERVER_BIND_ADDR` | 服务监听地址（新名）；同时支持 `--addr` flag |
| `AUTH_ENABLED` | API Key 鉴权开关 |
| `API_KEYS` | 逗号分隔 API Key 列表（密钥只在这里） |
| `RATE_LIMIT_ENABLED` | 限流开关 |
| `RATE_LIMIT_WINDOW_MS` | 限流窗口 |
| `RATE_LIMIT_MAX_REQUESTS` | 限流最大请求数 |
| `DB_PASSWORD` | Postgres 密码（通过 `${DB_PASSWORD}` 插值） |
| `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` | LLM profile 的 `${...}` 插值来源 |

## 文件内插值

- `${VAR}` / `${VAR:default}`：配置文件解析前展开，仅大写变量名匹配。
- `${VAR}` 未解析时可保留原样；严格模式（`strict_env`）下启动失败。
- `{{parameters.*}}`：工作流执行期替换，不属于配置加载。

## 运行环境默认

`RuntimeEnvironment` 区分 Development / Production。缺失基础设施文件时使用对应环境的优化默认值（如存储、输出、指标、预设、沙箱模式）。
