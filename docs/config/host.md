# 宿主旋钮（config.toml）

文件：`{settings_dir}/config.toml` 与 `{project}/.wf/config.toml`（项目优先）。宽松加载：缺失给默认值，坏文件告警跳过。

字段（`FileLayerConfig`）：

- `storage`：`StorageConfig`（可覆盖存储后端）
- `log_level`：日志级别字符串
- `tool_approval`：`ToolApprovalConfig`

说明：

- 该层只包含宿主旋钮；固定基础设施文件仍由 orchestrator 加载并优先生效。
- 文件检查点与沙箱设置故意排除在此，避免 dotfile 用部分值遮蔽固定基础设施文件。
- CLI 前端显示配置位于同目录的 `tui.toml`，由 CLI shared 模块独立加载，不读写 `config.toml`。
