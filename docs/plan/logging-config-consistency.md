# 日志配置一致性修正方案

本文基于当前代码的实际行为列出日志配置问题与修正方案。修改只落在现有模块与部署资产上，不新增替代模块，不保留旧配置别名，开发期版本号保持不变。路径相对仓库根目录。

## 一、问题清单

| 问题 | 位置 | 影响 |
|---|---|---|
| `WF_LOG_LEVEL=` 在 EnvironmentFile 中是空字符串而非未设置；`resolve_log_config` 直接交给 `LogLevel::parse`，空串被判为非法值 | `crates/app/wf-runtime/src/bootstrap/config.rs`、`deploy/env/wf-server.env.template` | 按部署模板原样生成环境文件后，服务启动失败；与模板注释和通用 `WF_*` 覆盖的“空值跳过”语义不一致 |
| 日志格式没有外部配置入口；`Compact` / `Pretty` 只能程序化使用，JSON 只能靠 `CLI_OUTPUT_FORMAT=json` 间接推导 | `crates/app/wf-runtime/src/logger.rs`、`crates/app/wf-runtime/src/bootstrap/config.rs` | 服务端日志格式绑定 CLI 输出语义，运维无法直接选择格式 |
| `LogFormat` 没有与 `LogLevel` 对称的解析器和 fail-fast 校验 | `crates/app/wf-runtime/src/logger.rs` | 新增格式配置后会出现两套校验逻辑和不同的错误处理 |
| 全局 subscriber 首次安装胜出；重复 bootstrap 的级别、格式、silent 设置被静默忽略 | `crates/app/wf-runtime/src/logger.rs` | 嵌入式多实例与测试中，后传入的配置看似生效实际不生效 |
| 配置文件解析失败告警已在 subscriber 安装后输出，但级别字符串非法时错误发生在安装前；需要保持这一时序不回退 | `crates/app/wf-runtime/src/bootstrap/inner.rs` | 修正解析逻辑时若调整顺序，会让配置错误重新变得无声 |
| `SdkOptions.log_level: Option<String>` 声明后没有任何消费者，也不走统一解析器 | `crates/app/wf-runtime/src/sdk_options.rs` | 对外暴露了不可兑现的宿主旋钮 |
| `FailurePolicyConfig.log_level` 只有构造和默认值，没有进入 tracing 管线，也不控制任何日志级别 | `crates/foundation/wf-types/src/execution/failure_policy.rs` 及其测试构造点 | 领域配置中出现无消费者的“日志级别”，与宿主日志概念混淆 |
| CLI `--log` 实际是命令输出 tee，不是 tracing 日志落盘；旧 CLI 文档仍写 `-v/--verbose`、`-d/--debug`、`-l/--log-file` | `crates/app/cli/wf-cli-shared/src/args.rs`、`docs/cli/01-功能清单.md`、`docs/app/cli/README.md` | 用户按文档找不到参数，或误以为 CLI 能配置运行时日志 |

## 二、目标设计

- 宿主日志只保留两个可配置项：级别与格式。两者都是受约束枚举，共用同一套解析入口，非法值 fail-fast。
- 空的环境变量值按“未设置”处理，与 `wf-config` 中其他 `WF_*` 覆盖保持一致；未设置时继续按低优先级来源填充。
- 级别优先级为：程序化配置 / CLI > `WF_LOG_LEVEL` > 项目 `config.toml` > 全局 `config.toml` > 默认 `warn`；格式优先级相同，但最终默认值为 `Full`，JSON 模式仅在所有显式来源都未设置时生效，`silent` 模式仍强制 `off`。
- 日志固定写 stderr、关闭 ANSI；这一契约不变，文件留存仍由 journald 负责。
- 全局 subscriber 仍只安装一次，但级别改为可重载；重复 bootstrap 会更新过滤级别。格式在安装后不可变，请求不同格式时返回明确错误，不再静默忽略。
- 删除无消费者的日志字段；CLI 的 `--log` 保留原名，但在帮助与文档中明确它是输出 tee，与 tracing 日志无关。

## 三、修改阶段

### 阶段一：日志核心

- 为 `LogFormat` 增加与 `LogLevel` 对称的解析和字符串转换，接受 `full` / `compact` / `json` / `pretty`，大小写不敏感，其余值一律拒绝。
- 用 `tracing-subscriber` 自带的可重载过滤层替换静态 `EnvFilter`，不新增依赖 feature。
- 首次初始化安装 subscriber 并持有过滤器句柄；后续初始化更新级别，遇到格式不一致或已有外部 subscriber 时返回 `RuntimeError::Logger`。
- 保持 stderr writer、target 开关、ANSI 开关和线程字段设置不变。
- 补充格式解析与重复初始化的单元测试。

### 阶段二：配置入口与优先级

- 在文件层增加 `log_format`，在运行时解析 `WF_LOG_FORMAT`，并让 `wf-server` 接受 `--log-format`。
- 级别与格式都经过同一个“环境变量优先、空值视为未设置、再回落文件层”的选择逻辑。
- `resolve_log_config` 先解析程序化之外的级别与格式，再交给模式调整；非法环境变量或文件值直接返回配置错误。
- 更新宿主配置文档、环境变量文档、部署环境模板与部署说明，列明新字段、空值语义和优先级。

### 阶段三：死配置与歧义清理

- 删除 `SdkOptions.log_level` 和 `FailurePolicyConfig.log_level`，同步清理所有测试构造点。
- 更新 CLI `--log` 的帮助文本，说明它只复制渲染后的命令输出。
- 更新 CLI 文档中的全局参数描述，去掉不存在的短参数，避免把输出 tee 写成运行时日志。
- 在既有日志收敛方案文档顶部标注后续修正的承接关系。

### 阶段四：集中验证

- 运行 `cargo fmt --all`。
- 一次运行 `cargo clippy --all-targets --all-features`，输出重定向到文件后读取结果。
- 一次运行日志相关包的定向单测，覆盖解析、优先级、空环境变量和重复初始化。
- 核对文档、模板、环境变量与代码消费者一一对应，不再有只声明不生效的日志旋钮。

## 四、明确不做的事

- 不引入进程内日志文件、日志轮转或 `tracing-appender`。
- 不恢复 `RUST_LOG` 作为宿主进程的日志入口；托管子进程使用自己的环境变量不属于宿主日志配置。
- 不引入 `dyn Layer` 或拆出新模块来包装 subscriber；格式保持进程级不可变。
- 不改变业务事件字段和现有 tracing 打点位置。
- 不重命名 CLI `--log`，避免无收益的参数迁移。
