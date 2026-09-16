# Execution ID 统一寻址与 checkpoint 子命令整改方案

## 一、背景与目标

CLI 的 `checkpoint create` 目前通过布尔标志 `--agent` 在工作流与 agent loop 两条创建路径间切换，且 `list`、`chain`、`gc`、`restore` 只覆盖工作流侧查询，agent loop 的检查点在这些子命令下无法正确操作。

更深层的问题是：execution 本应是 workflow 与 agent loop 的共同抽象（类型层已有 `ExecutionType::{Workflow, AgentLoop}` 与执行层级模型），但"一个 execution id 指向哪个域"的解析职责目前由各前端自行承担——CLI 用布尔标志、HTTP server 用分端点 URL，只有 `audit` 模块已经实现了 facade 内部解析的先例。

本方案的目标：

1. 在 wf-api 层建立统一的 execution id 解析入口，由 facade 内部消解 id 指向；
2. CLI checkpoint 子命令全部改走该解析入口，删除 `--agent` 布尔标志，同时补齐 agent loop 检查点在 list/chain/gc/restore 上的功能缺口；
3. HTTP server handler 内部复用同一解析逻辑，URL 分端点结构保持不变。

## 二、现状结论

- 类型层：`wf-types` 已定义 `ExecutionType::{Workflow, AgentLoop}` 与 `ChildExecutionReference`，概念上 agent loop 已是 execution 树的子节点类型。
- API 层：`audit.rs` 的 `audit_summary` 已实现"先尝试按 agent loop 解析、失败则落回工作流"的内部解析模式；`agent.rs` 的 `require_agent_loop` 提供按 id 加载 agent loop 的能力。
- checkpoint 侧：agent 与 workflow 两个 coordinator 各自独立，持久化时以 `entity_type` 区分（agent 侧为 `agent_loop`，工作流侧为 `checkpoint`），id 生成各自独立、无跨域唯一性约束。
- CLI 侧：`CheckpointSub::Create` 以布尔分发；其余子命令硬编码工作流查询；`FileCreate` 已拆为独立子命令，说明按实体拆分在该命令组中已有先例。

## 三、设计决策

### 决策一：寻址统一，状态机不统一

统一只发生在"id 到实体的解析与路由"这一层。workflow 与 agent loop 的 checkpoint coordinator、状态结构、恢复流程本质不同，保持各自独立；facade 解析出实体归属后，路由到对应 coordinator。

### 决策二：由内部解析，而非要求调用方带类型

调用方（CLI、HTTP handler）只传裸 execution id。解析顺序：先查活跃的 agent loop 注册表（内存态、成本低），未命中再查工作流 execution 存储。两者都命中时返回歧义错误，此时才允许调用方以显式覆盖参数指定域——将"默认隐性猜测"降级为"异常路径的显式覆盖"，语义健康。

### 决策三：CLI 按实体拆分子命令，与 server 端点风格对齐

即使寻址统一，CLI 面上仍将 agent loop 的创建/恢复拆为独立子命令（如 `create-agent`），理由：两者的创建参数与恢复语义不同，子命令结构能显式表达差异，且与 `FileCreate` 的既有拆分先例一致。而 `show`、`delete`、`list`、`chain`、`gc` 这类对 id 通用且语义同构的操作，改走统一解析，不拆分。

## 四、整改任务

### 任务一 新增 execution 解析入口

在 wf-api 的 entity 模块下新增 execution 引用解析：定义解析结果枚举（工作流分支、agent loop 分支）与解析函数。解析函数先查 agent loop 注册表，未命中查工作流 execution 存储，双命中返回歧义错误，均未命中返回统一的不存在错误。

将 `audit.rs` 中现有的私有解析逻辑重构为该入口的调用方，消除重复模式。

### 任务二 checkpoint 记录层接入统一解析

wf-api checkpoint 记录层的按实体操作（list、chain、gc、delete）在入口处先做 execution 解析，根据解析结果选择正确的 `entity_type` 过滤值，替代当前硬编码的工作流过滤值。`show` 按 checkpoint id 直接读取，属全局寻址，不改。

### 任务三 CLI checkpoint 子命令整改

- 删除 `Create` 的 `--agent` 布尔标志，新增独立的 `CreateAgent` 子命令（参数为 agent loop id 与可选名称）。
- 新增 `RestoreAgent` 子命令，走 agent checkpoint 的恢复路径；原 `Restore` 保持工作流语义并在文档字符串中说明。
- `List`、`Chain`、`Gc`、`Delete` 改走任务二的统一解析，删除内部硬编码的域假设。
- 更新 args 相关单元测试，覆盖新子命令与歧义覆盖参数的解析。

### 任务四 HTTP server handler 复用解析

agent 与 workflow 的 checkpoint 相关 handler 内部改调任务一的解析入口做 id 归属校验：当 id 的实际归属与路由的端点域不一致时返回明确的错误提示（引导用户改用正确端点），而不是按错误的域静默处理。URL 分端点结构不变，对外契约保持。

### 任务五 歧义与错误语义

统一三类错误的表达：域不匹配、id 歧义（双命中）、id 不存在。错误信息中携带实际解析到的域，便于用户使用显式覆盖参数或切换端点。CLI 侧为 `create` 保留一个可选的域覆盖参数（仅在歧义时需要），默认省略。

## 五、验证与收尾

- wf-api 层：为解析入口补充单元测试，覆盖仅 agent、仅工作流、双命中歧义、均未命中四种情形；audit 既有测试回归通过。
- CLI 层：args 解析测试更新；用 `cargo clippy --all-targets --all-features` 做全量编译检查。
- 文档同步：更新 docs/api 中 agent 域与 workflow 域文档里 checkpoint 命令的说明，标注 `--agent` 标志已移除。

## 六、明确不做的事项

- 不合并 workflow 与 agent 两个 checkpoint coordinator 的内部实现。
- 不改动 checkpoint 持久化的 `entity_type` 命名与存储布局。
- 不为 id 引入前缀式编码（如 `exec:`/`agent:`）——解析入口已消除该需求，前缀反而把内部约定泄漏到用户可见的 id 中。
