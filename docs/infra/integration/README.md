# wf-integration

infra 层外部服务集成 crate：负责以统一的 HTTP 传输 + 本地托管进程（sidecar）方式对接外部服务，并内置「代码上下文服务（code context）」的完整客户端适配层。它只做传输与线协议适配，不承载领域策略——预算、默认值等策略留在调用方适配器中。当前唯一的业务通道是 code-context 服务（外部 CCE server），服务两个消费者方向：压缩路径的文件折叠阶段，以及预定义代码检索/导航工具。

## 依赖

`wf-types`、`wf-common`，以及 serde/serde_json/reqwest/tokio（process、net、io-util 特性）/tokio-util/tracing。是 infra 层的叶子型 crate：不依赖其他 infra 包，反被 `wf-config`、`wf-resource`、`wf-execution-shared`、`wf-tools`、`wf-workflow`、`wf-runtime` 等复用。

## 模块结构

### transport — 传输层

| 模块/项 | 职责 |
|---|---|
| `transport.rs` | 传输模型：`TransportMode`（`External` 连接独立部署地址 / `Managed` 由运行时启动并托管本地进程）；`ServiceTransport` 描述一次服务调用的整体超时、地址与托管参数（二进制名、回环端口、启动上限），并提供 `is_usable` / `external_base_url` / `with_managed_address`（托管模式拿到地址后归一化为外部模式，下游只见外部传输） |
| `http_client` / `post_json` | 构建带客户端级超时的 JSON HTTP 客户端；单次调用整体超时，任何失败（不可达、超时、状态码、解码）都折叠为字符串错误，由调用方映射到各自的降级路径 |
| `probe_http` | 就绪探测：任意完成的 HTTP 响应即视为进程存活（状态码由调用方关心，降级内容也算活着） |
| `pick_loopback_port` | 绑定临时监听器挑一个空闲回环端口（best-effort，仅适合单机回环） |
| `transport/sidecar.rs` | 托管进程生命周期：`SidecarSpec`（程序、参数、环境、就绪 URL、启动上限、重启预算、随机关联的临时文件）→ `start_sidecar` 拉起子进程、轮询就绪 URL、随后后台监督——意外退出按指数退避重启直至预算耗尽，关闭令牌触发时终止进程并清理临时文件。`RunningSidecar` 持有 base_url，优先显式 `shutdown()`（未显式关闭依赖 `kill_on_drop` 兜底）。裸二进制名经 `PATH` 解析，缺失即快速失败 |

sidecar 只负责进程生命周期，不涉及线协议；线契约始终是普通 HTTP。

### code_context — 代码上下文客户端

| 项 | 职责 |
|---|---|
| `code_context.rs` | 配置与策略：`CodeContextConfig`（`tools.toml` 的 `[code_context]` 段，含主开关 `enabled`、传输、fold/retrieval 两个通道策略；整体缺失或 `enabled == false` 表示服务停用，所有消费方静默跳过）。`FoldPolicy`（进入折叠的最小 token 门槛、单条 token 预算、单批条目上限、失败重试轮数）、`RetrievalPolicy`（默认项目 id、默认条数、调用方上限——服务仍是最终执行者并按自身硬限截断）。另提供 `write_cce_server_config`（写最小服务端配置钉住回环地址，返回 `CCE_CONFIG` 环境变量所需的路径）与 `cce_server_command`（托管启动行：只靠 `CCE_CONFIG` 环境变量传配置，无 CLI 参数） |
| `code_context/retrieval.rs` | 检索通道：混合搜索（`/api/search`）与 BM25 关键词搜索（`/api/tools/keyword-search`）的请求构造、响应归约与参数校验。归约只暴露对模型导航安全的字段（文件路径、行区间、片段、分数、类型/来源标记），内部连接键（数字实体 id、分段 id）不越服务边界；片段按 `MAX_SNIPPET_CHARS` 截断并逐条标记 `truncated` 而非静默丢弃。`clamp_limit` 依策略预算收敛调用方条数（默认 → 上限 → 服务硬限） |
| `code_context/navigation.rs` | 符号导航通道：`/api/tools/symbols`、`/api/tools/references`、`/api/tools/definition` 三个端点的请求构造（路径数上限 32、符号树深度 8 / 每层子节点 32）与响应归约（隐藏数字键，剥离内部结构）。提供工具参数校验（`require_path` / `require_line` / `optional_column` / `resolve_paths` 等）与异步调用入口 |
| `code_context/fold.rs` | 折叠通道（压缩路径）：`tool_call_paths` 从消息流恢复 tool-result 对应的文件路径线索；`select_fold_candidates` 选出估算 token 达门槛的 tool-result 消息（纯函数，token 估算由调用方注入；非 tool 消息不动以保全对话结构）；`infer_language_hint` 按扩展名给服务端语言提示（仅线索，未知后缀回退服务端推断）；`FoldClient::fold_batch` 发批量折叠请求（`/api/tools/fold-batch`）；`apply_fold_results` 把结果按 id 写回快照副本（未知 id 忽略）；`build_fold_notice` 生成模型必须看到的压缩边界提示（骨架保留签名与结构、省略函数体，原文不可再得） |

## 语义约定

- **失败一律字符串化**：`post_json` 与各适配器把一切失败折叠为 `String`，由上层映射到自己的降级路径（跳过 / 告警），本 crate 不定义错误枚举。
- **服务是最终执行者**：本 crate 侧的策略（折叠门槛、检索上限）只是调用方意图预算，服务端按自身硬限再次收敛。
- **托管归一化**：`Managed` 传输在 sidecar 就绪后立刻转成等价的 `External` 配置，下游消费者永远只见外部传输形态。

## 测试

各模块内联 `#[cfg(test)]`：服务端配置写入、检索/导航请求与响应形状、参数校验、候选选择与结果回写、sidecar 启动超时与干净关闭（用临时 TCP 响应器模拟就绪）。
