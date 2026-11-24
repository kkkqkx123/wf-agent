# 远端部署作业下一阶段治理设计

## 定位

本文是 `docs/plan/remote-deploy-job-hardening.md` 的后续任务设计。上一阶段已完成作业提交、进度、取消、停机收敛、持久化、CLI/Web 入口与恢复；本阶段只处理仍然影响正确性、审批语义和客户端体验的剩余问题。

本文是设计方案，不引入代码变更。实际实施时应按本文阶段顺序推进，并在每一阶段结束后同步更新本文的落地状态。

## 现状基线

当前实现已经具备：

- `POST /api/v1/system/backends/{name}/deploy` 提交后台作业并返回 job id。
- 按 backend 的原子 slot 准入，同一时刻只允许一个在飞作业。
- 作业列表、单作业查询、取消接口，以及 CLI/Web 对应入口。
- 作业事实持久化在 `backends.db`，管理端重启后遗留作业被收敛为 `interrupted`。
- 取消与停机收敛有终态守卫，强制终态不会被后台任务覆盖。
- 步骤进度带 `totalSteps`，`currentStep` 表示正在执行的步骤。
- 中断后的 outcome 带一次只读 probe，stdin 写入纳入命令超时。

仍存在的问题集中在四类：

1. **审批契约**：用户看到的预检 plan 与 apply 重算后的 plan 可能不同。
2. **停机边界**：请求通过 middleware guard 后、服务开始停机前，仍可能创建新作业。
3. **客户端语义**：15 分钟轮询上限把“仍在执行”表述成失败；Web 取消按钮本地伪造终态。
4. **治理完整性**：测试覆盖、保留策略、审计字段和指标仍未闭环。

## 阶段一：修复停机准入竞态

### 问题

`is_new_workload` 在 middleware 中拒绝停机期间的新提交，但请求通过 guard 之后，真正进入 `submit_deploy_job` 之前仍可能遇到服务开始停机。当前 manager 自身不知道服务正在停止，因此可能在该窗口内登记并启动新作业。

### 设计

- 在 `BackendManager` 中增加独立的接纳开关，初始为开启。
- 新增一个“开始停止接纳”的方法：
  - 关闭接纳开关。
  - 不取消已有作业。
  - 不关闭隧道。
- `submit_deploy_job` 在校验请求前先检查接纳开关；关闭时返回明确的 shutting-down 错误，由 handler 映射为 503。
- `ServerHandle::shutdown` 在设置全局 stopping 标记之后、HTTP drain 之前调用该方法。
- drain 完成后的 `cancel_deploy_jobs()` 保持不变，继续负责在飞作业收敛。
- `BackendManager::shutdown()` 直接被调用时也先关闭接纳开关，避免测试或其他调用方绕过。

### 验收

- 接纳关闭后提交返回 503。
- 关闭接纳前已提交的作业仍能被正常查询、取消和收敛。
- 关闭接纳不会提前取消作业，取消仍由 drain 后的流程负责。

## 阶段二：修正 Web 取消语义

### 问题

当前 Web 的取消按钮在收到 202 后直接把本地 `deployJob.status` 写成 `interrupted`。但服务端取消是协作式的：正在执行的步骤可能完成，最终甚至返回 `succeeded`。本地伪造终态会让页面与服务端事实不一致。

### 设计

- 取消请求成功后只设置 `cancelRequested = true`。
- 不修改 `status`，继续轮询直到服务端返回真实终态。
- 列表中的 live 行同样只反映 `cancelRequested`，不提前显示 `interrupted`。
- 终态展示只信任服务端返回的 `job.status` 与 `outcome.status`。
- 若取消请求返回 409，说明作业已终态，刷新列表并用服务端状态覆盖本地视图。

### 验收

- 取消后、服务端终态前，页面显示“已请求取消”但不是终态。
- 服务端返回 `succeeded` 时页面显示成功，不保留伪造的 interrupted。
- 服务端返回 `failed` 或 `interrupted` 时按真实结果显示。

## 阶段三：建立 plan/apply 审批闭环

### 问题

`plan` 是预检，`apply` 会重新计算 plan，因为远端状态和本地部署源都可能漂移。这在安全上是正确的，但用户看到的“批准内容”可能不是最终执行内容。现有 `DeployJobView.total_steps` 只能说明步骤数，不能证明命令一致。

### 设计

#### plan 摘要

- `DeployPlanView` 增加一个稳定 `plan_id`。
- `plan_id` 是 plan 内容的摘要，计算时必须排除时间戳等不稳定字段，例如 `planned_ms` 与 probe 时间。
- 摘要纳入：
  - action、scope、source。
  - install root、server port。
  - target platform、artifact 解析结果。
  - 待传文件路径、字节数和权限。
  - 最终步骤的命令列表。
- 同一请求、同一远端事实、同一部署源应得到相同 `plan_id`。

#### 提交契约

- `DeployRequest` 增加 `plan_id`。
- 写动作必须携带 `plan_id`；`probe` 不需要。
- `apply` 内重算 plan 后比较摘要：
  - 一致：继续执行。
  - 不一致：不执行任何远程步骤，落 `failed` 终态，错误明确要求重新 plan。
- `force` 不绕过 plan 漂移。`force` 仅继续用于版本、平台或下载条件的阻塞；审批失效不能靠 force 跳过。

#### 最终 plan 可见性

- `DeployJobView` 增加 `final_plan`。
- `final_plan` 保存 apply 重算后实际采用的 plan。
- Web 在作业详情中展示预检 `plan_id` 与 `final_plan` 的差异；CLI 在 JSON 输出中提供同一字段。
- 提交回执中的 `total_steps` 继续保留，UI 不再自行推测总数。

#### 三个入口

- Web：plan 后保存 `plan_id` 到当前表单状态，apply 时携带；作业完成后可查看 final plan。
- CLI：`remote deploy --apply` 内部先调用 plan，再提交，保持用户只输入一次命令的习惯。
- `--no-wait` 同样要求有效 `plan_id`，返回 job id 与摘要。

### 验收

- 预检与执行一致时正常执行。
- 修改部署源文件后，提交被拒绝且没有任何远程写步骤。
- 远端平台或安装状态变化导致步骤变化时，提交被拒绝。
- `force` 无法绕过 plan 漂移。
- job 详情中能看到 final plan。

## 阶段四：重定义客户端等待语义

### 问题

CLI 与 Web 都设置了 15 分钟轮询上限。现在已有 job id、列表和单作业查询，超时不应再被描述为部署失败；它只表示客户端停止等待，服务端作业可能仍在运行。

### 设计

#### 通用结果

- 客户端内部统一区分两类等待结果：
  - `terminal`：服务端已给出终态，携带 outcome。
  - `pending`：到达等待上限，服务端仍未终态，携带当前 job。
- `pending` 不是错误，也不伪装成失败。
- 两类结果都包含 job id、backend、action、当前步骤和完成步骤数。

#### CLI

- `--apply` 到达上限时输出结构化 pending 结果。
- 输出中包含后续查询命令提示。
- 保留 `--no-wait` 作为显式“只提交不等待”入口。
- 进程退出码不把 pending 表达为业务失败；具体退出码沿用 CLI 既有约定，必要时单独定义。

#### Web

- 到达轮询上限后停止 active polling，但保留 job id 和列表项。
- 页面显示“仍在执行”，不弹失败 toast。
- 刷新页面后从作业列表重新 attach。
- UI 不再依赖单一内存中的 `deployJob` 作为唯一状态来源。

### 验收

- 超过 15 分钟后，CLI 输出 pending 和 job id。
- Web 不显示失败，作业列表中仍可查询和取消。
- 页面刷新后能按列表项恢复观察。

## 阶段五：补齐测试覆盖

上一阶段文档中列出的验证项比实际自动化覆盖更宽。本阶段补齐可稳定自动化的部分，不为 SSH 真机行为构造复杂 mock。

### 必补用例

- 并发提交同一 backend：
  - 构造 enabled manager、memory store 与一个已存在 backend。
  - 并发提交多个作业。
  - 只有一个成功，其余返回冲突。
  - 最终 slot 只指向成功的 job。
- stdin 挂死：
  - 将 stdin 写入与命令等待抽出可注入的超时边界。
  - 用不读取数据的异步 pipe 模拟远端不消费输入。
  - 断言在配置超时内产生失败步骤并释放 slot。
- plan 摘要稳定性：
  - 证明时间戳不参与摘要。
  - 证明步骤命令、文件内容和目标平台参与摘要。
- plan 漂移：
  - 预检后改变输入，提交被拒且没有远程写步骤。
- Web 取消语义：
  - 确认取消成功后只更新 `cancelRequested`。
  - 确认终态仍由服务端响应决定。

### 文档同步

完成测试后，重新核对 `docs/plan/remote-deploy-job-hardening.md` 的验证结果描述，避免文档声称的覆盖范围与实际用例不一致。

## 阶段六：保留、审计与指标

本阶段是治理增强，可在前五阶段之后独立排期。

### 作业保留

- 新增配置：
  - `deploy_job_retention_days`，默认 30 天。
  - `deploy_job_max_per_backend`，默认 200。
- 清理只作用于终态作业；queued/running 永不清理。
- 启动时执行一次清理，后台按天级周期执行。
- 清理顺序：
  1. 删除超过保留天数的终态作业。
  2. 每个 backend 只保留最近 N 条，删除更早记录。

### 审计字段

- 若鉴权上下文能稳定提供调用主体，增加 `submitted_by`。
- 不能区分主体时写 `"api-key"`，不伪造用户名。
- `submitted_by` 随作业一起持久化，只用于审计，不参与权限判断。

### 指标

- 增加作业级计数：
  - submit。
  - succeeded。
  - failed。
  - interrupted。
  - cancel requested。
- 增加终态耗时统计。
- 不为每一步推送指标；步骤进度由 job 视图承担。

## 阶段七：多管理进程唯一性（条件触发）

仅在确实允许多个管理进程共享同一 `backends.db` 时实施。当前单管理进程模型下不启动。

### 设计方向

- 把按 backend 的 slot 从进程内 `DashMap` 提升到存储层表。
- `backend` 作为唯一键。
- 提交时在同一事务中：
  - 插入 slot。
  - 插入 job。
  - 任一失败则整体回滚。
- 终态时在同一事务中删除 slot。
- 启动恢复时清理指向非 live 作业的陈旧 slot。
- 存储层唯一性替代进程内 slot，或作为进程内 slot 之后的第二道防线；两者不能各自独立判断。

### 验收

- 两个管理进程同时提交同一 backend，只有一个成功。
- 管理进程崩溃后，遗留 slot 能在重启恢复中被清理。
- 单个管理进程内仍保持现有 409 语义。

## 明确不实现的内容

以下内容在本阶段确定不实现，除非产品前提发生变化并另立设计：

- **远端 `desiredState` 与自动收敛**：远端服务是显式动词管理，自动重建会在网络抖动时删除正在服务的目录。
- **重启后重放远端操作**：远端没有操作游标，重放可能重复 stop、install 或 uninstall。重启只收敛本地记录。
- **远端保存作业历史**：管理面是作业所有者，远端只保存安装事实；远端保存历史会形成双真相源。
- **中断后自动回滚**：回滚是新的长写操作，风险高于原始部署。中断后只通过 probe 暴露事实，由操作者决定下一步。
- **SSE 进度推送**：现有轮询、作业列表和重新 attach 已覆盖主要使用习惯，引入 SSE 会增加流生命周期与鉴权复杂度，本阶段不做。
- **强制中断后的长 probe**：强制中断只使用短超时 probe 呈现事实，不延长 shutdown 时间，也不作为成功判定。
- **多管理进程分布式锁**：当前部署模型要求单一管理进程；未引入 HA 前不实现阶段七。
- **远端进程开机自启与守护**：属于远端维护的另一类能力，不在作业治理范围内。

## 实施顺序

建议按以下顺序推进：

1. 阶段一：停机准入竞态。
2. 阶段二：Web 取消语义。
3. 阶段三：plan/apply 审批闭环。
4. 阶段四：客户端等待语义。
5. 阶段五：测试补全与文档核对。
6. 阶段六：保留、审计与指标。
7. 阶段七：仅在多管理进程需求成立时启动。

前五阶段属于同一批正确性任务；第六阶段可独立排期；第七阶段默认不启动。

## 验证方式

沿用 `docs/dev/devs同步与远程编译指南.md`：

- 源码同步到 devs。
- `cargo check --locked`。
- `cargo test` 覆盖 `wf-server` backends 与 `wf-cli-shared`。
- OpenAPI 快照刷新并回传。
- 前端代码生成与 `svelte-check`。
- 若新增配置，补充配置解析测试并确认示例配置无需迁移旧字段。

真机 SSH 场景不属于本阶段的自动化验收范围；若需要，应单独安排带两台主机的运维验证。

## 落地状态

- 阶段一：已完成。`BackendManager` 独立接纳开关在 HTTP stopping 标记之后、drain 之前关闭；提交在校验前检查开关，handler 映射 503；直接调用 manager `shutdown()` 也先关闭接纳。已有作业的查询、取消与 drain 后收敛路径不变。
- 阶段二：已完成。Web 取消只设置 `cancelRequested`，终态只取服务端 `job.status` / `outcome.status`；409 时刷新列表覆盖本地视图。
- 阶段三：已完成。`DeployPlanView.plan_id` 是排除时间戳的稳定摘要；写动作提交必须携带 `plan_id`，apply 重算后比对，漂移时落 `failed` 且不执行远程步骤，`force` 不绕过；`DeployJobView.final_plan` 在 Web 与 CLI JSON 中可见。
- 阶段四：已完成。CLI 等待返回 `terminal` / `pending` 结构并附后续查询命令，pending 不作为业务失败；Web 超时停止 active polling、保留列表项并在刷新后从列表重新 attach。
- 阶段五：已完成。新增并发准入、stdin 超时边界、plan 摘要稳定性、plan 漂移与 Web 取消语义用例；已核对上一阶段文档的验证结果描述。
- 阶段六：已完成。新增保留配置（30 天 / 每后端 200 条）、启动与日级清理、只清理终态；审计字段 `submitted_by` 固定为 `api-key`；新增作业提交、终态、取消计数与终态耗时指标。
- 阶段七：未启动。当前仍是单管理进程模型，共享 `backends.db` 的多进程前提不成立。

本轮自动验证：OpenAPI 快照与前端 schema 已刷新；`wf-server` backends 98 项、CLI 共享 173 项、指标 155 项、前端服务 6 项测试通过；`svelte-check` 0 error / 0 warning。
