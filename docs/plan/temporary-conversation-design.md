# 临时对话（Temporary Conversation）设计（正式实现版）

> 本版把临时对话收敛到 agent 循环内部：`AgentLoopConfig.temporary_mode` 是唯一的
> 模式开关，引擎在唯一的 entity 构建漏斗里强制执行，客户端只能请求、不能加宽。
> 外加一档无检查点的只读问答（ephemeral），覆盖短期临时问答。
> opencode 背景与术语对照见 `docs/ref/opencode-临时对话逻辑分析.md`。

## 1. 背景

opencode 用 draft session（草稿会话）表示"尚未创建服务端会话"的临时对话：
前端先生成 `draftID`，持久化输入框与模型选择，提交第一条消息时才创建真实
session 并晋升。wf-agent 的现状已有同样的懒创建骨架（run/stream 按体生成 ID、
`settleRun` 基线认领、chat 预置 anchor），工具侧有 `ToolRiskLevel` 分级与按轮
exposure 约束。缺的是引擎内部的一等模式：首轮受限此前靠各前端自行拼装请求体，
服务端不认、历史不记、晋升无语义。本设计补上这一块。

## 2. 设计目标

1. 临时模式是 agent 循环内部的一等概念：配置项携带、引擎强制、历史可查。
2. 受限档（restricted）：draft 的前几轮只能调只读检查、交互追问与网络检索；
   落库行就是普通 Agent 行，可晋升为完整 Agent。
3. 只读问答档（ephemeral）：短期临时问答，不提供任何检查点，只允许本地只读
   工具与交互追问；对话记录照常可回放，但检查点链恒为空。
4. 强制点在服务端：客户端请求模式，引擎覆盖 exposure，客户端名单永远加不宽。
5. 前端 draft 键（`draft:<uuid>`）保留，只管"提交前"状态；提交后一律由后端行锚定。
6. 体积可控：ephemeral 不写任何检查点与文件历史；restricted 复用既有检查点纪律。

## 3. 非目标

- 不新增执行器、执行类型、执行状态、API 资源：两档都是 `execution_type = agent`。
- 不把临时对话做成无限制执行；晋升目标只有完整 Agent（chat 是无工具原语，
  反方向；跨执行器是重跑，不是晋升）。
- 不兼容历史数据（项目无兼容义务）；不做桌面端/CLI 全量对齐，策略函数供复用。

## 4. 核心概念

### 4.1 临时模式是循环配置，不是前端约定

唯一开关是 `AgentLoopConfig.temporary_mode`（`restricted` | `ephemeral`，
缺省为完整运行）。否决"客户端拼装受限名单"的初版做法，理由：名单是请求，
不是强制；换一个客户端就能绕过；历史里看不出哪轮是受限跑的。现在强制发生在
`build_entity_with_forced_id`——全部入口（HTTP run/stream、resume、工作流派发、
call_agent 子代理、触发器）唯一收敛的 entity 构建点。有效配置（含强制结果）
写入 entity 并随记录持久化，因此：审计可查每轮的真实 exposure；崩溃恢复重建的
entity 自动延续同一限制；晋升就是同 ID 去掉标记重跑。

### 4.2 两档工具策略（risk 档位派生）

- restricted：允许 ReadOnly + Interaction + Network，拒绝 Write + Execute +
  System + Mcp。对应 draft 的受限前几轮：可查资料（含网络检索）、可追问、可记
  todo，不可写文件、不可跑命令、不可派生执行、不可调第三方 MCP。
- ephemeral：允许 ReadOnly + Interaction，拒绝其余全部（含 Network）。短期问答
  只答附带上下文，不付网络延迟与成本，不依赖连通性；是 restricted 的严格子集，
  逐级晋升只会加宽。
- 口径唯一实现点是 `wf-tools` 风险分级上的两组常量与四个查询函数；未知工具 ID
  两档一律拒绝；partition 不变量测试保证新工具自动归属，无漏网。
- 变量与 todo 不在工具名单里：变量是 REST 面，`update_todo_list` 本就属于
  ReadOnly 档。

### 4.3 检查点矩阵

- restricted：检查点行为与普通 Agent 轮次完全一致（轮次边界、hook、手动、晋升前）。
- ephemeral：不提供任何检查点。覆盖全部五条写入路径：无策略/interval 时本就无
  集成；有 interval/strategy 也强制关闭；生命周期完成/失败检查点同把柄关闭；
  hook 的 `create_checkpoint` 走同一把柄因而只发事件不落盘；文件历史会话不建
  （无快照可锚定的 diff 观察只是增重）；手动创建接口显式拒绝。
- 两档的消息、变量、执行记录照常持久化：transcript 可回放、审计可查；ephemeral
  的代价是崩溃后不可 resume（无快照可续），这正是"短期"的含义，文档与错误信息
  明确告知。

### 4.4 插件映射到现有扩展面，不新增

- 工具扩展：注册时声明 `risk_level` 即自动进入两档的允许/拒绝桶。
- 操作钩子：既有 `HookConfig` 与 handler 注册表；ephemeral 下钩子照发事件，
  只是不落检查点。
- UI 扩展：Composer 动作与晋升入口走现有槽位。

## 5. 后端设计

### 5.1 数据模型：零新增

落库行始终是 `AgentExecution`（`execution_type = agent`）；"临时"由该轮
`loop_config.temporary_mode` 表达。不新增定义标记（`agent_id` 仍走模板解析，
无 `@temporary` 模板），不新增状态（复用 `ExecutionStatus`），删除沿用现有链。

### 5.2 执行链路：漏斗强制

- HTTP 入口在请求体新增 `temporary_mode` 字符串字段，严格解析（未知值 400，
  不静默归为完整运行）。
- entity 构建时覆盖：pool 置为档位 allowlist；initial/discoverable 置空（允许的
  全部 upfront 可见，短问答不跳发现舞步）；activated 只保留档内；hidden 取
  客户端与档位拒绝的并集（hidden 优先级最高）；`general` 批量代理置否（内层调用
  同样受 exposure 约束，双保险）。
- 校验顺序不变：`gate_agent_config` 先验客户端体，强制后生成的一定是合法 exposure
  （allowlist 全是已注册预定义 ID）。
- ephemeral 额外关闭：检查点集成（本节 §4.3 五条路径）、文件历史会话；手动创建
  接口读 entity 有效配置后拒绝。

### 5.3 晋升：同 ID 去标记，无迁移

- restricted → 完整：同执行 ID 下一轮去掉标记、改用完整 exposure；建议先打一个
  晋升前命名检查点作为受限期与全功率期的分界。
- ephemeral → restricted → 完整：逐级加宽，每级都是同 ID 重跑；也可以直接跳到完整。
- 无需服务端迁移接口：消息、变量、todo、检查点本就锚定在该 ID 上。

### 5.4 项目历史

临时行自动出现在 `agent-loops/summaries` 与检查点链查询中。ephemeral 行的检查点
链恒为空，调用方按空链展示即可。"临时"徽标由前端本地维护（该 ID 是否经历过
受限轮且未晋升），不进后端过滤器。

## 6. 前端设计

### 6.1 多 draft 键 + 懒创建 + 认领（opencode 同构）

- 未发送草稿键用 `draft:<uuid>` 命名空间，按 ID 隔离存储，刷新不丢失。
- 提交首轮时：agent 走 `settleRun` 基线认领（draft 键→真实 ID 跳转），chat 走预置
  anchor；两种都是"提交才落库"，历史无空行。draft 首轮在 stream/run 请求体带
  `temporary_mode: 'restricted'`；短期问答入口带 `'ephemeral'`。
- 关闭未发送 draft 只清本地；认领后旧键随现有清理逻辑清除。

### 6.2 状态持久化

未发送阶段走 localStorage（按 draft 键）；落库后消息、变量、todo 按执行 ID 从
后端加载，composer 草稿叠加本地。刷新语义不变：服务端为准。

### 6.3 UI

- Composer 显示"临时对话/只读问答"标识；工具选择器按 risk 过滤隐藏拒绝档
  （后端仍强制）。
- 晋升入口：restricted 行可晋升为完整 Agent（晋升前检查点 + 同 ID 完整续轮）；
  ephemeral 行可升级为 restricted 或完整（同 ID 重跑）。
- 检查点面板中 ephemeral 行展示"无检查点"空态，而非报错。

## 7. API 设计：只增一个请求字段

- `POST /agent-loops/{id}/run`、`/stream` 请求体增 `temporary_mode`
 （`restricted` | `ephemeral`，缺省完整运行）；resume 请求体经由同一结构自动覆盖。
- 其余复用：summaries、消息、iterations、timeline、变量、todo、检查点列表/恢复
  全部按执行 ID 沿用现有端点；工具元数据（含 risk 分级）走现有工具列表面。

## 8. 迁移步骤

1. 模式与策略：`TemporaryMode` 类型 + 配置字段（已完成）、两档 risk 策略函数与
   partition 测试（已完成）。
2. 引擎强制：entity 漏斗覆盖（已完成）、ephemeral 检查点五路径关闭（已完成）、
   手动创建拒绝（已完成）。
3. 服务端字段：run/stream 请求体 `temporary_mode`（已完成）。
4. 前端透传：stream/run 服务函数字段（已完成）。
5. Composer 装配：draft 首轮默认 `restricted`、短期问答入口 `ephemeral`、
   工具展示过滤、临时徽标、晋升入口。
6. 历史与检查点面板的临时徽标与 ephemeral 空态（本地标记，不碰后端过滤）。
7. 补充测试：未知模式 400、强制覆盖单测（已随引擎落地）、首轮受限 e2e、
   ephemeral 零检查点断言、认领跳转、刷新恢复。

## 9. 风险与缓解

- 客户端伪造加宽：不可能。强制在 entity 构建漏斗，客户端名单只会被覆盖或收窄。
- 旧记录无标记：`temporary_mode` 缺省即完整运行，旧 blob 照常解析，语义不变。
- ephemeral 崩溃恢复：无快照可续，resume 会因查无检查点而失败；这是短期语义的
  代价，调用方应引导重问而非恢复。
- ephemeral 长期不晋升堆积：沿用现有归档/删除，不另设保留。
- 工具拒绝必须后端强制：hidden 最高优先级 + general 内层同约束 + 未知 ID 拒绝，
  前端仅展示。
- 体积：ephemeral 零检查点、零文件历史；restricted 沿用既有 interval 与清理策略。

## 10. 结论

临时对话是 agent 循环内部的配置模式，不是前端约定：`temporary_mode` 开关、
漏斗强制、历史可查。restricted 覆盖 draft 受限轮并可晋升；ephemeral 覆盖短期
只读问答，不提供任何检查点但保留可回放的 transcript。两者都不新增执行器、
状态与资源。
