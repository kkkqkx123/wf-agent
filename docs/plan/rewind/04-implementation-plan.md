# 回溯实施计划

> 目标：按阶段把回溯能力落进现有分层，每阶段可独立验收、可停下不回退。
> 约束：不新建 crate，不新建 `mod.rs`，不在代码注释里引用文档结构；代码与注释用英文，文档用中文。

## 阶段一：会话锚

| 改动落点 | 内容 |
|---|---|
| `wf-types/src/storage/message.rs` | 消息元数据新增会话标识与消息序号；两者可选，缺省视为历史数据 |
| `wf-types/src/agent_execution.rs`（运行参数所在文件） | 运行参数新增可选会话标识 |
| `wf-storage/src/adapter/message.rs` 及其实现 | 新增按会话读取与按会话计数的查询；保留按运行读取 |
| `wf-api/src/agent/agent_message.rs` | 新增按会话的历史读取，跨运行按序号升序拼接；现按运行读取保留 |
| `wf-api/src/agent/agent_execution/lifecycle.rs` | 运行与流式入口接收会话标识，缺省时生成；把会话标识下传到运行参数 |
| `wf-api/src/agent/agent_execution/checkpoint.rs` | 会话持久化改为只追加本回合新增消息，并对客户端回传历史做幂等去重；序号取自引擎快照而非数组下标 |
| `wf-server/src/api/agent/loops/lifecycle.rs` | 请求体新增可选会话标识，透传到运行参数 |
| `wf-client/src/sse.rs` | 命令行流式调用带上会话标识 |
| `wf-cli-shared/src/turn.rs`、`wf-tui/src/interactive/turn.rs` | 回合参数携带标签页会话标识 |
| `web-app/src/lib/services/streaming.ts`、`chat-session.svelte.ts` | 网页流式调用带上当前会话标识 |

验收：

- 同一会话连续三个回合后，按会话读取历史得到完整、有序、无重复的消息。
- 现有按运行读取、续跑、自动续跑、回放、标签页重连不回归。
- 未传会话标识的旧调用仍然可用。

## 阶段二：账本与捕获接缝

| 改动落点 | 内容 |
|---|---|
| `wf-types/src/checkpoint/rewind.rs`（新增） | 回溯点、文件侧状态、作用域、账本页、预览、回执、错误分类 |
| `wf-types/src/config/rewind.rs`（新增） | 启用、保留点数、边界补拍、预算、余量下限；接入统一配置聚合 |
| `checkpoint-file/src/storage/migrations.rs` | 新增回溯点与回溯事务两张表及索引，保持幂等 |
| `checkpoint-file/src/rewind/ledger.rs`、`checkpoint-file/src/rewind/refs.rs`（新增） | 账本读写、分页、裁剪；点的持久引用写入与删除 |
| `wf-checkpoint/src/rewind.rs`、`wf-checkpoint/src/rewind/capture.rs`（新增） | 回合起止登记、按回合选取链上检查点、缺失时补拍、变更计数 |
| `wf-agent/src/checkpoint/coordinator.rs` | 暴露回合开始与回合终结的通知调用，不承载账本逻辑 |
| `wf-api/src/agent/agent_execution/lifecycle.rs` | 运行与流式入口在开始与终结处调用门面；终结覆盖成功、失败、取消 |

要点：

- 写点顺序固定为“引用 → 账本”，失败删刚写的引用。
- 补拍在阻塞线程池执行并受超时约束；补拍未完成时点为 `Pending`。
- 取消与失败也要落点，避免账本出现空洞。

验收：

- 成功、失败、取消三种回合都产生回溯点。
- 关闭常规检查点但开启边界补拍时，仍能产生可用的文件侧树。
- 账本写入失败后列表回到上一版，引用不残留。
- 裁剪 101 个点后，剩余点仍可解析。

## 阶段三：应用与事务

| 改动落点 | 内容 |
|---|---|
| `checkpoint-file/src/git_store/objects.rs` 或同级新文件 | 批量探测树闭合（跳过 gitlink），返回缺失清单 |
| `checkpoint-file/src/rewind/restore.rs`（新增） | 恢复树捕获、与回合后树的差异、按目标树选择性恢复、恢复后哈希校验 |
| `checkpoint-file/src/rewind/transaction.rs`（新增） | 事务三阶段、守卫默认补偿、启动扫描与恢复 |
| `checkpoint-file/src/rewind/budget.rs`（新增） | 可用空间探测、预算判定、与现有 GC 联动 |
| `wf-checkpoint/src/rewind/apply.rs`（新增） | 前置校验、冲突判定、会话头序号前移、代码恢复、补偿 |
| `wf-api/src/agent/agent_message.rs` | 历史读取按会话头序号过滤，支持回退后继续追加 |

要点：

- 冲突默认拒绝；覆盖需显式参数。
- 先拍恢复树，再做任何写操作。
- 会话头序号前移与文件恢复必须同属一个事务方向：任一失败都整体补偿。
- 启动扫描遇到“已应用未提交”，能确认完全应用则补提交，否则回滚。

验收：

- 存在未提交改动时默认拒绝，返回可读冲突清单。
- 强制覆盖后可用恢复引用退回改动前。
- 目标树对象缺失时拒绝代码作用域，仅会话仍可用。
- 人为中断在已应用阶段，重启后工作区不出现半应用。

## 阶段四：门面与路由

| 改动落点 | 内容 |
|---|---|
| `wf-api/src/checkpoint/rewind.rs`（新增） | 状态、分页列举、预览、应用四个领域函数；错误映射 |
| `wf-api/src/checkpoint.rs` | 导出回溯域 |
| `wf-server/src/api/checkpoint/rewind.rs`（新增） | 四条路由与请求/响应模型 |
| `wf-server/src/router.rs` | 合并回溯路由 |
| `wf-server` 的接口快照与前端类型生成 | 同步 OpenAPI 描述 |

验收：

- 四个接口返回结构稳定，分页游标可往返。
- 过期请求被乐观并发校验拒绝，且不改工作区。
- 错误分类能映射到界面可执行动作。

## 阶段五：命令行

| 改动落点 | 内容 |
|---|---|
| `tui-core/src/keymap.rs` | 新增“打开回溯”动作 |
| `tui-core/src/command.rs` | 新增命令项：斜杠 `rewind`、别名 `rw`、leader 快捷键、命令面板可见 |
| `tui-components/src/footer.rs` | 新增回溯路由与面板状态；同步路由标识与绘制高度 |
| `tui-components/src/rewind_panel.rs`（新增） | 两阶段面板的纯数据与绘制 |
| `wf-tui/src/interactive/panels.rs` | 打开与关闭回溯面板、提交作用域 |
| `wf-tui/src/interactive/keys.rs` | 面板内按键、双击 Esc 武装与抑制 |
| `wf-tui/src/interactive.rs` | 新增回溯列表与结果的交互事件 |
| `wf-tui/src/interactive/turn.rs` | 结果回填本地历史、序号、输入框与转录提示 |

要点：

- 首行“当前状态”为伪条目，回车关闭且无副作用。
- 流式期间不武装手势；取消后清空武装。
- 面板打开与提交都走异步事件，按键循环不阻塞。

验收：

- 单击 Esc 行为不变；双击 Esc 在空闲态打开面板。
- 面板默认停在“当前状态”，回车无副作用。
- 应用成功后输入框回填提示词，历史与序号与后端一致。
- 不可用作用域置灰且有原因文案。

## 阶段六：网页

| 改动落点 | 内容 |
|---|---|
| `web-app/src/lib/services/rewind.ts`（新增） | 四个接口函数 |
| `web-app/src/lib/components/chat/MessageBubble.svelte` | 新增“回到此处”动作 |
| `web-app/src/lib/components/chat/RewindModal.svelte`（新增） | 两步模态与不可用原因 |
| `web-app/src/lib/chat/chat-session.svelte.ts` | 打开模态、提交、用返回会话重建状态、流式中禁用 |

验收：

- 网页与命令行对同一会话、同一点、同一作用域产生一致结果。
- 应用后消息列表、当前头标记、输入区状态同步更新。
- 失败只在模态内提示，不改工作区。

## 阶段七：预算与可观测

| 改动落点 | 内容 |
|---|---|
| `checkpoint-file/src/rewind/budget.rs` | 余量与预算判定的完整实现与测试 |
| `checkpoint-file/src/gc.rs` | 回收时保留未提交事务的恢复引用与在途点引用 |
| `wf-checkpoint`、`wf-api` | 回溯指标：点数、裁剪数、预览/应用次数、失败分类计数 |
| 审计查询 | 按会话列点与事务历史 |

验收：

- 可用空间不足时降级为仅会话并给出原因。
- 回收不会删除未提交事务的恢复引用。
- 指标能在现有度量通道里观察。

## 验证方式

- 全量编译检查：`cargo clippy --all-targets --all-features`。
- 子系统测试：`cargo test -p wf-checkpoint`、`cargo test -p wf-api`、`cargo test -p wf-server`、`cargo test -p wf-tui`。
- 文件检查点与引擎相关测试在改动 crate 内运行；状态链与文件链的既有回归必须全绿。
- 前端按仓库既有方式做类型检查与构建。
- 手工清单：Git 与非 Git 工作区、取消回合、失败回合、冲突覆盖、对象缺失、重启恢复、命令行与网页双端一致性。

## 风险与应对

| 风险 | 影响 | 应对 |
|---|---|---|
| 会话锚改造面广 | 消息读取、续跑、回放同时受影响 | 单独成阶段，先补测试再改实现；保留按运行读取 |
| 序号与去重不一致 | 会话回退后历史错位 | 序号取自引擎快照；回退只前移头序号，不逐条删 |
| 回合边界无检查点 | 代码回退不可用 | 边界补拍开关 + 仅会话降级 |
| 大仓库补拍慢 | 回合终结延迟 | 异步补拍 + 超时 + 点状态为准备中 |
| 回收顺序错误 | 账本可回滚但对象已丢 | 固定“先写后删”，并把顺序写入测试用例 |
| 崩溃半应用 | 工作区与会话不一致 | 事务行 + 恢复引用 + 启动扫描 + 守卫默认补偿 |
| 前端各自截断 | 双端行为分叉 | 会话由服务端物化，前端只替换不裁剪 |

## 上线与开关

- 回溯总开关默认开启，但代码作用域受预算与余量闸门独立约束；闸门不满足时只降级，不整体关闭。
- 若账本表迁移失败，回溯整体降级为未启用，检查点其余功能不受影响。
- 第一至第三阶段为纯后端，可先于界面合入；界面阶段通过开关隐藏入口，后端稳定后再默认展示。
