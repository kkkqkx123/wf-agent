# 配置来源与校验收敛方案

本文基于当前配置加载代码与文档的实际差异整理问题清单，并给出按阶段执行的修改方案。目标是让配置来源边界明确、错误策略可预测、程序化覆盖与文件族合并语义一致。修改只落在现有模块，不新增替代模块，不保留旧别名。

## 一、问题清单

| 编号 | 问题 | 位置 | 影响 |
|---|---|---|---|
| 1 | `wf-server` 只在传入 `--config` 或 `--storage` 时构造文件层来源；仅传 `--settings-dir` 时运行时完全绕过文件装配 | `crates/app/wf-server/src/main.rs` | 单机包启动脚本只传 `--settings-dir`，infrastructure、LLM、MCP、Skills 与宿主 `config.toml` 均不生效 |
| 2 | 项目根目录用空路径表示“无项目”，各加载器仍会拼出相对 `.wf` 路径 | `wf-config` 的 orchestrator、file_layer、llm、llm_auth、mcp、skill、keymap | 无显式项目时可能读到进程工作目录下的 .wf 文件，来源边界不可控；MCP 在只有 settings 目录时直接返回空 |
| 3 | 服务端未设置 `runtime_env`，缺失基础设施文件时回退 Development 默认值 | `crates/app/wf-runtime/src/bootstrap/config.rs`、`crates/app/wf-server/src/main.rs` | 服务可能使用开发数据库路径与周期指标上报 |
| 4 | LLM 文件族的格式错误、索引错误被告警跳过 | `wf-config/src/llm.rs`、`llm_auth.rs` | 与文档声明的 fail-fast 不符，拼错配置后静默丢失 profile |
| 5 | 程序化 LLM profile/provider/auth 非空时完全跳过文件族 | `crates/app/wf-runtime/src/bootstrap/config.rs` | 与按 ID 合并语义不符，宿主提供一个记录会遮蔽整个文件族 |
| 6 | `WF_SERVER_BIND_ADDR` 非法时告警并忽略 | `crates/app/wf-server/src/server_config.rs` | 与文档中的监听地址 fail-fast 不符，可能绑定到非预期地址 |
| 7 | `tools.toml` 未知段被当作 passthrough 保存，但该字段没有消费者 | `wf-config/src/orchestrator.rs` | 段名拼写错误不会报错，配置意图丢失 |
| 8 | `ServerConfig::resolve` 的多个子解析重复读取同一个 `server.toml` | `crates/app/wf-server/src/server_config.rs` | 重复 I/O，且文件在不同子步骤之间可能被修改 |
| 9 | 文档与实现漂移：`--infra-dir` 无 CLI 消费者、环境变量清单不完整、错误策略描述不一致 | `docs/config/*` | 运维无法从文档判断真实优先级和失败行为 |

`${VAR}` 目前是有意采用的文本插值，因为它同时支持字符串与裸数字/布尔标量。带引号或换行的密钥不适合作为裸 TOML 值；本次不改成类型感知插值，只在文档中明确约束。

`ConfigDefaults`、`ConfigOverrides`、`SdkOptions` 的字段重复属于各层的公开类型边界，不是加载链路重复；本次不为此改动公共 API。

## 二、阶段一：配置来源边界

- 将 orchestrator、宿主文件层、LLM 文件族、MCP、Skills、Keymap 的项目根目录统一改为可空输入。空值表示完全跳过项目层，不再拼出相对 `.wf` 路径。
- `wf-server` 始终通过带来源的 bootstrap 装配运行时；`--settings-dir` 单独存在时能够驱动全部配置域。
- MCP 在只有 settings 目录时加载全局服务；凭据库在无项目根目录时落到 settings 目录，避免回落到个人 home。
- 服务端显式使用 Production 环境默认值。

## 三、阶段二：错误策略收敛

- LLM profile、provider、auth 文件族遇到非法索引、无法读取或无法解析的记录时直接失败，不再跳过。
- 非法 `WF_SERVER_BIND_ADDR` 直接失败，文件值仍保持 fail-fast；auth、限流等中间件环境变量保留告警并沿用旧值的可用性策略。
- `tools.toml` 只接受已知段，未知段报校验错误；删除无消费者的 passthrough 字段。
- 更新对应单元测试，确保新策略被覆盖。

## 四、阶段三：程序化覆盖按 ID 合并

- 在 `wf-config` 中提供按记录 ID 合并 profile、provider 与 auth provider 的能力；程序化值优先，文件记录补充。
- 运行时装配无条件读取已声明来源中的文件族，再与程序化值合并；显式 `store_path` 优先，文件族只补充 provider 声明。

## 五、阶段四：服务配置加载与文档同步

- `ServerConfig::resolve` 只读取一次 `server.toml`，把解析结果传给各子域。
- 更新 `docs/config/` 下的配置总览、优先级、基础设施、服务、LLM、MCP/Skills 与环境文档，使文档与代码行为一致。
- 清理代码注释中与旧实现相符但已失效的描述。

## 六、阶段五：集中验证

- 执行一次 `cargo fmt --all`。
- 一次运行 `cargo clippy --all-targets --all-features`，输出重定向到文件后读取结果。
- 一次运行受影响 crate 的定向测试，覆盖空项目根、settings-only、非法声明与按 ID 合并。
