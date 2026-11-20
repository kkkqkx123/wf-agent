# 日志配置收敛修改方案

本文列出当前日志配置的问题、修改方案和分阶段实施任务。只修改现有模块与部署资产，不新增替代实现，不保留旧配置的兼容映射，版本号保持现状。路径相对仓库根目录。

## 一、问题清单

| 编号 | 问题 | 位置 | 影响 |
|---|---|---|---|
| 1 | `LogOutput::Stderr` / `File` 已建模但无实现，fmt 层始终使用默认 writer；`OutputConfig` 的日志文件、终端、SDK 日志字段和 `WF_OUTPUT_DIR` 也没有消费者 | `crates/app/wf-runtime/src/logger.rs`、`crates/foundation/wf-types/src/config/output.rs`、`crates/infra/wf-config/src/orchestrator.rs` | 配置承诺与运行时行为不一致；部署侧 logrotate 没有输入文件 |
| 2 | 文件层用整个 `LogConfig` 的默认值比较判断是否显式设置；显式传入默认值会被文件覆盖，显式设置格式会让文件级别被忽略 | `crates/app/wf-runtime/src/bootstrap/config.rs` | 违反已声明的优先级，行为随无关字段变化 |
| 3 | 配置解析与告警发生在 `init_tracing` 之前，宽松加载的告警被丢弃 | `crates/app/wf-runtime/src/bootstrap/inner.rs` | 配置错误无声无息，排障困难 |
| 4 | 全局 subscriber 只能设置一次；`Guard` 为空，重复初始化靠错误文本吞掉；文件层的级别无法在初始化后应用 | `crates/app/wf-runtime/src/logger.rs` | 多实例嵌入场景首配生效且后续配置静默失效；测试无法观察最终配置 |
| 5 | 级别字段是无约束字符串；CLI 只接受枚举值，文件层接受任意 EnvFilter 表达式；`warning` 与 `off` 在两条路径行为不同 | `crates/app/wf-server/src/main.rs`、`crates/infra/wf-config/src/file_layer.rs`、`crates/app/wf-runtime/src/logger.rs` | 校验分裂，非法值可能在启动最后阶段才失败 |
| 6 | 没有服务级日志级别环境变量；外部托管子进程却使用 `RUST_LOG` | `crates/infra/wf-config/src/orchestrator_env.rs`、`crates/infra/wf-integration/src/code_context.rs` | 运维入口不统一 |
| 7 | 日志写入 stdout，与 TUI 终端保护和 JSON 输出的 stdout 契约冲突 | `crates/app/wf-runtime/src/logger.rs` | 结构化输出与日志混流；终端界面可能被日志破坏 |
| 8 | 测试只覆盖 builder 字段赋值，不覆盖解析、优先级和默认解析 | `crates/app/wf-runtime/src/logger.rs`、`crates/app/wf-runtime/src/bootstrap/tests/` | 配置回归无法被捕获 |

## 二、目标设计

- 运行时只有一个日志配置模型：级别为受约束枚举，格式为受约束枚举；“未设置”由 `Option` 表达。
- 日志固定写 stderr，关闭 ANSI。stdout 只承载业务输出，由 systemd journald 负责捕获与留存；不再保留进程内文件日志配置与 logrotate。
- 文件层先加载并返回告警，再与程序化配置、环境变量和模式配置一起解析为最终日志配置；安装 subscriber 后再输出加载告警。
- 级别解析只有一处实现；CLI、文件层、环境变量共用它。非法值 fail-fast。
- 全局 subscriber 的首次安装胜出；`Guard` 明确表示进程级生命周期，不再用错误文本判断错误类型，也不假装能卸载 subscriber。

## 三、阶段任务

### 阶段一：重写日志核心

- 在 `crates/app/wf-runtime/src/logger.rs` 定义日志级别枚举与统一解析函数，接受 `trace/debug/info/warn/warning/error/off`，大小写不敏感。
- 将 `LogConfig` 的级别与格式改为 `Option`，默认均为未设置；删除 `LogOutput`。
- `init_tracing` 解析出最终级别（默认 `warn`）与格式（默认 `Full`），使用 stderr writer 且关闭 ANSI。
- `Guard` 保留为零大小类型，语义明确为进程级全局安装；重复安装不再依赖错误文本判断。
- 补齐解析、默认值和格式选择的单元测试。

### 阶段二：修正优先级与初始化时序

- 文件层的 `log_level` 使用同一解析器，非法值返回运行时配置错误；仅在级别未显式设置时填充。
- 新增 `WF_LOG_LEVEL` 环境变量，优先级为：CLI/程序化配置 > `WF_LOG_LEVEL` > 项目 `.wf/config.toml` > 全局 `config.toml` > 默认值。
- `CLI_OUTPUT_FORMAT=json` 仅在格式未显式设置时切换到 JSON；`CLI_OUTPUT_FORMAT=silent` 仍强制 `off`。
- 调整 `bootstrap_inner`：先加载文件层、解析模式与最终日志配置并安装 subscriber，再把已加载的文件层交给基础设施解析，最后输出加载告警。
- 更新所有使用 `LogConfig` 的测试与程序化调用点，改用类型化级别。

### 阶段三：统一入口与文档

- `wf-server --log-level` 改用同一解析器，作为 clap value parser 在参数解析阶段校验；帮助文本包含 `off`。
- 删除 `run` 中重复的级别校验分支。
- 更新 `docs/config/host.md`、`docs/config/environment.md` 与部署环境模板，说明 stderr 输出、`WF_LOG_LEVEL` 与优先级。
- 在部署文档中说明 systemd journald 是日志留存入口，不再由应用写文件。

### 阶段四：移除失效的输出域

- 删除 `wf-types::config::output` 模块及 `OutputConfig`、`LogLevel`、`SdkLogLevel` 类型。
- 从配置编排器、覆盖项、SDK 选项、运行时结构中移除 output 域，以及 `WF_OUTPUT_DIR` 环境变量映射。
- 删除 `output.toml` 示例与模板，更新相关配置文档和测试。
- 删除 logrotate 配置与部署文档中的文件日志章节，移除安装/卸载脚本中不再需要的日志目录锚定。

### 阶段五：集中验证

- 运行 `cargo fmt --check` 与一次 `cargo clippy --all-targets --all-features`，输出重定向到文件后读取结果。
- 核对所有日志相关配置项：文档、模板、环境变量、代码中的实际消费者一一对应，不再存在只有声明没有实现的选项。

## 四、实施状态

五个阶段的改动均已落地：

| 阶段 | 状态 | 说明 |
|---|---|---|
| 日志核心 | 已完成 | `LogLevel` 统一解析，`LogConfig` 字段改为可选，删除 `LogOutput`，日志固定写 stderr 且关闭 ANSI |
| 优先级与时序 | 已完成 | 文件层先加载后初始化；`WF_LOG_LEVEL`、项目/全局 dotfile、程序化配置按字段生效；无效级别 fail-fast |
| 入口与文档 | 已完成 | `--log-level` 使用同一解析器并在参数阶段校验；宿主配置、环境变量与部署文档同步 |
| 失效输出域清理 | 已完成 | `OutputConfig`、`output.toml`、`WF_OUTPUT_DIR`、logrotate 与日志目录锚定全部移除 |
| 验证 | 已完成 | `cargo fmt --all`；`cargo clippy --all-targets --all-features` 通过；logger、文件层、日志优先级、基础设施文件层定向单测通过 |

## 五、明确不做的事

- 不引入 tracing-appender 或进程内日志轮转；文件留存交给 journald。
- 不保留 `LogOutput`、旧日志字段或 `WF_OUTPUT_DIR` 的兼容别名。
- 不改变 `tracing` 事件的字段内容与业务打点位置。
- 不改变模式检测、存储、指标等其他基础设施域的语义。
