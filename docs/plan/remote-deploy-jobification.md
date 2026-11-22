# 远端部署作业化

> 基准提交：`528378c8`（`main`）。本文所有行号均针对该提交，代码落地时须重新核验。
> 本文遵循 `AGENTS.md`：方案以自然语言为主，不内嵌完整代码；代码与注释一律英文。

## 0. 结论

把同步部署请求改造为**可查询、可取消、停机可收敛的后台作业**。目标形态：提交即返 job id → 后台按步骤执行并逐步落盘进度 → 查询接口读状态 → 停机时把在飞作业收敛为 `interrupted` 并落终态。

明确不做（延续既有拍板）：远端仍不维护 `desiredState`；probe 仍是独立只读验证面；远端命令集仍是固定动词（`deploy.rs:98-111`），无自由 shell；写权限模型不变。作业化只改"执行与生命周期"，不改"部署什么、怎么部署"。

与保活阶段对执行建立的"终态收敛"哲学对齐：执行落 `Cancelled`、部署落 `interrupted`，都不落 `Failed`。

## 1. 本轮决策与代码事实

### 1.1 事实核验结果

输入草案引用的行号已逐条核验，**绝大多数精确吻合**。以下为基准（文件均相对仓库根）：

| 事实 | 核验位置 | 结论 |
|---|---|---|
| 四个部署端点 handler | `crates/app/wf-server/src/api/system/backends.rs:594/616/648/680` | 属实 |
| 路由注册（probe/plan/deploy GET+POST） | 同上 `:57-70` | 属实 |
| is_enabled 503 守卫 | 同上 `:601/624/656/687` | 属实 |
| POST /deploy 同步调 apply | 同上 `:659` → `manager.rs:1561-1582` | 属实 |
| 部署缓存两个 DashMap + "memory-only" 注释 | `manager.rs:144-147` | 属实 |
| deploy_apply 成功后写两表 | `manager.rs:1576-1580` | 属实 |
| backends.shutdown 只 cancel token + 停隧道 | `manager.rs:749-753` | 属实 |
| apply 是单次长 async，plan 内重算 | `deploy.rs:1837-1848` | 属实 |
| push_step! 同步顺序、失败即 return、无取消检查 | `deploy.rs:1885-1904` | 属实 |
| DeployChannel::run 单步 kill_on_drop + timeout | `deploy.rs:567-617`（`:587`/`:598`） | 属实 |
| run_step 产出 DeployStepResultView | `deploy.rs:1809-1830` | 属实 |
| DeployOutcomeView 仅有 finished_ms，无 job_id/status | `deploy.rs:433-447` | 属实 |
| DeployRequest 三入口共用 | `deploy.rs:215-232` | 属实 |
| is_new_workload 不含 deploy POST | `middleware.rs:406-439` | 属实 |
| shutdown 顺序：drain→reaper→PTY→backends | `server.rs:87-138`（abort `:108`） | 属实 |
| CLI 同步等 apply_server_deploy | `crates/app/cli/wf-cli-shared/src/cmd/remote.rs:321-326` | 属实 |
| Web 一次 POST 等 outcome | `apps/web-app/src/lib/services/backends.ts:266-277` | 属实 |
| DetachedRunRegistry 常驻 reaper + 终态收敛范式 | `detached.rs:92-165` | 属实 |

### 1.2 草案中两处需修正的事实（重要）

1. **执行终态 `Cancelled` 的位置**：草案写作 `entity.rs:786`。实际该语义在 `crates/engine/wf-agent/src/entity.rs:782-792`（"settle them as `Cancelled` (not `Running`/`Failed`)"），`crates/app/wf-api/src/entity.rs` 无此内容。本文引用以 engine 侧为准。
2. **id 生成器**：草案 diff 写 `wf_types::new_id()`，仓库中不存在。实际统一入口是 `wf_common::generate_id()`（`crates/foundation/wf-common/src/id.rs:3`）。作业 id 采用该生成器并加 `deploy-` 前缀区分（见 O7）。

### 1.3 已拍板决策（结合 AGENTS.md "No-backward-compatible"）

`AGENTS.md:5-6` 明确：项目处于开发阶段、无需专门考虑向后兼容、重在合理架构。据此对草案开放点逐项拍板：

| 点 | 决策 | 依据 |
|---|---|---|
| O1 旧同步 POST /deploy | **(c) 直接改造为作业化提交**，不保留同步双轨、不加 `?wait=true` | No-backward-compatible；避免两套 apply 逻辑漂移 |
| O2 状态持久化 | **(a) 纯内存**，与 `deploy_outcomes` 一致 | 远端才是 durable truth；首版不做本地 store |
| O3 同 backend 并发 | **(a) 409 拒绝** | 部署是低频运维操作，排队会掩盖误操作 |
| O4 提交时校验 | **(a) 提交端轻量校验 + apply 内重算 plan** | 保留 `deploy.rs:1834-1848` 的防漂移语义 |
| O5 进度推送 | **(a) 轮询 GET jobs/{id}** | 首版最简；SSE 依赖既有 stream 基建，列为后续 |
| O6 停机拒新范围 | **(a) 仅拒 POST /deploy 与 POST /deploy/jobs** | plan/probe 只读，drain 期间放行有助观测 |
| O7 job id | `wf_common::generate_id()` + `deploy-` 前缀 | 修正草案错误引用；前缀与执行 id 区分 |
| O8 作业保留期 | **(b) 可配** `deploy_job_retention_secs`（默认 600） | 与 `run_detach_idle_secs` 等配置风格一致 |
| O9 interrupted 回滚 | **(a) 不回滚**，仅落终态 + probe | 回滚本身是新的长操作，风险高；靠 probe 暴露实际状态 |

O1 选 (c) 的直接后果：CLI/Web 必须迁移到"提交 + 轮询"，这是本轮必须一并完成的改动（见第 6 节）。

## 2. 总体设计

### 2.1 核心抽象 DeployJob

新增内存作业表 + 作业句柄，复用 `detached.rs` 的"按主体配额、常驻 reaper、终态收敛"成熟思路，但部署作业更短命、步骤固定、按 backend 维度限制并发（每 backend 1 个在飞）。

```
POST .../deploy ──> 立即 202 { job_id, status:"queued" }
                         │ spawn 后台任务
                         ▼
                  后台 apply（逐步落进度）
                  每步写 steps 前缀 + current_step
                         │ 完成/失败/取消
                         ▼
                  succeeded | failed | interrupted
GET .../deploy/jobs/{id} ──> 读状态 + 步骤前缀 + outcome
POST .../deploy/jobs/{id}/cancel ──> 请求取消
GET .../deploy ──> 既有缓存视图（last outcome + probe），保留不变
```

### 2.2 状态机

`queued → running → succeeded`，另两支 `→ failed`（步骤/校验失败）、`→ interrupted`（停机或显式 cancel）。`interrupted` 与 `failed` 严格区分：语义是"操作被打断而非部署逻辑失败"，对齐执行落 `Cancelled` 的既有拍板。

### 2.3 关键原则

1. 提交即返：只做轻量校验 + 登记 + spawn，立即 202 返回。
2. 逐步落盘：每完成一步追加 `DeployStepResultView` 到 `steps` 并更新 `current_step`。
3. 可取消：给 apply 注入 `CancellationToken`，每步前检查；取消收敛 `interrupted`。
4. 停机收敛：drain 阶段先 cancel 在飞作业，有界等待落终态，再走 reaper/PTY/隧道关闭顺序。
5. 单一实现：O1 选 (c)，`POST /deploy` 本身就是提交，不再有同步分支。

## 3. 类型设计（deploy.rs）

- 新增 `DeployJobView`：`job_id / backend / action / status / current_step(Option) / steps(Vec<DeployStepResultView>) / outcome(Option<DeployOutcomeView>) / submitted_ms / finished_ms(Option)`。挂在 `:447` 之后，与 `DeployOutcomeView` 并列。
- `DeployOutcomeView`（`:433-447`）向后兼容地新增两个可选字段：`job_id: Option<String>`、`status: Option<String>`（缺省视为 succeeded）。字段名用 `_ms` 风格已符合 `AGENTS.md:76` 时间单位约定。
- `DeployJobStatus` 以字符串表达（`queued/running/succeeded/failed/interrupted`），不引入新枚举依赖；如引入枚举需 derive `ToSchema`。
- 所有新类型 derive `Debug, Clone, Serialize, Deserialize, ToSchema`，与既有 view 一致。

## 4. 执行层改造（deploy.rs）

- `apply`（`:1837`）签名增加 `cancel: &CancellationToken` 与进度回调 `on_progress: impl FnMut(DeployStepResultView)`。
- `push_step!`（`:1885-1904`）每步前 `select!`：取消令牌先完成则构造 `status:"interrupted"` 的 outcome 提前返回；否则照旧 `run_step`。每步完成后调用 `on_progress` 推送结果。
- blocked 分支（`:1849-1860`）不检查取消（尚未 spawn 实际步骤），但仍带上 `job_id`。
- `DeployChannel::run`（`:567`）已有 `kill_on_drop(true)`（`:587`），`select!` 取消时子进程被杀，远端无孤儿 ssh。
- 所有 `DeployOutcomeView` 构造点（`:1850/:1891/:1988`）统一补 `job_id`/`status` 字段。

## 5. manager 层改造（manager.rs）

### 5.1 存储（字段区 `:146` 附近新增）

- `deploy_jobs: Arc<DashMap<String, DeployJobView>>`（job_id → 作业视图）
- `deploy_job_handles: Arc<DashMap<String, JoinHandle<()>>>`（job_id → 任务句柄，用于 cancel/abort）
- `deploy_job_cancels: Arc<DashMap<String, CancellationToken>>`（job_id → 令牌）

内存 only，与 `deploy_outcomes` 并列。`new_with_metrics`（`:177-190`）同步初始化三个 DashMap。

### 5.2 方法

- `submit_deploy_job(name, request) -> Result<DeployJobView, String>`：
  - `require_enabled` + `entry_for`（复用 `:1520`）+ 轻量参数校验（复用 `parsed_action/parsed_scope`）；
  - 并发检查：该 backend 已有在飞作业（queued/running）→ 返回含 `409` 语义的错误，由 handler 映射为 409；
  - 生成 `deploy-{generate_id()}`，登记 queued 视图与 token/handle；
  - spawn 后台任务（`self: Arc<Self>` 模式，参照 `run_reconcile_loop` `:834`）：设 `running` → 调 `apply(cancel, on_progress)`，`on_progress` 把步骤结果写入 `job.steps` 并置 `current_step` → 终态写 `job.outcome/status`、同步写 `deploy_outcomes`/`deploy_probes`（沿用 `:1576-1580` 语义）→ 记录 `finished_ms`。
- `deploy_job_status(job_id) -> Option<DeployJobView>`：查询。
- `cancel_deploy_job(job_id) -> bool`：置取消令牌，任务侧收敛 interrupted。
- `cancel_deploy_jobs()`：供停机调用，cancel 全部在飞作业并 `timeout` 有界等待；超时则 abort 句柄并**强制写入 interrupted 终态**，保证任何被中断的部署都有记录。
- 终态作业保留 `deploy_job_retention_secs` 后由常驻 reaper 清理；在飞作业不回收。复用现有 reaper 生命周期（挂 `shutdown_token`），随 `backends.shutdown()`（`:749`）一并清理。

## 6. API 与三个入口

### 6.1 端点（api/system/backends.rs，路由组 `:57-70`）

| 方法 | 路径 | 响应 | handler |
|---|---|---|---|
| POST | `/system/backends/{name}/deploy` | `202 DeployJobView` | 改造 `:648` 为提交 |
| GET | `/system/backends/{name}/deploy/jobs/{id}` | `200 DeployJobView` | 新增 |
| POST | `/system/backends/{name}/deploy/jobs/{id}/cancel` | `202` | 新增 |

四个 `is_enabled` 守卫（`:601/624/656/687`）保持；新端点同样先判 `is_enabled`。错误映射沿用 `:661-675`：backend 不存在 → 404，参数错误 → 400，并发冲突 → 409，其余 → execution error。新增 view 需在 `openapi.rs` 的 `components(schemas(...))` 与 docs 注册，保证 `openapi.json` 快照无漂移（`openapi.rs:1198` 有快照测试）。

### 6.2 停机拒新（middleware.rs）

`is_new_workload`（`:406-439`）在 `launch_suffixes` 判断之后追加：POST 且路径以 `/deploy` 或 `/deploy/jobs` 结尾 → true。用后缀精确匹配，不误伤 `/deploy/plan`、`/deploy/probe`（O6）。补一条单测仿照 `:1224`。

### 6.3 关闭顺序（server.rs）

在 HTTP drain 完成（`:111`）之后、停 reaper（`:115`）之前插入 `#[cfg(feature = "backends")] backends.cancel_deploy_jobs().await`。必须在 `backends.shutdown()`（关隧道，`:134`）之前——隧道关闭后 SSH 步骤会失败，届时应落 failed 而非干净的 interrupted。注意 `self.backends` 在此处被后续 `:133` 再次使用，需调整借用（改为引用而非 move，或重排）。

### 6.4 CLI / Web

- CLI `remote deploy --apply`（`remote.rs:321-326`）：提交获 job id → 轮询 `GET jobs/{id}` 打印进度；新增 `--wait` 显式等待终态（默认也等待，保持现有 UX）。
- Web `applyBackendDeploy`（`backends.ts:266-277`）：提交后轮询 job 状态；部署面板（`RemoteServersPanel.svelte`）通过 `onProgress` 回调实时显示当前步骤、完成数与 job id。新增 TS 类型与 `schema.d.ts`、`openapi.json` 同步（codegen：仓库内 `tools/openapi-codegen`，`npm run gen`）。
- i18n：`pages.{en,zh}.json` 新增 `page.settings.remoteJobProgress` 键并经 `gen:i18n` 合并。取消按钮 UI 列为紧随任务（API 层 `cancelBackendDeployJob` 已就绪）。

## 7. 关键 diff 骨架

以下为方向性骨架，行号按基准提交，落地须重核。

**deploy.rs — apply 签名与 push_step 取消注入**

```diff
 pub async fn apply(..., local_port: u16,
+    cancel: &CancellationToken,
+    on_progress: impl FnMut(DeployStepResultView),
 ) -> Result<DeployOutcomeView, String> {
```

`:1885` 的 `push_step!` 在 `run_step(...).await` 前加 `tokio::select!` 分支：`cancel.cancelled()` 命中则返回 `status:Some("interrupted")` 的 outcome；`run_step` 命中后调用 `on_progress(result.clone())` 再判 `ok`。

**manager.rs — 提交作业（新增方法）**

```diff
+ pub async fn submit_deploy_job(self: &Arc<Self>, name: &str, request: DeployRequest)
+     -> Result<DeployJobView, String>;
+ pub async fn cancel_deploy_jobs(&self);
```

内部：并发检查 → 登记 queued → spawn（设 running → `apply(cancel, on_progress)` → 写终态 + 缓存）。

**middleware.rs — 停机拒新（`:431` 附近）**

```diff
+ if is_post && (path.ends_with("/deploy") || path.ends_with("/deploy/jobs")) {
+     return true;
+ }
```

**server.rs — 收敛插入（`:111` 之后）**

```diff
+ #[cfg(feature = "backends")]
+ if let Some(backends) = &self.backends { backends.cancel_deploy_jobs().await; }
```

（同时把 `:133` 的 `self.backends` move 改为借用，避免与此处冲突。）

## 8. 验证计划

| 层面 | 验证点 | 手段 |
|---|---|---|
| 提交 | 立即返 202 + job_id，不阻塞 | handler 单测（mock manager） |
| 进度 | running 期间 GET jobs/{id} 见步骤前缀 | 可控慢 mock（本地假 ssh） |
| 成功/失败 | 正常落 succeeded；步骤失败落 failed 且 steps 是失败前缀 | 复用 `deploy.rs:2002+` step 夹具 |
| 取消 | cancel 后半途停止，落 interrupted，无孤儿 ssh | cancel 单测 + kill_on_drop 语义 |
| 停机 | 在飞部署遇 shutdown 落 interrupted；新提交 503 拒 | 仿 `server.rs:224` 集成测试 |
| 并发 | 同 backend 第二提交 409 | manager 单测 |
| 兼容契约 | openapi.json 快照 + schema.d.ts 无漂移 | `openapi.rs:1198` 快照测试 + `npm run gen:check` |

基线：`cargo check --workspace` 全绿（默认特性集）。改动后跑 `cargo clippy --all-targets --all-features`、`cargo fmt --check`。

### 8.1 本轮执行记录（基准 528378c8）

| 项 | 结果 | 说明 |
|---|---|---|
| `cargo check -p wf-server --features backends` | 通过 | 首轮一处 `select!` 临时值生命周期错误（`:2006` `file_step(&layout, path)`）已用局部绑定修复 |
| `cargo check -p wf-cli-shared` | 通过 | CLI 提交+轮询改造无类型错误 |
| `cargo test -p wf-server --all-features --lib` | 通过（294+1） | 新增 reaper 单测与守卫断言；openapi 的 200-only 断言放宽为 2xx（202 合法） |
| OpenAPI 快照 | 已刷新 | `WF_REFRESH_OPENAPI=1 cargo test -p wf-server --all-features --lib openapi::tests::committed_snapshot_matches_document`；注意模块需 file-checkpoint/git-fs/pty/rag-vector/backends/metrics 六特性，`--no-default-features` 下测试不编译 |
| `schema.d.ts` | 已重生成 | `tools/openapi-codegen` 的 `npm run gen` |
| 前端 `npm run typecheck` | 通过 | 依赖已装（node 22.13 略低于 kit 要求 22.17）；含 Svelte 进度块与新 i18n 键 |
| `cargo clippy -p wf-server --features backends --lib` | 无 error | 仅基线的 unused workspace 依赖警告 |
| `cargo fmt --check` | 本任务改动文件全通过 | rustfmt 1.100（edition 2021）格式化；仓库基线文件仍有历史格式差异，非本轮引入 |
| 进度/成功失败/取消/停机/并发端到端 | **待开发板验证** | 本地无 SSH 目标机；单测只覆盖状态机纯逻辑与守卫，真实 SSH 步骤收敛与孤儿进程检查须在目标机执行 |
| `cargo fmt --check` / clippy 全量 | 待跑 | 作为交付前最后门禁 |

## 9. 与既有方案的关系

| 维度 | 写路径方案（remote-deploy-write-path.md） | 本方案 |
|---|---|---|
| 关注点 | 部署什么/怎么部署 | 以什么生命周期执行 |
| 远端 desiredState | 不维护 | 仍不维护 |
| probe | 独立只读验证面 | 不变 |
| 状态存储 | deploy_outcomes 内存 | 新增 deploy_jobs 内存；durable 化不在范围 |
| 停机 | 未覆盖 | 核心：拒新 + cancel + interrupted 终态 |
| CLI/Web | 同步等待 | 提交 + 轮询 + 取消 |

与保活方案（remote-keepalive-and-shutdown-semantics.md）共享"终态收敛"哲学：执行落 `Cancelled`、部署落 `interrupted`。

## 10. 落地顺序

开放点已全部拍板（O1~O9 见 1.3），无阻塞项。内部顺序：

1. 类型与作业表（deploy.rs / manager.rs）
2. apply 取消注入（deploy.rs）
3. 提交/查询/取消端点 + openapi 注册（backends.rs / openapi.rs）
4. 停机收敛（middleware.rs / server.rs / manager.rs）
5. CLI / Web 适配（remote.rs / backends.ts / schema.d.ts / openapi.json）
6. 单测与集成测试补齐
7. cargo fmt / clippy / check + 契约快照校验

## 11. 待拍板与下一阶段

本轮已拍板 O1~O9，无遗留阻塞。以下为明确不纳入本任务、另立后续的方向：

- 作业状态持久化（O2 增强项）：落本地 store，支持重启对账。
- SSE 进度推送（O5 增强项）：复用既有 stream 基建。
- 部署回滚（O9 增强项）：interrupted 后尝试恢复，风险高。
- 部署面板的取消按钮控件（本轮已做进度展示 + API 层 `cancelBackendDeployJob`，按钮交互另立）。
