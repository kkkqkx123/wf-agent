# 回顾注记（recap）与通用旁路请求（aside）实施计划：总览

两份分析文档（[recap 扩展][recap-analysis]、[aside 通用化][aside-analysis]）的对象是 `ref/atomcode`
快照，当前主仓尚无 recap 的任何实现，且只有压缩摘要这一种旁路形态。本计划回答三个问题：
现有设计哪些成立、正式实现落在哪里、分几个阶段做。

[recap-analysis]: ../../analysis/2026-10-26-recap-extension-design.md
[aside-analysis]: ../../analysis/2026-10-27-aside-request-and-resources-design.md

## 1. 背景与目标

recap 是回合正常结束后追加在回答下方的一行暗色注记（形如 `※ recap: …`），给刚回过神的人一句话交代
「做到哪了、下一步等谁」。当前主仓（`web-app`、`wf-tui`、`wf-mini`、chat 与 agent loop 执行链）都没有它。
目标是先把 recap 做成第一个正式的旁路请求实例（后端可调、前端可显、可手动触发），再在第二个真实用途
出现时把沉淀的形状抽成通用原语。三个硬约束贯穿所有阶段：只为屏幕（不写会话日志、不进快照、resume
不重放）、模型不读回（不进入下一次请求的 conversation）、旁路调用（无状态、无工具、有超时，
失败静默且不改变回合终态）。

## 2. 现有设计是否合理：结论

成立并原样继承：三条硬约束；「值不值得回顾」归视图判定；请求-响应而非广播（多端共享等确有需求再立项）；
失败一律等价于「这次没有 recap」；`ui.recap` 只管自动注记、手动触发不受其约束；ACP 不补；
resume 与非正常终态不生成；屏幕/事实/决策三去向不可混同，事实只走白名单且 `AssistantMessage` 永不在其中；
`prompt` 先判值不值得问（返回空则连请求都不发）；调度永不阻塞主流程；不新增第二压缩状态机；
todo 先做提案路径、真写回另立 ADR；提示词是资源资产、不进可打印配置。

不成立、需要修正的有三处。第一，传输层的文件路径与类型名（`CodingRuntimeHandle::recap`、
`SideCallProvider`、`LiveViewHub`、`POST /live/recap`、`HostCommand::Recap`）在主仓都不存在，
对照关系见第 3 章。第二，通用化顺序反转：快照里 recap 已稳定、原语可以先行；
主仓 recap 尚不存在，因此先做实例、后抽原语，提前抽只会得到没有第二个调用方的抽象。
第三，多资源格式不引入：`aside.toml + prompt.md` 双文件格式改为 `wf-resource` 既有形态
（提示词正文进模板/片段注册表，节奏与上限由 Rust 侧结构携带）。

## 3. 主仓落点对照

| 能力 | 落点 |
| --- | --- |
| 旁路调用原语 | `wf-execution-shared::single_shot::generate_text_once` + `LlmGateway`，立项时补超时与取消 |
| 上下文投影与脱敏 | 新模块，基于 `ConversationSession::view_messages` / 消息存储，规则照搬快照 |
| 清洗（240 字符、单行、`<none>` 即无） | 新模块同名函数与常量，单测照搬快照 |
| 服务端读取面 | `wf-api` 新增 + `wf-server` 新增路由，复用现有鉴权与 envelope |
| 前端触发与渲染 | `web-app` 与 `wf-tui` / `wf-mini` 各自实现，判定抽为可单测纯函数 |
| 显示开关 | `wf-config` 显示旋钮一侧，精确键名由探针固定 |
| 通用原语（后期） | `wf-execution-shared` 内 `aside` 模块 + 注册表，订阅 `EventBus` 已有事件 |

## 4. 阶段划分

| 阶段 | 内容 | 详见 | 可独立合入 |
| --- | --- | --- | --- |
| 0 | 立项探针：固定模型槽、超时/取消、投影数据源、配置键、路由形状 | [01](01-recap-foundation.md) | 否（只产出结论） |
| 1 | recap 核心：投影、旁路调用、清洗、`wf-api` 读取面、单测 | [01](01-recap-foundation.md) | 是 |
| 2 | 服务端路由 + `web-app` 请求、判定、渲染、手动命令 | [02](02-recap-transport.md) | 是 |
| 3 | `wf-tui` / `wf-mini` 判定、渲染、手动命令，显示开关只读 | [02](02-recap-transport.md) | 是 |
| 4 | 通用化：第二个用途立项时抽 `aside` 原语，迁移 recap | [03](03-aside-generalization.md) | 否（以用途立项为触发条件） |

阶段 0 是强制前置：它只回答问题、不写功能代码，避免在模型槽归属、取消语义、配置键三个易返工点上赌错。
阶段 1～3 每个都可独立合入、独立跑门。阶段 4 不是排期，是触发式工作：没有第二个真实用途就不做。

## 5. 非目标（所有阶段）

resume 或打开历史会话时不补 recap；失败、取消、超限终态不生成；不改已调过的产品数值
（240 字符、25 秒量级超时、脱敏规则）；服务端不广播 recap 事件；不为 recap 新增 kernel 命令
与第二压缩状态机；提示词不进 `config.toml` 类可打印配置。

## 6. 门

每阶段独立跑：`cargo clippy --all-targets --all-features`、`cargo fmt --all -- --check`、
相关 crate 的 nextest（`wf-execution-shared`、`wf-api`、`wf-server`、`wf-resource` 等）与前端类型检查/构建。
