# 宿主旋钮（config.toml）

文件：`{settings_dir}/config.toml` 与 `{project}/.wf/config.toml`（项目优先）。宽松加载：缺失给默认值，坏文件告警跳过。

字段（`FileLayerConfig`）：

- `storage`：`StorageConfig`（可覆盖存储后端）
- `log_level`：日志级别，取 `trace` / `debug` / `info` / `warn`（`warning`）/ `error` / `off`；大小写不敏感，非法值启动失败
- `tool_approval`：`ToolApprovalConfig`

说明：

- 日志级别优先级：`--log-level` 或程序化配置 > `WF_LOG_LEVEL` > 项目 `config.toml` > 全局 `config.toml` > 默认 `warn`。级别只在未显式设置时由低优先级来源填充。
- 日志固定写 stderr，由服务管理器捕获；应用不写日志文件，不使用 `RUST_LOG`。
- 该层只包含宿主旋钮；固定基础设施文件仍由 orchestrator 加载并优先生效。
- 文件检查点与沙箱设置故意排除在此，避免 dotfile 用部分值遮蔽固定基础设施文件。
- CLI 前端显示配置位于同目录的 `tui.toml`，由 CLI shared 模块独立加载，不读写 `config.toml`。
