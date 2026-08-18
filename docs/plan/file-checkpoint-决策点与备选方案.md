# file-checkpoint 分层决策点与备选方案

> 状态：决策点分析（各决策点已定，部分具体设计待细化）
> 范围：延续 `docs/plan/file-checkpoint-layertwine-整合整改方案.md` 路径 B，对「修改来源」的识别、分层归属、审批语义给出决策点与备选方案
> 目的：各决策点的**多个备选方案**与利弊分析，以及已确定的决策结论（见文末「已确定的设计决策清单与待考虑问题清单」）

## 〇、问题域总览

路径 B 以 layertwine 的六层状态机（`manual_edit → agent_edit → approval → integrated → unified → staged`）为权威模型。但 workflow 实际的"文件修改"来源比 layertwine 假设的更复杂，需要先回答：**一次文件修改属于哪一层，由谁来触发进入哪一层**。

修改来源可归纳为四类：

| 来源 | 触发路径 | 当前落点 | 是否需要专门处理 |
|------|----------|----------|------------------|
| A. LLM 节点调用文件编辑工具 | `LlmHandler::execute_tool_call` → `ToolRegistry::execute_tool("write_file"/"edit_file"/"apply_patch")`（`wf-workflow/src/handler/llm.rs:500`） | 直接写磁盘（`wf-tools/src/filesystem.rs`） | **待定（见决策点 1）** |
| B. Agent 循环内 LLM 调用文件工具 | `wf-agent/src/coordinator/tool.rs` 的工具执行链路 | 直接写磁盘，经 `ToolApprovalHandler` | **待定（见决策点 1/4）** |
| C. 脚本节点 / 脚本执行工具 | `ScriptHandler` → `SandboxRuntime`（`wf-sandbox`，VFS overlay `delta` 内存映射） | 沙箱内，写进 `OverlayVFS.delta`，需回写 | **待定（见决策点 2）** |
| D. 非 LLM/脚本的人工/宿主修改 | 直接文件系统写、IDE、外部进程 | 无追踪 | **manual 层（见决策点 3）** |
| E. 嵌套子执行实例 | `parent_execution_id` / `root_execution_id`（`wf-types/src/execution/hierarchy.rs`） | 无归属隔离 | **见决策点 5** |

---

## 决策点 1：LLM 节点调用编辑工具产生的修改，是否单独处理

### 背景

LLM 节点（`StaticNodeType::Llm`）通过 `execute_tool_call` 把 LLM 返回的 `LlmToolCall` 交给 `ToolRegistry` 执行，文件工具（`write_file`/`edit_file`/`apply_patch`/`apply_diff`）直接改工作区。当前没有任何 hook 记录这些修改到 checkpoint / layertwine。

### 备选方案

**方案 1-A：不单独区分，统一按"agent 修改"进入 `agent_edit` 层（推荐基线）**

- 把 LLM 节点与 Agent 循环统一映射为 `SourceType::Agent(agent_id)`，`agent_id` 取该节点的执行上下文（execution id + node id）。
- 优点：模型简单，LLM 与 Agent 语义天然一致（都是"模型驱动的编辑"）；改动小。
- 缺点：无法区分"哪个 LLM 节点"改的（若同一 execution 内有多个 LLM 节点），溯源粒度只到 execution。

**方案 1-B：以 node_id 为 agent 维度细分**

- `agent_id = execution_id + ":" + node_id`，每个 LLM 节点一个独立 `AgentInstanceId` 分区。
- 优点：溯源精确到节点；契合"多 agent 编排"——每个 LLM 节点可视作一个"虚拟 agent"。
- 缺点：分区数量膨胀；一个节点多次迭代会产生多个分区历史，需明确是否合并。

**方案 1-C：LLM 节点的编辑也先入 `manual` 层再走 `move_*`**

- 优点：与人工修改统一入口，审批语义一致。
- 缺点：把模型修改与人工修改混为一谈，语义上不准确（manual 语义是"非模型"）；违反 layertwine 分层本意。

### 关键子问题

- `agent_id` 的粒度：execution 级 vs node 级 vs tool-call 级（见决策点 5 汇总）。
- LLM 节点 vs Agent 循环（`AgentLoop` 节点）是否同一 `AgentInstanceId` 空间，还是各自独立前缀。

### 决策（已定）

- **agent 节点（agent 作为主执行实例）**：每个 agent 实例单独一个分区（`AgentInstanceId = agent 实例 id`）。
- **workflow 中其他节点**（LLM 节点、脚本节点等）：共用一个 workflow 级 `agent_edit` 分区（`AgentInstanceId = workflow execution id`）。
- **subgraph**：单独提供一个分区。
- **triggered subworkflow**：本身就是独立工作流，自然拥有独立分区（沿用 workflow 级规则）。
- **不做 node 级隔离**：对 agent、subgraph 特殊处理即可，无需每个节点一个分区。

---

## 决策点 2：脚本执行工具 / 脚本节点产生的文件变更如何归类

### 背景

脚本节点（`ScriptHandler`）通过 `SandboxRuntime` 执行，文件写入走 `OverlayVFS`（`delta: HashMap<PathBuf, Vec<u8>>` 内存 overlay，`wf-sandbox/src/vfs/overlay.rs`），策略由 `PathPolicy.allowed_write` 控制。脚本自身可以直接写文件（shell 脚本 `>`、Python `open()`、JS `fs.write`），这些写入**不经过** filesystem 工具，而是沙箱 VFS。此外还有"脚本执行工具"（脚本作为工具被 LLM 调用）这一变体。

### 备选方案

**方案 2-A：脚本变更归入 `agent_edit`，以"触发脚本的 agent"为归属（推荐基线）**

- 脚本的修改视作"执行它的那个 agent/LLM 节点的修改"，复用 `SourceType::Agent`。
- 需要把脚本的 VFS overlay 变更在脚本执行结束后**回写并采集 diff**，然后走 `apply_agent_edit`。
- 优点：语义统一（脚本是模型的工具延伸）；不引入新层。
- 缺点：脚本可能是确定性批处理（非模型意图），归属到 agent 略勉强；需处理 VFS 回写与 diff 采集。

**方案 2-B：新增 `SourceType::Script(script_name)` 或独立分区类型**

- 在 layertwine `SourceType` 扩展 `Script(String)`（或在 `PartitionType` 加 `Script`），脚本变更独立溯源。
- 优点：溯源能区分"模型直接编辑"与"脚本批量变更"；便于审计脚本副作用。
- 缺点：需扩展 layertwine 核心类型（`SourceType`/`PartitionType` 是 closed enum），侵入 layertwine；破坏其"六层"假设。

**方案 2-C：脚本变更不进入 layertwine 分层，仅作为 checkpoint 的"文件快照"记录**

- 脚本执行后对工作区做一次整树扫描快照（复用原 `WorkspaceScanner`），存为 `FileCheckpoint`（内容级），不参与三方合并。
- 优点：改动最小，不破坏 layertwine 模型；脚本副作用天然不可合并（脚本产出无 diff 语义）。
- 缺点：脚本变更无法参与多 agent 三方合并；与路径 B "权威模型为 layertwine" 有张力。

### 关键子问题

- VFS overlay 的 `delta` 是否需要落地到真实磁盘（当前 overlay 是内存态），以及落地时机（脚本结束 vs 显式 commit）。
- 脚本的"允许写"路径（`PathPolicy.allowed_write`）与 file-checkpoint 追踪范围如何对齐。

### 决策（已定）

- **采用方案 2-A**：脚本变更归入 `agent_edit` 层，以"触发脚本的 agent"为归属，脚本执行结束后回写 VFS overlay 变更并采集 diff，走 `apply_agent_edit`。
- 脚本 VFS 回写与 diff 采集的具体设计后续细化。

---

## 决策点 3：manual 层 —— 非 LLM/脚本修改（含人工修改）的处理

### 背景

layertwine 的 `ManualEdit` / `PartitionType::Manual` + `SourceType::Manual` 就是为"非模型编辑"设计的（`layered/manual.rs`）。工作流中的人工修改、外部进程写入、IDE 直改等都属于此类。你已明确：**manual 层用于追踪非 LLM 调用文件编辑工具执行的修改，包括人工修改，需要专门处理**。

### 备选方案

**方案 3-A：文件 watcher 捕获 + 归入 `manual` 层（推荐基线）**

- 复用现有 `FileWatcher`（`wf-checkpoint/src/watcher.rs`，已有 `FileChangeKind::Add/Change/Unlink`）监听工作区，把非模型来源的变更采集为 `SourceType::Manual` 的 `Delta`，写入 `manual` 分区。
- 优点：layertwine 原生语义；能自动捕获"模型/脚本之外"的一切变更。
- 缺点：watcher 无法区分"变更是谁发的"——LLM/脚本路径若也走 watcher 会误判；需要**来源排除机制**（模型/脚本在写入时打标记，watcher 忽略带标记的路径）。

**方案 3-B：显式 API 记录（人工提交前主动 `apply_manual_edit`）**

- 人工修改由宿主/IDE/CLI 在变更后显式调用 `apply_manual_edit`（layertwine `manual.rs` 已有类似入口）。
- 优点：来源清晰、无 watcher 竞态；审核友好。
- 缺点：依赖宿主配合，非受控修改（直接 `vim`）仍漏捕。

**方案 3-C：混合 —— watcher 兜底 + 显式 API 优先**

- 模型/脚本路径显式走 `agent`/`script` 层并标记；其余 watcher 捕获统一归 `manual`。
- 优点：兼顾自动捕获与精确归属。
- 缺点：需要 watcher 与显式写入的互斥协调，复杂度最高。

### 关键子问题

- watcher 的"来源标记"机制：写路径如何标记"这是模型/脚本的写入"，避免被 watcher 重复捕获。
- manual 变更与 checkpoint 触发（`CheckpointTiming`）如何联动：是否人工修改也触发 checkpoint。

### 决策（已定）

- **采用方案 3-C（混合）**：模型/脚本路径显式走 `agent` 层并标记；其余 watcher 捕获统一归 `manual`。
- 具体设计（watcher 与显式写入的互斥协调、来源标记机制）后续再讨论。

---

## 决策点 4：approval 层的保留与形态

### 背景

你已明确：**approval 需要保留**，既要支持"工作流中自动批准 / LLM 审核"，也要支持"结束后人工审核"。当前已有两套审批基建：

- `wf-agent/src/coordinator/tool.rs` 的 `ToolApprovalHandler` + `ToolApprovalCoordinator`（工具级审批，执行前拦截）。
- `wf-agent/src/approval.rs` 的 `ToolApprovalRequest/Result`。

layertwine 的 `Approval` 分区（`PartitionType::Approval(AgentInstanceId)`）+ `move_agent_to_approval` + `merge_agent_to_feature` + `reject_approval`（`layered/approval.rs`、`layered/agent.rs`）提供"编辑 → 审批 → 合并"的分层流转。

### 备选方案

**方案 4-A：双层审批 —— 工具级（执行前）+ 分层级（合并前）（推荐基线）**

- 保留现有工具级审批（`ToolApprovalHandler`，自动/LLM 审核，拦截文件编辑工具调用）。
- 新增 layertwine 分层审批：`agent_edit` → `move_agent_to_approval` → 待审批 → `merge_agent_to_feature` / `reject_approval`。
- 自动批准：`approval` 配置为 auto 时，`move_agent_to_approval` 后立即 `merge`。
- LLM 审核：审批动作由某个 LLM 节点执行（调用审批工具）。
- 人工审核：审批挂起，`approval` 分区保留 `history.len() > 1` 状态，等待宿主/CLI 结束（或结束后）人工调用 approve/reject。
- 优点：复用两套成熟基建，语义清晰；天然支持"结束后人工审核"（approval 分区状态持久化）。
- 缺点：两层审批可能重复拦截（同一编辑被拦两次），需定义审批粒度分工。

**方案 4-B：仅分层审批，废弃工具级**

- 所有审批统一到 layertwine approval 层。
- 优点：单一审批模型。
- 缺点：丢失"执行前拦截"（工具级审批能阻止副作用发生，分层审批是事后合并），对高风险操作保护更弱；迁移成本大。

**方案 4-C：审批作为可选策略插件**

- 审批逻辑抽象为 `ApprovalPolicy` trait（auto / llm / manual / none），在 coordinator 注入。
- 优点：灵活，满足"自动批准、LLM 审核、人工审核、无审批"四种模式的统一配置。
- 缺点：新增抽象层，需定义与 `ToolApprovalHandler` 的关系。

### 关键子问题

- 审批粒度：文件级 / 工具调用级 / agent 分区级 / checkpoint 级。
- "结束后人工审核"：approval 分区状态如何跨执行持久化（执行结束不代表审批结束）。
- 审批通过后的合并冲突如何处理（`merge_agent_to_feature` 返回 `MergeConflict`）。

### 决策（已定）

- **暂时停下**：采用方案A。工具审批与文件编辑操作的审批是不同层次的内容。

---

## 决策点 5：嵌套子执行实例的归属与隔离

### 背景

嵌套子执行（sub-workflow / child execution / agent 子循环）通过 `parent_execution_id` / `root_execution_id` 表达层级（`wf-types/src/execution/hierarchy.rs`、`wf-agent/src/trigger.rs`、`wf-agent/src/registry.rs`）。你已明确：**需要区分不同操作者作出的修改**，子执行实例的操作者身份必须可区分。

### 备选方案

**方案 5-A：每级执行实例一个独立 `AgentInstanceId`（推荐基线）**

- `AgentInstanceId = execution_id`（或 `root_execution_id + execution_id` 复合），每级子执行在 layertwine 中拥有独立的 `agent` 分区。
- 父子关系用 layertwine `Branch`（`checkpoint/branch.rs`）表达：子执行从父的 head 拉分支，合并回父时产生多父 `Checkpoint`（merge commit）。
- 优点：最贴合 layertwine 的 `AgentInstanceId` + `Branch` + 多父 `Checkpoint` 模型；隔离天然。
- 缺点：`execution_id` 数量大，分区/Branch 管理需设计生命周期（执行结束是否保留分支）。

**方案 5-B：操作者身份复合 —— `AgentInstanceId = execution_id + agent_instance_id`**

- 在子执行内，进一步区分是"哪个 agent 实例"（同一个子执行可能跑多个 agent）。
- 优点：溯源粒度最细。
- 缺点：需要 agent 层提供 `agent_instance_id`；`AgentInstanceId` 字符串膨胀；与方案 1-B（node 级）叠加会过于细碎。

**方案 5-C：不引入 Branch，仅靠 `AgentInstanceId` 前缀 + `Checkpoint` 多父**

- 用 `AgentInstanceId` 的命名前缀（如 `root/{root}/child/{child}/...`）表达层级，不用 layertwine `Branch` 实体。
- 优点：少一套 Branch 生命周期管理。
- 缺点：放弃 layertwine 现成的 `Branch`/`switch_branch` 能力，溯源需自行解析前缀。

### 关键子问题

- `AgentInstanceId` 的最终编码规则（execution 单层 vs 复合 vs 层级前缀）——这是方案 1/5 的公共决策。
- 子执行结束后，其分区/Branch 是保留（溯源）还是归档（GC）。

### 决策（已定）

- **采用方案 5-C（命名前缀）**：用 `AgentInstanceId` 的命名前缀表达层级（如 `root/{root}/child/{child}/...`），不引入 layertwine `Branch` 实体表达嵌套关系。
- **fork-join 场景例外**：并行分支（fork/join）使用 layertwine `Branch` 表达（分支合并产生多父 `Checkpoint`）。分支的生命周期管理后续设计。
- 命名前缀的具体编码格式待定（见文末问题清单）。

---

## 汇总：交叉决策矩阵

| 修改来源 | 归属层（已定/候选） | 溯源 key（已定/候选） | 是否合并 | 是否审批 |
|----------|----------------|------------------|----------|----------|
| LLM 节点文件工具 | workflow 级 agent_edit（共用分区） | workflow execution id | 是 | 待定（决策点 4 暂停） |
| Agent 循环文件工具 | agent 专属分区 | agent 实例 id | 是 | 待定（决策点 4 暂停） |
| subgraph | 专属分区 | subgraph id | 是 | 待定（决策点 4 暂停） |
| 脚本节点/脚本工具 | agent_edit（2-A） | 触发脚本的 agent | 是（2-A） | 待定（决策点 4 暂停） |
| 人工/宿主修改 | manual（3-C） | 无（SourceType::Manual） | 需 move 到上层 | 待定（决策点 4 暂停） |
| 嵌套子执行 | 命名前缀分区（5-C） | 前缀编码 | 待定（合并下一阶段） | 待定（决策点 4 暂停） |
| fork-join 并行分支 | Branch（5-C 例外） | 分支名 | 多父 merge commit（下一阶段） | 待定（决策点 4 暂停） |

## 已确定的决策组合

- 决策点 1 → 按执行实例类型分区：agent 主执行实例独立分区；workflow 其他节点共用 workflow 级 `agent_edit` 分区；subgraph 独立分区；triggered subworkflow 走独立工作流。
- 决策点 2 → 2-A（脚本归 agent_edit，回写 VFS 后采集 diff）。
- 决策点 3 → 3-C（watcher 兜底 + 显式 API，来源标记互斥；具体设计后续）。
- 决策点 4 → **暂停**，保留现有工具级审批基建与 layertwine 分层审批候选。
- 决策点 5 → 5-C（命名前缀）；fork-join 场景使用 Branch。
- 合并设计 → **暂不考虑**，作为下一阶段问题。

---

## 附：已确定的设计决策清单与待考虑问题清单

> 本清单为历次评审的结论汇总，作为后续实施阶段的输入。按决策时间倒序补充。

### 一、已确定的设计决策

| # | 决策点 | 结论 | 备注 |
|---|--------|------|------|
| D1 | 决策点 1：LLM 节点编辑归属 | **按执行实例类型分区**：agent 节点（agent 作为主执行实例）单独分区；workflow 中其他节点（LLM/脚本等）共用一个 workflow 级 `agent_edit` 分区；subgraph 单独分区；triggered subworkflow 走独立工作流。不做 node 级隔离。 | 对 agent、subgraph 特殊处理即可 |
| D2 | 决策点 2：脚本变更归类 | **方案 2-A**：脚本变更归入 `agent_edit`，以触发脚本的 agent 为归属，执行结束后回写 VFS overlay 并采集 diff 走 `apply_agent_edit`。 | 回写与采集细节后续细化 |
| D3 | 决策点 3：manual 层 | **方案 3-C（混合）**：模型/脚本路径显式走 `agent` 层并标记；其余 watcher 捕获归 `manual`。 | 具体设计（来源标记互斥）后续再讨论 |
| D4 | 决策点 4：approval 层 | **暂时停下**：保留现有工具级审批基建（`ToolApprovalHandler`）与 layertwine 分层审批候选（`Approval` 分区 / `move_agent_to_approval` / `merge_agent_to_feature` / `reject_approval`）。 | 后续阶段恢复，再定形态（自动 / LLM / 结束后人工审核） |
| D5 | 决策点 5：子执行隔离 | **方案 5-C（命名前缀）**：`AgentInstanceId` 命名前缀表达层级；**fork-join 场景使用 Branch**。 | 前缀编码格式待定 |
| D6 | 合并设计 | **暂不考虑**，作为下一阶段问题。 | fork-join 的 Branch 合并、三方合并接入均属此范畴 |

### 二、待考虑问题清单

1. **`AgentInstanceId` 命名前缀的最终编码格式**（层级前缀 `root/{root}/child/{child}/...` 的具体分隔符与长度限制；workflow 级分区 / agent 分区 / subgraph 分区的前缀命名规则）。
2. **脚本 VFS overlay 回写与 diff 采集**：回写时机（脚本结束 vs 显式 commit）、overlay `delta` 落地路径、与 `PathPolicy.allowed_write` 的对齐。
3. **manual 层具体设计**：watcher 与显式写入的互斥协调、来源标记机制、manual 变更是否触发 checkpoint（`CheckpointTiming` 联动）。
4. **approval 层的恢复与形态**（决策点 4 暂停项的后续）：自动批准 / LLM 审核 / 结束后人工审核的具体实现、审批粒度、跨执行持久化。
5. **合并设计（下一阶段）**：多 agent / 多分支三方合并接入（`merge_agent_to_feature` / `merge_features_to_staged`）、fork-join 的 Branch 生命周期与合并、冲突处理策略（中断 / 冲突标记 / 走审批）。
6. **子执行分区/Branch 生命周期**：执行结束后分区与分支是保留（溯源）还是归档（GC）。
7. **`FileCheckpointStorageMetadata`（wf-types 指针层）去留**：删除还是保留为 layertwine 投影。
8. **溯源查询 API 形态**：按操作者/脚本/子执行的变更查询与展示。
