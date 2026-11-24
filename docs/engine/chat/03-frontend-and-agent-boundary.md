# 前端控制器与 agent 边界

## 前端：`ChatSessionController`

> 源码：`apps/web-app/src/lib/chat/chat-session.svelte.ts`

页面 `/chat` 用它作为唯一控制器（`routes/chat/+page.svelte`），路由组件全部变薄、只从该实例读取状态。职责：

- **选中态在地址里**：`selectedId` 派生自 `page.url.searchParams.get('id')`，侧栏与页面共享选中。
- **发送管线**：`sendText` 按会话执行器类型分派——`runChat`（chat 执行链）、`runAgent`（agent loop）、`runWorkflow`（工作流运行），`streamCallbacks` 统一消化事件流。
- **交互应答**：`refreshInteractions` / `respondApproval` / `respondQuestions` / `cancelInteraction` 处理 agent 侧的审批与提问。
- **会话治理**：`resumeSession` 重连、`maybeAutoTitle` 自动标题、`maybeRecap` / `beginRecapTurn` 摘要续接。
- **围栏入口**：`askFenced` 发起受限问答（见下）。

## ChatSession 与 agent loop 的职责边界

系统里有三种"跟模型说话"的形态，外表相似、抽象层不同：

| 形态 | 抽象层 | `execution_type` | 工具 | 生命周期 | 落库 |
|------|--------|------------------|------|----------|------|
| `ChatSession`（原语） | 引擎原语 | 无（不落库） | 无 | 无 | 无 |
| chat 执行链 | 执行域 | `Chat` | 无 | 无事件/审批/迭代/checkpoint | message store + `agent_execution` |
| agent loop（含无工具档） | agent 运行 | `Agent` | 有/受限白名单 | 完整：事件、审批、迭代、checkpoint、层级 | agent loop 全链路 |

真正的重叠只有"都不调用工具"一格，但语义、生命周期、落库归口与晋升方向均不同——这是有意分层，不是冗余实现。

### agent loop 的两条"无工具"路径（不是 chat）

- **路径 A：显式空工具池**。判定谓词 `is_explicit_no_tools`（`wf-tools/src/tool_exposure.rs`）：`available` 为显式空列表且 `initial`、`discoverable` 都为空（`None` = 继承全部，不算）。走完整 agent 生命周期，但**没有产品入口**，仅测试构造，属内部/兼容档位。
- **路径 B：`TemporaryMode`**（`wf-types/src/agent_execution/temporary.rs`）。不改 executor（仍 `execution_type = agent`），只强制收紧工具暴露：

| 档位 | 工具暴露 | checkpoint | 晋升 | Web 入口 |
|------|----------|-----------|------|----------|
| `Restricted` | 临时只读白名单（草稿首轮） | 正常 | 同 execution id 去掉 flag 重跑即晋升 | `/fenced` |
| `Ephemeral` | 只读 + 交互，无网络/副作用（只读快问） | 完全关闭 | 不复用 id，另起新执行并带显式历史 | `/readonly` |

它跑在 `execution_type = agent` 上，与 chat 的 `ExecutionType::Chat` 是**两条不同的执行域**；其价值恰在于"仍是 agent run"——保留审计、事件、层级、可晋升语义。

### 一致性细节

- server 层严格解析 `TemporaryMode` wire 名，未知值直接拒绝（`wf-server/src/api/agent/loops/lifecycle.rs`）。
- 引擎侧**强制覆盖、不合并**临时模式的工具暴露，客户端无法扩宽自己的笼子。
- CLI 侧 `temporary_mode` 恒为 `None`，暂无 CLI 入口（`wf-cli-shared/src/turn.rs`）。
- 前端非 agent 执行类型会清空一次性围栏 scope。

## 选型准则

| 场景 | 用哪个 |
|------|--------|
| 纯文本多轮问答，不需要工具/审批/checkpoint/agent 事件 | chat 执行链（底层 `ChatSession`） |
| 要 agent 审计/层级，首轮先围栏只读、之后晋升 | agent loop + `TemporaryMode::Restricted`（`/fenced`） |
| 一次性只读快问，不要 checkpoint、不复用执行 id | agent loop + `TemporaryMode::Ephemeral`（`/readonly`） |
| 显式空工具池的 agent run | 路径 A（内部档位，无产品入口） |

## 历史背景

当初的争议——chat 为何双写、为何混入 `agent_execution` 表、无工具模式是否与 ChatSession 重复——的完整论证，以及 chat session / single shot / agent loop / 临时对话四种形态的统一说明，见 `docs/engine/execution-modes.md`；本目录是该结论的实现侧整理。
