# .wf-user/ — 工作站演示替身（非生产配置）

本目录是显式用户设置目录的工作站演示替身，仅用于本地演示与文档举例。
真实的用户设置目录由调用方显式传入（`--settings-dir`），
或按系统规范解析（`$XDG_CONFIG_HOME/wf`、`~/.wf`）。

服务进程默认不读任何个人状态（含本目录），租户隔离不依赖本目录是否存在。
生产代码不硬编码引用本目录；加载器只认显式传入的目录。

## 与真实用户设置目录的对应关系

| 本目录 | 真实位置 | 说明 |
|---|---|---|
| `config.toml` | `{settings_dir}/config.toml` | 演示用宿主旋钮 |
| `llm-auth/` | `{settings_dir}/llm-auth/` | 认证声明样例（`*.example.toml` 从不加载）与索引格式 |
| `llm-profiles/` | `{settings_dir}/llm-profiles/` | 模型档案样例与索引格式 |
| `llm-providers/` | `{settings_dir}/llm-profiles/` 之外的服务商目录 | 公开端点定义（无秘密），可作为托管目录的来源 |
| `server/`（无物理目录） | `{settings_dir}/server/` | 服务端用户层，样例见 `config-examples/server/`，按需复制 |
| `plugins/example-*` | 用户 `plugins/` 目录 | 插件契约样板，仅用于教学与测试夹具 |

服务端用户层的样例不重复存放在此：唯一来源是 `config-examples/server/`，
复制到 `{settings_dir}/server/` 或项目 `.wf/server/` 后生效。

凭据库默认落点跟随操作人（`~/.wf/auth.json`），跟随人不跟随项目；
服务将凭据库改落到项目 `.wf/` 下以实现租户隔离。
