# wf-server 测试覆盖后续分析

> 范围：`crates/app/wf-server`。基于上一个提交已落地的启动配置、提取器、流协议、票据与路由契约测试，重新盘点剩余缺口并给出下一轮的单元与集成测试清单。

## 当前覆盖盘点

按测试属性静态统计，当前有 340 个单元测试与 20 个集成测试；上一轮之前分别为 313 与 9。单元测试中约 101 个集中在 `backends/*`，144 个在传输基础设施（`server_config`、`directory`、`envelope`、`middleware`、`static_files`、`openapi`、`ws`、`detached` 等），API 领域层约 95 个。

按 API 子域看，`observation`、`system`、`workflow` 各有十余个单测，`agent` 有 10 个（含 agent loop 请求映射与路由可达性）；`checkpoint`、`llm`、`entity` 仍为零单测，`trigger`、`template` 各只有 1 个。源码中仍有 39 个百行以上文件没有任何直接单测，约 1.37 万行，绝大多数是领域处理器。

上一轮已经补齐的通道：CLI 参数解析与失败退出、路径占位符、SSE/WebSocket 帧与游标、票据铸造与一次性消费、文件系统路径归属、backend upsert 与禁用态、会话版本条件、实体变量 CRUD、模板库列表、未知执行体包络。这些路径不重复展开。

## 覆盖判断

传输层与后端管理面的守卫已经足够密集，`openapi.rs` 的快照和路由一致性测试使新增测试不宜按 OpenAPI 操作数量堆砌。剩余风险不在“有没有路由”，而在三类行为：

- 安全判定散落在领域处理器内部的纯函数里，尚未被直接断言，尤其是 Webhook 令牌校验和 PTY WebSocket 握手授权；
- 破坏性操作与响应裁剪的边界条件只依赖处理器语义，没有在传输边界固定失败条件，例如文件检查点 rebase 的确认开关和检查点链截断；
- 目录作用域与执行体存在性门禁是跨模块中间件，现有契约测试使用无作用域路由器，双节点测试只覆盖恢复路径的一条作用域分支。

## 下一轮单元测试

优先为下列已有纯函数补充直接单测，测试边界停在请求解析、授权判定、状态与帧映射，不重测引擎语义。

- `api/trigger/hooks.rs`：覆盖 `check_auth` 的无认证、令牌认证、`x-hook-token` 与 `Authorization: Bearer` 两种来源、缺失与错误令牌；覆盖 `fold_body` 的非对象输入、白名单筛选、缺失键与无映射时的整体透传；覆盖 `constant_time_eq` 等长与不等长比较。这是外部入口的鉴权，优先级最高。
- `api/pty.rs`：覆盖 `authorize_pty_handshake` 的有效票据、错误作用域票据、非法来源和票据缺失时回退到 API key 的路径；覆盖 `authenticate_connection` 在认证关闭、header/query 传 key、查询开关关闭和来源不合法时的判定；顺带断言 PTY 状态与标题来源的转换往返。
- `api/agent/loops/detached_handlers.rs`：覆盖 `principal_for` 使用配置头名与缺失 key 的归桶结果、`event_frame` 的精确 SSE 帧格式、`is_terminal` 的六种终态与非终态，以及 `SubscriberGuard` 释放后订阅计数回落。配额与回收主体逻辑已有单测，不再重复。
- `api/checkpoint/file_sync.rs`：覆盖 `rebase_baseline_config` 的确认开关、borrow/copy 模式、非法模式，以及 branch、commit_prefix、behind、tag 的透传；同时断言请求体的默认值反序列化。该文件承载破坏性 rebase 的输入守卫。
- `api/checkpoint/checkpoints.rs`：覆盖 `cap_chain` 在硬上限边界与超限时的截断标记、总数与保留数量，以及 transitions 同步截断、时间范围不变。
- `api/observation/listing.rs`、`logs.rs`、`artifacts.rs`：覆盖 `parse_filter` 的状态与执行类型解析和非法值拒绝、其余过滤字段透传；覆盖 `parse_event_types` 的逗号分隔、空白项、未知事件类型；覆盖 `parse_kind` 的合法别名与未知值，以及领域到执行类型的映射。
- `api/agent/definitions.rs`：覆盖 `page_registry` 的无 limit、limit 为零、offset 越界、offset 单独出现被忽略，以及查询条件委托给共享模板过滤器后的匹配结果。
- `api/llm/scripts.rs`：覆盖 `ScriptExecuteBody` 的默认参数映射与可选字段透传，确保 `timeout_ms`、`max_output_bytes` 等带单位字段不会在序列化边界丢失。
- 扩展 `api/agent/loops/tests.rs` 中已有的 `params_from_body` 用例，补齐显式文件作用域逃逸、图片/文本附件校验、缺少目录作用域被拒这三条 wiring 断言；空白消息、模型默认值、临时模式和检查点间隔已覆盖。

次要项：`api/system/retention.rs` 的 dry-run 默认值与 `api/template/locks.rs` 的空 owner 校验可在相关领域测试时顺带补上，不单独成轮。

## 下一轮集成测试

集成测试继续走内存上下文与路由器直连，只有必须观察真实进程或端口行为时才启动服务器。

- 新增目录作用域契约测试：构造无默认作用域与有默认作用域两种 `DirectoryContext`，断言非法目录返回 `LOCATION_NOT_FOUND`、不在允许列表的路径返回 `FORBIDDEN`、服务未就绪返回 `SERVICE_UNAVAILABLE`；断言缺少作用域的 workspace 绑定请求被拒；向内存存储写入一条执行记录后，验证执行视图门禁仅在作用域匹配时通过，非 GET 请求不受该门禁影响。
- 新增 Webhook 入口契约测试：未知 hook 返回统一错误包络；注册一个令牌认证 hook 后，缺失与错误令牌被拒，两种合法头部都能触发；创建型目标回显 `fired` 与 `fire_id`，执行体作用域目标缺少 `execution_id` 时返回参数错误。
- 扩展现有 `tests/http_api_contract.rs`：补一条实体任务或消息的创建、查询、过滤、删除与删后不可见往返；补一条节点模板草稿的保存、读取、提升、版本回滚与删除往返，以及锁冲突时 owner 不变。这两条覆盖状态迁移，而非重复变量 CRUD。
- 扩展现有 `tests/http_auth_stream.rs`：用纯函数层断言流票据作用域不能跨 PTY 会话复用即可，不启动真实 PTY 子进程或建立永久挂起的 WebSocket。
- 低优先烟测：在默认 feature 下遍历 PTY 列表、Git 只读注册表、文件检查点分区等只读空状态端点，断言不出现 500。路由存在性已由 OpenAPI 一致性测试守卫，此项只在需要验证 feature 组合时做。

## 前端使用路径带来的增量

上一个画布修复提交同时改了前后端：后端把节点模板列表改为“存储记录 + 运行时注册表”合并来源，`wf-api` 补了合并函数单测，前端新增了队列规范化与拓扑配置透传两个 Vitest 用例；后端还把 SSE 准入计数器的生命周期绑到响应流上，并补了对应单测。这几条已有直接守卫，不需要重复。

但前端真正走到的几个后端边界仍缺传输层断言，应在上一节清单之外优先补上：

- `api/template/templates.rs`：当前只有空上下文下的可达性断言，没有检查节点模板列表的合并来源与分页包络。补一条用例：同名模板以存储记录为准、注册表独有条目仍可见；`q`、`node_type` 过滤与 `limit`/`offset` 分页后的 `items`、`has_more` 与行字段（`id`、`name`、`node_type`、可选 `description`）稳定。Add Node 抽屉依赖的正是这条契约。
- `api/template/node_drafts.rs`：`POST /api/v1/templates/node/validate` 是入队前的预检，尚无直接测试。补一条用例：非法模板返回带字段定位的 issue 列表，合法模板返回空列表，错误请求体走统一 400 包络。
- `api/workflow/drafts.rs` 与 `api/workflow/workflows.rs`：画布插入后的静默编译检查使用唯一草稿 id，保存时又要把节点位置写回布局。补一条覆盖草稿校验结果、正式保存后 `config.template_id` 仍在节点配置里、以及 `GET/PUT/DELETE /workflows/{id}/layout` 往返与未知工作流 404 的用例。现有的工作流 CRUD 用例只查了列表、导出和删除。
- `api/template/locks.rs` 或 `api/workflow/locks.rs`：控制器现在先取编辑锁再消费待插入队列。锁核心逻辑已有单测，但缺处理器级断言：冲突时原 owner 不变、心跳只续租持有者、释放后他人可获取。

前端侧的控制器编排（添加节点时的默认类型/名称/模板引用、锁定时的意图保留、布局合并、预检失败不入队）尚无 Vitest 用例；它与 `wf-runtime` 启动时把存储节点模板回填注册表的行为一样，属于各自 crate 的门禁，建议单独排期，不塞进 wf-server 本轮。

## 暂不补充

- 为所有 OpenAPI 操作逐个建测试：低价值重复，由路由一致性测试与代表性契约测试共同守卫。
- 真实 SIGTERM、SSH tunnel、PTY 子进程收敛和双节点新场景：成本高且相邻测试已覆盖主体逻辑。
- 客户端 E2E 黄金输出：属于 CLI 前端链路，与本轮 Rust 测试门禁分开推进。

## 本轮实施结果

已补 26 个单元测试和 9 个集成测试：覆盖 Webhook 鉴权与请求折叠、PTY 握手授权、detached 流终态与订阅计数、文件检查点 rebase 校验、检查点链截断、观察查询解析、注册表分页、脚本参数映射、节点模板列表合并与分页、节点模板预检，以及目录作用域、Webhook 入口、实体任务、画布定义/布局、编辑锁和节点模板草稿生命周期。

同时修复了 `main.rs` 中环境变量测试的竞态：配置覆盖用例与 kill-switch 用例现在共享同一把进程内锁，并显式设置再清理 `WF_USER_GIT_ENABLED`。

devs 远端在 `cargo check --locked --all-targets` 与 `cargo test --locked -p wf-server` 下全部通过：库单测 362、二进制单测 4、集成测试 29、文档测试 0。
