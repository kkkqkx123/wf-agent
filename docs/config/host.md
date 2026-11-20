# 宿主旋钮（config.toml）

文件：`{settings_dir}/config.toml` 与 `{project}/.wf/config.toml`（项目优先）。宽松加载：缺失给默认值，坏文件告警跳过。

字段（`FileLayerConfig`）：

- `storage`：`StorageConfig`（可覆盖存储后端）
- `log_level`：日志级别，取 `trace` / `debug` / `info` / `warn`（`warning`）/ `error` / `off`；大小写不敏感，非法值启动失败
- `log_format`：日志格式，取 `full` / `compact` / `json` / `pretty`；大小写不敏感，非法值启动失败
- `tool_approval`：`ToolApprovalConfig`

说明：

- 日志级别优先级：`--log-level` 或程序化配置 > `WF_LOG_LEVEL` > 项目 `config.toml` > 全局 `config.toml` > 默认 `warn`。级别只在未显式设置时由低优先级来源填充。
- 日志格式优先级：`--log-format` 或程序化配置 > `WF_LOG_FORMAT` > 项目 `config.toml` > 全局 `config.toml` > JSON 模式推导 > 默认 `full`。`CLI_OUTPUT_FORMAT=silent` 仍强制关闭日志。
- 环境变量为空字符串时按未设置处理，与部署环境文件中留空的键保持一致。
- 日志固定写 stderr，由服务管理器捕获；应用不写日志文件，不使用 `RUST_LOG`。格式在进程内首次安装订阅者后固定；重复 bootstrap 请求不同格式会启动失败。
- 日志订阅者由 runtime bootstrap 安装，属于进程级单例：同一进程内后续 bootstrap 只重载级别，格式以首次安装为准；宿主不应再自行安装 tracing subscriber，否则 bootstrap 会因 subscriber 冲突失败。
- 该层只包含宿主旋钮；固定基础设施文件仍由 orchestrator 加载并优先生效。
- 文件检查点与沙箱设置故意排除在此，避免 dotfile 用部分值遮蔽固定基础设施文件。
- CLI 前端显示配置位于同目录的 `tui.toml`，由 CLI shared 模块独立加载，不读写 `config.toml`。
