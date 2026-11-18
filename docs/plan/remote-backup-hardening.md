# 远端备份加固方案

面向 `wf-server` 的 `backends` 域备份链路。评审结论是架构方向正确（管理端拉取、远端只读同步端点、失败不落盘），但缺少可验证性、崩溃安全、按目标裁剪与可运维性。本方案按"先正确性、再持久性、后观测"的顺序分六个阶段落地，不改变备份数据形态，不引入新依赖。

## 一、问题清单

行号基于当前主干；每条给出证据与影响，作为后续阶段的施工依据。

### 致命：会造成数据损失或安全假象

| 编号 | 问题 | 证据 | 影响 |
|---|---|---|---|
| C1 | 保留策略按后端聚合，多个执行共享同一数量配额 | `backends/backup.rs:240`、`backup.rs:246`、`backends/manager.rs:283` | 一个后端下 N 个执行时每个只留 `count/N` 份；失败执行的旧备份因"最旧"被兄弟执行的成功备份逐轮挤掉，恰在最需要历史时丢失 |
| C2 | 临时文件 + 改名全程无 fsync | `backups/backup.rs:137`、`backup.rs:149` | 断电后可能出现零长度或截断文件顶着正式文件名；索引里的摘要描述的是崩溃前内存字节，无人发现 |
| C3 | SHA-256 只写入不校验，且无恢复路径 | 写入侧 `backup.rs:114`、`backup.rs:148`；展示侧 `cli/wf-cli-shared/src/cmd/remote.rs:312`；全仓库无读取侧消费方 | 完整性只是纸面承诺，备份从未被验证可用 |
| C4 | SQLite 打不开时静默降级为内存索引 | `wf-server/src/router.rs:332`、`router.rs:340` | 违反本项目"不静默降级"原则；注册表变空导致备份全停，或备份继续写盘但索引为空、裁剪看不见、孤儿文件无限增长 |

### 重要：可用性与可运维性

| 编号 | 问题 | 证据 | 影响 |
|---|---|---|---|
| M1 | 到期状态只在内存 | `manager.rs:42`、`manager.rs:240` | 重启后所有执行同时到期，产生全量并发拉取；`backup_interval_secs` 在重启后失真 |
| M2 | 无抖动、无退避 | `manager.rs:171`、`manager.rs:255` | 到期任务集中在同一 tick 出发；坏执行按固定节奏永久重试 |
| M3 | 保留参数未校验 | `backends/config.rs:168`、`backends/types.rs:95` | `retention_count = 0` 或 `retention_days = 0` 会删光包含刚写入那一份的全部备份 |
| M4 | 拉取超时硬编码、无体积上限、检查点链无界 | `backup.rs:207`、`wf-api/src/checkpoint/record.rs:211`、`api/observation/bundle.rs:87` | 检查点多的执行每轮必然超时；宿主内存峰值随执行规模线性上升 |
| M5 | 备份路径的同步 IO 在 async 上下文 | `backup.rs:107`、`backup.rs:137`、`backup.rs:141`、`backup.rs:257` | 大文件占满 tokio worker；读回算哈希使 IO 与内存副本翻倍 |
| M6 | 无孤儿对账 | 无目录扫描逻辑 | 改名前崩溃、删文件失败、内存索引降级三种情况都会留下无人管理的文件 |

### 次要：加固项

| 编号 | 问题 | 证据 | 影响 |
|---|---|---|---|
| L1 | 备份无指标、无告警 | `backends/` 下无 metrics 引用 | 失败只能靠日志与接口轮询 |
| L2 | 明文 JSON、权限未收紧、无异地副本 | `backup.rs:110` | 含提示词与产物预览的包体对本地任意用户可读 |
| L3 | 手动与定时备份无互斥 | `manager.rs:510` 与 `manager.rs:217` | 同一目标可能并发备份并交错裁剪 |
| L4 | 换 `backup_dir` 后按旧绝对路径删除与展示 | 索引存 `path`，裁剪直接 `remove_file` | 迁移目录后旧记录指向旧位置 |
| L5 | 包体无版本字段 | `execution_bundle.rs:17` 只有 bundle 版本 | 合成体格式演进后历史文件无法自证版本 |
| L6 | 条目 `backup_executions` 未在写入时校验 | `types.rs:74` 未校验该字段，`backup.rs:82` 才失败 | 坏条目在创建时被接受，之后每轮都产生同一条告警 |
| L7 | `backup_round_error` 未进 CLI | `error-handling.md:45` 自述 | 轮级失败在终端不可见 |

### 文档需同步的事实

`docs/features/remote/backup.md:37` 声称跳过首个 tick 可避免重启后突发，实际只推迟一个周期；`backup.md:64` 描述"原子改名"但未声明无 fsync；`backup.md:31` 的"历史时间线有上限"只对时间线成立，检查点链无上限。

## 二、总体原则

- 不改变备份产物的数据形态与既有端点形态，只在其上增加能力。
- 不新增依赖；沿用 `sha2`、`hex`、`sqlx`、`tokio`。
- 失败继续保持大声：拒绝静默降级、拒绝把错误信封写成空备份。
- 已有的备份文件与索引在升级后仍可读，不做破坏性迁移。

## 三、阶段划分

### 阶段一：保留策略按目标裁剪

裁剪的作用域从后端收窄到"后端 + 执行"，与 `backup_executions` 的语义对齐：每个执行是独立备份目标，各自持有 `retention_count` 与 `retention_days` 配额。

实现要点：`prune_backups` 增加执行参数；存储层增加按执行过滤的查询；索引表补 `(backend, execution, created_ms)` 复合索引以支撑新的裁剪查询与后续到期判定的兜底查询。

同时在配置校验与条目校验中要求保留数量与保留天数至少为 1，把"删光全部备份"的配置错误挡在启动期与写入期。

### 阶段二：崩溃安全与异步 IO

落盘顺序改为写入、同步文件、改名、同步目录；摘要改为边写边算，去掉读回临时文件这一步。整个落盘过程放进阻塞任务池，目录创建与裁剪删除改用异步文件接口。

备份目录创建时收紧权限，包体文件按仅所有者可读写落盘。合成体增加格式版本字段，为将来的读取方提供判据。序列化从美化输出改为紧凑输出，减小体积。

### 阶段三：去掉静默降级并补对账

存储打开失败时不再退化为内存索引：后端域启用且给定了索引路径或备份目录时，直接让启动失败并给出明确原因，与管理端其他配置"存在但非法即失败"的处理保持一致。

新增启动期对账：清理上次崩溃残留的临时文件，统计备份目录中无索引记录的文件与索引中缺文件记录的条数，聚合成一条告警。对账只报告，不自动删除用户数据。

### 阶段四：备份完整性校验入口

新增按备份 id 复算摘要的路径：存储层按 id 取记录，管理器读回文件重算 SHA-256 与字节数并与索引比对，路由层暴露查询接口，CLI 提供对应子命令。

校验失败要区分三种情况：记录不存在、文件缺失、内容不匹配。三者都必须返回明确错误，不能合并成布尔值。

### 阶段五：调度健壮性

到期判定改为"内存态优先、索引兜底"：内存中没有该目标的上次成功时间时，用索引中该目标的最大创建时间作为兜底，使重启不再造成全量突发。

到期时刻加入按目标名派生的确定性抖动，把同一轮的任务摊开在一个小窗口内。失败次数记录到状态中，按指数退避拉长该目标的有效间隔并设上限，成功即清零。

同一目标的手动触发与定时备份通过进程内锁串行化，避免同一目标并发写入与裁剪交错。

### 阶段六：文档同步

代码落地后同步 `docs/features/remote/backup.md` 与 `error-handling.md`：修正重启突发的表述，补充 fsync、校验入口、按执行裁剪语义、检查点链无上限、不降级行为与新 CLI 子命令。

## 四、验收

- `cargo fmt` 与 `cargo check -p wf-server --features backends` 通过。
- 保留策略：同一后端下三个执行各跑多轮后，每个执行各自保留到上限，互不挤占。
- 落盘：临时文件不存在残留；索引摘要与文件内容一致；目录权限仅所有者可写。
- 存储：坏库文件在启用状态下导致启动失败，不再静默使用内存索引。
- 校验：篡改文件内容后校验接口报内容不匹配；删除文件后报文件缺失。
- 调度：重启后不出现全部目标同时到期；连续失败的目标间隔递增。

## 五、不做的事

- 不做备份到对象存储或其他远端，不引入异地副本。
- 不做包体加密，不做恢复/导入执行语义（需要独立的只读导入设计）。
- 不改备份数据形态，不新增跨实例分布式锁。
- 不把裁剪改成后台独立任务，仍在成功路径后触发。

## 六、实施状态

六个阶段均已落地。按问题条目回填：

| 条目 | 状态 | 落地位置 |
|---|---|---|
| C1 保留策略作用域 | 已修复 | `backup.rs` 的 `prune_backups` 增加执行参数，`store.rs` 增加按目标查询与复合索引，`manager.rs` 两处调用同步收窄 |
| C2 落盘无 fsync | 已修复 | `backup.rs` 的 `write_backup_file` 改为创建私有文件、边写边算摘要、同步文件、改名、同步目录 |
| C3 摘要无校验、无恢复 | 部分修复 | 新增 `store.rs::get_backup`、`manager.rs::verify_backup`、校验路由与 `wf remote backup-verify`；恢复/导入仍未实现，按第五节约定不在本方案内 |
| C4 静默降级 | 已修复 | `router.rs` 的两个启动入口改为 `?`，坏库直接失败 |
| M1 到期状态不持久 | 已修复 | `store.rs::latest_backup_ms` 作为内存态兜底 |
| M2 无抖动无退避 | 已修复 | `manager.rs` 增加确定性抖动、`failures` 计数与封顶指数退避 |
| M3 保留参数未校验 | 已修复 | `config.rs::validate` 与 `validate_backend_entry` |
| M4 超时、体积、检查点链 | 未处理 | 涉及导出 API 形态变更，超出本方案边界；当前仍为 15 秒固定超时、无响应体上限、检查点链无界 |
| M5 同步 IO 阻塞 | 已修复 | 落盘与校验读文件进 `spawn_blocking`，目录创建与裁剪删除改异步接口，读回算哈希已去掉 |
| M6 无孤儿对账 | 已修复 | `backup.rs::reconcile_backups` 与启动期一次性任务，只报告不删数据 |
| L1 无指标 | 未处理 | 留待可观测性专项 |
| L2 权限 | 部分修复 | 备份目录 0700、包体文件 0600；加密与异地副本仍不做 |
| L3 手动与定时并发 | 已修复 | `manager.rs::backup_guard` 按目标加锁并在删除后端时清理 |
| L4 绝对路径索引 | 未处理 | 需要索引迁移，不在本方案内 |
| L5 包体无版本 | 已修复 | 合成体增加 `backup_version` 字段 |
| L6 条目非法执行 id | 已修复 | `validate_backend_entry` 写入期校验 |
| L7 轮级错误不可见 | 未处理 | CLI 状态展示调整，留待下一批 |

新增测试覆盖：按目标裁剪不挤占兄弟执行、零保留参数被拒、非法执行 id 被拒、退避封顶、抖动有界、错误信封拒绝、落盘校验与索引、越界路径拒绝。

文档同步：`docs/features/remote/backup.md` 与 `error-handling.md` 已按上述行为更新。

验证结果：

* `cargo check -p wf-server --no-default-features --features backends,file-checkpoint,storage-sqlite --lib` 通过；仅有三个既存 dead_code 警告。
* `cargo check -p wf-cli-shared` 通过。
* `cargo test -p wf-server --no-default-features --features backends,file-checkpoint,storage-sqlite --lib backends::` 通过。
* 验证环境的 `rust-toolchain.toml` 会触发跨平台 rust-std 下载，实际执行时显式指定本机工具链；本机 stable 原为 1.93，不满足 `checkpoint-base` 对新 stable API 的要求，已通过镜像升级到 1.100。
* 顺带修正 `probe.rs` 的一个过期测试夹具：探测读取 `apiVersion`，测试桩原来返回 `api_version`，导致既有测试必然失败。该修正只影响测试，不涉及运行逻辑。

---

## 七、第二轮：剩余问题改进

### 7.1 范围与边界

第一轮按第五节约定留下了四类未处理项（M4、L1、L4、L7）与"无恢复路径"这条最要紧的判断。本轮逐项收敛：

* M4、L4、L7 是纯实现缺陷，直接修复。
* L1 需要跨 `wf-metrics` 与 `wf-server` 接线，本轮接入。
* 恢复路径做一个边界收敛：**不实现把备份包重放成执行**（那需要 `wf-api` 的只读导入语义与新的写入面，属于独立设计议题），但补齐"可取出、可本地自检"这一半——备份包能按 id 原样导出到本机，本地能检视其结构与摘要。这样当前的产物至少具备可验证的取回路径，不再只是一个只能写入、无法读出的文件。

原则不变：不新增依赖，不破坏既有备份文件与索引的可读性，失败继续大声。

### 7.2 M4：超时、响应体上限与检查点链

证据：固定 15 秒超时在 `backup.rs:294`；响应体经 `response.json()` 无长度校验在 `backup.rs:311-314`；检查点链无界在 `checkpoint/record.rs:211,373` 与 `api/observation/bundle.rs:87`。

改进：

* `BackendConfig` 新增三个带默认值的参数：拉取超时秒数、响应体字节上限、检查点条数上限；文件层、环境层、校验层同步接线。参数经 `BackupRequest` 透传到拉取函数，不再硬编码。
* 拉取响应改为先读字节、校验长度不超过上限、再做 JSON 解码；超限直接失败，避免无界 payload 进入解析器。
* 检查点链增加有界构建函数：按时间排序后保留最近 N 条，产物新增 `truncated` 与 `total_count` 字段自证是否被裁剪；原无界函数保留为包装，其他调用方行为不变。远端同步端点传入配置上限。

开放点：三个参数的默认值（15 秒、64 MiB、500 条）需在开发板用大执行验证；若某执行检查点确实超过上限，备份将只包含最近窗口，需在文档显式声明。

### 7.3 L4：索引路径相对化

证据：写入时把绝对路径写入 `path`（`backup.rs:141`）；裁剪、校验、对账分别直接按该绝对路径删除或读取（`backup.rs:407`、`manager.rs:628`、`backup.rs:342,374`）。

改进：

* 索引的 `path` 改存相对于备份根目录的路径；新增"相对路径转绝对路径"的单一解析函数，绝对路径存量记录按原样使用，保证升级后旧索引仍可读。
* 裁剪、校验、对账统一通过该解析函数取真实路径，管理器把当前备份根目录注入这些调用。
* 换目录后，新记录自动落到新根目录；旧记录的绝对路径仍指向原位置，不会被误删到新目录之外。

开放点：是否提供一次性把存量绝对路径重写为相对路径的迁移命令，本轮不做，留给后续运维需求。

### 7.4 L7：轮级错误进入 CLI

证据：服务端 `BackendListView` 已有 `backup_round_error` 并在列表接口填充（`api/system/backends.rs:52,122`），但 CLI 的 `fetch_server_backends` 只解析 `backends` 数组、丢弃顶层字段（`cli/wf-cli-shared/src/backends.rs:320-349`），状态输出也没有该字段（`cmd/remote.rs:250-290`）。

改进：CLI 用一个快照结构同时承载后端行与轮级错误，`fetch_server_backends` 返回该结构；所有调用点适配；`wf remote status` 输出增加 `backup_round_error`，使整轮失败在终端可见。

### 7.5 L1：指标与告警

证据：`backends/` 下无任何 metrics 引用；失败只能靠日志与接口轮询。

改进：

* `wf-metrics` 新增备份专用 collector，指标覆盖：成功与失败次数、单次耗时、备份字节数、校验结果、裁剪数量、对账发现的孤儿与缺失数。
* 注册进 `MetricsRegistry` 的构造、访问器、`collectors()` 与 `collector_names()`，使 `/metrics` 自动导出；沿用现有 label 治理与保留窗口机制。
* 管理器持有可选的 registry 并在结果处理处上报，把指标逻辑挡在纯备份函数之外；`router.rs` 在构建管理器时注入。
* 不内建告警发送：告警以 Prometheus 抓取加既有异常阈值机制承载，避免引入通知通道依赖。

开放点：label 基数——后端名与执行 id 均作为 label，执行数很多时序列数会增长；后续可按需要降为只按后端聚合。

### 7.6 恢复路径（可取出、可自检）

证据：SHA-256 只在写入侧产生，全仓库无读取消费方；无导出、无导入命令。

改进：

* 服务端新增按备份 id 导出原始 payload 的只读端点，复用记录读取与文件读取逻辑，返回包体与记录的摘要、大小供核对。
* CLI 新增 `backup-export ID DEST`：调用该端点把备份包写到本机指定路径，写入同样用临时文件与原子改名。
* CLI 新增 `backup-check FILE`：本地检视包体结构、格式版本、执行 id 一致性、检查点对象，并在给出期望摘要时重算 SHA-256 比对。
* 明确不做：不把备份包导入为可执行状态、不重放执行、不改 `wf-api` 数据模型。

### 7.7 实施状态

| 条目 | 状态 | 落地位置 |
|---|---|---|
| M4 超时/体积/检查点链 | 已修复 | `config.rs` 三个新参数与校验；`backup.rs` 可配置超时、字节上限校验、`checkpoint_limit` 查询参数；`checkpoint/record.rs` 的 `build_chain_limited` 与 `chain_for_execution_limited`；`bundle.rs` 端点接收上限 |
| L4 相对路径 | 已修复 | `backup.rs` 的 `relative_backup_path`/`resolve_backup_path`/`normalize_path`，写入存相对路径、裁剪与对账按当前 root 解析并拒绝越界；`manager.rs` 校验与裁剪注入 root |
| L7 轮级错误进 CLI | 已修复 | `cli/wf-cli-shared/src/backends.rs` 解析顶层 `backup_round_error` 并镜像到每行；`cmd/remote.rs` 状态输出增加该字段 |
| L1 指标 | 已修复 | `wf-metrics` 新增 `BackupMetricsCollector` 与 8 个指标、注册进 registry；`manager.rs` 在成功、失败、校验、裁剪、对账处上报；`router.rs` 注入 registry |
| 恢复路径 | 已修复（边界内） | 服务端 `GET /system/backups/{id}/export` 导出前复验；CLI `backup-export ID DEST` 原子落盘、`backup-check FILE [--expect-sha256]` 本地检视；不实现执行重放 |

新增单测：检查点链截断保留最近条目并上报 `total_count`/`truncated`；相对路径跟随 root 且拒绝越界；备份 collector 计数与 Prometheus 导出。

验证结果：

* `cargo check -p wf-metrics -p wf-api -p wf-server -p wf-cli-shared --no-default-features --features wf-server/backends,wf-server/file-checkpoint,wf-server/storage-sqlite` 通过（仅 3 个既存 dead_code 警告）。
* `cargo test -p wf-metrics -p wf-api --lib`：wf-metrics 151 项通过；wf-api 通过。
* `cargo test -p wf-server --no-default-features --features backends,file-checkpoint,storage-sqlite --lib backends::`：41 项通过。
* `cargo fmt --all -- --check`：通过。

开放点（未变）：三个新参数默认值需开发板压测；`backup_checkpoint_limit` 生效后，超长执行的备份仅含最近窗口，已在文档声明；未做存量绝对路径索引的一次性迁移命令。
