# 工作流 / Agent 逐步调试

本文件聚焦用户需求中的「每一步的输出、消息 diff、变量 diff、出错时给出实际值」，以及「节点/边路由：可选要求分支穷尽」。

## 一、逐步输出（step output）

### 1.1 工作流节点步

每一步对应一个 `NodeExecutionRecord`（`crates/engine/wf-workflow/src/state.rs:9-29`）。debugger 展示字段：

| 字段 | 含义 | 出错时 |
|------|------|--------|
| `node_id` / `node_name` / `node_type` | 节点身份 | — |
| `input` | 进入 handler 前的输入（payload-capped） | 打印实际 input |
| `result` | handler 产出 | 若为 `Err`，打印实际 `result` 与 `error` |
| `success` / `error` | 成败与原因 | **高亮实际 `error` 字符串** |
| `branch_id` | 所属 fork/join 分支 | 标注分支上下文 |
| `start_time` / `end_time` | 耗时 | 计算并显示耗时 |

### 1.2 Agent 迭代步

一次 agent 循环迭代 = 一次 LLM 调用 + 工具调用 + 结果，对应 `IterationResult { should_continue, content, completion_data, tool_call_count }`（`crates/engine/wf-agent/src/coordinator/iteration.rs:57-63`）。debugger 把每次迭代当作一个「步」，展示：本轮 `content`、工具调用次数、是否继续。

## 二、消息 diff

### 2.1 数据来源

- 工作流：`__msg_ctx__` 前缀下的命名消息上下文，`get_context`（活跃视图）/ `get_context_history`（全量）（`crates/engine/wf-workflow/src/message_context.rs:74/158`）。
- Agent：`AgentLoopEntity::conversation`（`crates/engine/wf-agent/src/entity.rs:24`）。

### 2.2 展示方式

- 以 `message.id` 对齐前后快照，标注 **新增 / 改写 / 删除**；
- 改写判定：同 id 但 `role`/`content` 变化（参考 `Message` 结构，`message_context.rs:247` 测试的构造方式）；
- 输出两种粒度：仅差异的紧凑视图 + 全量上下文的展开视图；
- 出错时：若某步本应追加消息却未追加（或追加了错误内容），diff 视图直接标红并附**实际 content 值**。

## 三、变量 diff

### 3.1 数据来源

`VariableStore = Arc<DashMap<String, Value>>`（`crates/engine/wf-workflow/src/variable.rs:5`）。每步前后各取一份快照（clone 为 `HashMap`）。

### 3.2 展示方式

- 按 key 集合做 **新增 / 删除 / 修改** 三类分类；
- 修改项用工作区既有 `similar = "3"`（`Cargo.toml`）生成 JSON 文本 diff，便于阅读嵌套结构；
- 保留 `${path}` 解析语义：变量值本身可能由 `VariableResolver::resolve` 插值而来（`variable.rs:544`），diff 展示**解析后**的值；
- 出错时：若断言期望变量为 `X` 而实际为 `Y`，并排显示期望值与实际值 `Y`（即「实际值」）。

## 四、节点 / 边路由：可选的分支穷尽

用户明确「可选要求分支穷尽」。设计上把它作为**可选模式**，默认关闭、按需开启。

### 4.1 枚举对象

路由决策点来自：

- `Route` 节点：`RouteHandler` 的 `conditions` 数组 + `default_target_node_id`（`crates/engine/wf-workflow/src/handler/route.rs:43-66`）；
- `Edge` 的 `EdgeType::Conditional` + `condition`（`crates/foundation/wf-types/src/workflow/edge.rs:8/21`）；
- `Fork`/`Join` 并行分支（`StaticNodeType::Fork`/`Join`，`node/static.rs:19-20`）；
- `LoopStart`/`LoopEnd` 迭代边界（`node/static.rs:31-32`）。

### 4.2 穷尽算法（静态综合回放）

对每个决策点，取当前变量快照 `vars: HashMap<String,Value>`，对每条候选分支：

1. 用 `wf_core::condition::ConditionEvaluator`（`route.rs:5`、`matcher.rs:191` 已使用）求值其 `expression` / `condition`；
2. 标记该分支**命中 / 不命中**；
3. 对 `Route`：额外判定 `default_target_node_id` 在当前快照下**是否可达**（即所有显式条件均不命中时才会走默认）；
4. 收集所有分支结论，生成「决策点 → 各分支判定」表。

### 4.3 穷尽模式产出

- **分支决策树**：每个决策点的可选分支及其命中情况；
- **死边检测**：存在「在任何可枚举变量组合下都不可能命中」的边时告警（粗粒度：先按当前快照判定，后续可扩展为符号化枚举）；
- **覆盖统计**：被至少一条执行路径覆盖的边比例，辅助确认「分支是否穷尽测试过」。

### 4.4 与真实回放的关系

- 真实回放（导入 trace）只展示**实际走过**的那条边，并在该步注明「路由选择 = <target>」；
- 穷尽模式（静态综合）展示**所有可能**的边，二者可对照，快速发现「某分支从未被覆盖」。

## 五、出错路径的统一处理

无论工作流步、agent 迭代、变量/消息断言，出错一律遵循：

1. 捕获实际值（`error` 字符串 / 实际 `result` / 实际变量值 / 实际消息内容）；
2. 在格式化输出中以醒目区块呈现「期望 vs 实际」；
3. 不掩盖差异——diff 视图默认显示完整上下文，便于定位根因。

下一份文档 `03-hook-trigger-debug.md` 展开 hook 与 trigger 的调试设计。
