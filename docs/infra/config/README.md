# wf-config

> 全量配置文件与字段参考见 [`docs/config/README.md`](../../config/README.md)。


infra 层配置子系统：负责所有配置文件的发现、解析、分层合并、校验、转换与装配。它是配置语义的唯一权威（single source of truth），被 `wf-api`（保存路径）、`wf-workflow`(图校验）、`wf-resource`（自定义资源注册）等上层复用。类型定义（`StorageConfig`、`LimitsConfig` 等）位于 `wf-types::config`，本 crate 只做加载与处理。

## 依赖

`wf-integration`、`wf-types`、`wf-common`、`wf-core`、`wf-metrics`、`wf-script`，以及 serde/toml/serde_json/glob/regex/tokio。保持无网络传输依赖（如 llm_profile 校验中的代理检查为内联镜像实现）。

## 模块结构

### 基础设施

| 模块 | 职责 |
|---|---|
| `error.rs` | `ConfigError` / `ConfigResult` 统一错误类型 |
| `parser.rs` | 按扩展名识别 TOML / JSON（`ConfigFormat`），解析前先做环境变量展开，其余格式报错 |
| `loader.rs` | 单文件异步/同步加载、路径解析、候选文件名探测、glob 展开 |
| `layered.rs` | TOML 递归分层合并：多文件按低→高优先级递归合并（子表字段级合并，支持任意深度部分覆盖），可接入 `ConfigMetricsCollector` 记录访问/耗时/失败 |
| `env.rs` | `WF_*` 环境变量的声明式映射与覆盖应用 |
| `layout.rs` | 配置子系统文件系统布局的唯一出处（见下） |
| `file_layer.rs` | 用户级配置层：全局 `config.toml` + 项目 `.wf/config.toml`（项目优先），宽松加载——缺失给默认值、坏文件告警跳过而非阻断启动 |
| `storage_spec.rs` | 存储后端 spec 解析的唯一构造规则：`memory \| sqlite \| sqlite:<path> \| postgres:<conn> \| postgres://...` |
| `validator.rs` | 基于 `wf_types::hook` 统一 hook 类型注册表的校验，及聚合校验结果（计入 config metrics） |

### 目录布局（layout.rs）

- 项目层根目录：`{project_root}/.wf`；易变目录 `.wf/meta`（会话缓存等）与 `.wf/tmp`（超长输出溢写）内自动维护 `.gitignore`（内容 `*\n!.gitignore`），已有自定义内容不覆盖。
- 配置家族目录（`family` 模块）：`infrastructure`、`server`、`llm-auth`、`llm-profiles`、`llm-providers`，各自从用户 settings 层与项目 dotfile 层解析。
- 索引文件统一名 `index.json`；示例文档目录 `config-examples/` 永不加载。

### 配置索引（index.rs + index/）

索引文件只含路径模式，元数据从各配置文件本身提取：

- `load.rs`：加载索引文件（路径或目录内 `index.json`）、把 glob 展开为绝对路径并去重；`*.example.*` 样例文件永不匹配。
- `metadata.rs`：从 LLM Profile / Auth / Provider 定义中提取 `id`、名称、标签等元数据。
- `registry.rs` / `resolvers.rs`：每类型一个 resolver（parse + validate + transform → 提取元数据），注册进共享 `IndexRegistry`；单文件失败记为索引 failure，不中断整体。
- `filter.rs`：按标签（需全部满足）、分类、id 过滤解析结果。

### 领域文件加载

| 模块 | 职责 |
|---|---|
| `mcp.rs` | MCP 设置：全局 `{settings_dir}/mcp-settings.json` 与项目 `.wf/mcp.json` 合并，项目优先 |
| `skill.rs` | Skill 设置：同上模式，`skill-settings.json` + `.wf/skills.json` |
| `keymap.rs` | 按键绑定：独立 settings 文件中 `context → action → chord` 字符串，解析为 `Chord`；不感知 `tui-core` 类型，由上层转换 |
| `llm_auth.rs` | LLM 认证声明：`llm-auth/` 家族（index.json + 每 provider 一个 toml）聚合为 `AuthConfig` |
| `llm.rs` | LLM Profile 与 Provider 定义家族（`llm-profiles/`、`llm-providers/`，每记录一文件，支持 index.json 或扁平 glob） |

### 装配编排器（orchestrator*.rs）

- `orchestrator.rs`：装配入口。按固定域文件名（`InfrastructurePresetFiles`：storage / metrics / output / sandbox / file-checkpoint / tool-approval / presets / tools / limits）从"显式覆盖 → 用户 settings 层 → 项目层"加载，应用 `WF_*` 环境覆盖，产出完整装配配置。缺失文件回退代码默认值；覆盖与环境永远胜过文件。`ToolConfigs` 聚合 read_file/glob/list_files/shell 与 code-context 段。
- `orchestrator_loader.rs`：每域加载——缺失文件用调用方默认值，存在但解析失败则报错（防拼写错误静默跑默认值）。
- `orchestrator_env.rs`：`WF_*` 环境变量到配置字段的声明式映射（高改动频率清单独立成文件）；未知取值 fail-fast。

### 配置处理器（processor/）

纯函数：校验、转换、合并、参数替换。processor.rs 顶部的模块总览即权威清单：

- **基础设施域**：`infrastructure`（storage/metrics/output/sandbox 按运行环境合并默认值）、`limits`（agent 迭代上限 1000 / 默认 10、子代理深度 8、workflow 循环上限 10000 等，与引擎硬编码常量对齐）、`presets`、`sandbox_global`（profiles + 路由规则校验；程序化覆盖合并后仍再走校验）、`checkpoint` / `file_checkpoint`（空白可选路径视为未设置以继承；拒绝空白工作区根、非法 ignore 模式、无工作区的手动 watcher；`gc_interval_secs = 0` 等显式禁用语义被接受）、`file_scope`（执行文件范围合并/校验/展开，规范实现在 `wf_types::config::file_scope`，显式空列表表示全部拒绝）。
- **LLM 域**：`llm_profile`（provider 定义加载时校验必填字段）、`llm_auth`（声明不含密钥：只有端点、client id、环境变量名，错误在加载期暴露）、`decision_profile`。
- **Workflow 域**：`workflow`（定义校验、节点/边转换）、`node_config`（按节点类型校验：LLM / Script / Variable / Route / Fork / Join / Subgraph / Interaction / AgentLoop 等；插件贡献的未知类型免检；issue 携带点分路径如 `nodes.n1.config.profile_id`；`${path}` / `{{path}}` 引用形状错误在注册期失败）、`node_template`、`agent_loop`（引用的 profile_id 必须存在于已注册集合）。
- **扩展域**：`hook`（四种 hook 形态统一收敛到 `CanonicalHookSpec` 后校验：未知类型、负权重、空 handler 名拒绝）、`tools`（内置文件工具 glob/list-files/read-file 校验转换）、`tool_list`（available/hidden/discoverable 三分区不得相交，agent 与 workflow 共享语义）、`trigger`（模板/动作/scope/限额校验；有序多效果声明未实现、拒绝开启以防"校验通过但运行时忽略"）、`prompt`（模板校验，同名模板与片段以模板优先，与渲染期并集一致）、`script` / `script_flow` / `script_interactive`。
- **替换**：`substitute`——装配期仅字符串字段的 `{{parameters.*}}` 替换；占位符泄漏进类型化字段时反序列化失败并指明类型错配，而非泛化报错；可收集首次出现顺序的参数名与形似但形状非法的 span。

## 优先级总则

显式覆盖 > 环境变量（`WF_*`） > 项目层（`.wf/`） > 用户 settings 层 > 代码默认值。分层 TOML 合并为字段级递归合并；领域配置文件"存在但坏"报错、"缺失"回退默认值；用户级全局层（file_layer）采用宽松策略。
