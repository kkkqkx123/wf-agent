# 远端执行数据的本地备份

## 一、职责边界

本地备份发生在管理端 `wf-server`，不是远端节点自备份，也不是 CLI 直接落盘。管理端通过服务端注册表找到远端主机，经本地回环隧道访问远端只读同步接口，把执行数据写到管理端本地目录。

远端节点使用 `remote-node` feature，显式不包含后端管理域；`remote-node` 与 `backends` 同时开启会在启动时拒绝，避免远端再管理一层远端。

## 二、启用条件与配置

备份链路需要同时满足：

* 管理端启用 `backends` 域：`backends.toml` 中 `enabled = true` 或 `WF_BACKEND_ENABLED`。
* 配置 `backup_dir` 或 `WF_BACKEND_BACKUP_DIR`。未配置时不启动备份循环；手动触发也会失败。
* 注册表条目配置 `backup_executions`，列出要备份的远端执行 id。条目还可覆盖 `backup_interval_secs`、`backup_retention_count`、`backup_retention_days`。

全局默认值：备份周期 1 小时，保留 10 份，保留 7 天。备份文件与索引默认放在 `backup_dir` 下，索引库名为 `backends.db`；也可用 `registry_path` 指定索引位置。

远端同步请求携带两个上下文：

* `x-api-key`：来自 `WF_BACKEND_REMOTE_API_KEY`，不写入配置文件。
* `x-wf-directory`：优先取条目的 `backup_directory`，回退到管理端全局 `backup_directory`，可由文件或 `WF_BACKEND_BACKUP_DIRECTORY` 配置。

## 三、备份内容

一次备份对应一个执行 id。远端专用端点 `GET /api/v1/remote-sync/executions/{id}/bundle` 在一次响应中返回：

* 执行包：统一历史视图、artifact 预览、审计摘要、bundle 版本与导出时间。
* 检查点链：该执行的检查点记录、转移分析和时间范围。

远端先做一次执行归属检查，再导出两部分数据，避免不同执行的数据混在同一响应里。检查点链包含元数据和记录，不包含文件历史 blob 或数据库快照；历史时间线有上限，artifact 只保留预览。因此备份文件用于事后复查和人工复现，不是热恢复镜像。

## 四、触发与调度

计划备份由管理端后台循环驱动：

* 使用共享 interval，首次立即 tick 被跳过，避免服务重启后全量拉取突发。
* 迟到的 tick 直接跳过，不补积压。
* 到期状态按“后端名 + 执行 id”分别记录，一个执行成功不会掩盖另一个执行的状态。
* 到期任务按小批量并发执行，单次拉取的超时预算为 15 秒。
* 成功才推进该执行的上次成功时间；失败保留原时间并在下一周期自然重试。
* 关闭服务时取消循环并回收隧道子进程。

手动触发通过 `POST /api/v1/system/backends/{name}/backups` 走同一条备份路径；CLI 对应 `wf remote backup-now NAME --execution ID`。后端不存在返回 404，其他失败按服务端执行错误返回。

## 五、同步请求与校验

管理端从条目的本地隧道地址发起 GET 请求。拉取成功后不会直接写盘，而是依次校验：

* HTTP 状态为成功；
* 响应信封 `success` 为真；
* `data` 存在；
* `bundle` 与 `checkpoints` 都是对象；
* 两者的 `execution_id` 都等于请求的执行 id。

任一条件不满足都按失败处理：不写文件、不写索引、不推进到期时间、不触发裁剪。这样鉴权失败、目录不匹配、执行不存在等错误信封不会被写成“空备份”。

## 六、落盘与索引

校验通过后，检查点链被合入 bundle 的 `checkpoints` 字段，然后写入 `backup_dir` 下按后端名分目录的 `{execution}-{created_ms}-{suffix}.json` 文件。

后端名、执行 id 和文件后缀都必须通过路径组件校验，拒绝路径分隔符和相对路径成分。

写入过程使用同目录临时文件：先写临时文件，读取落盘字节计算 SHA-256，再原子改名为最终 JSON。索引写入失败会删除已发布文件，避免留下未登记的备份。索引记录包含 id、后端、执行、创建时间、字节数、SHA-256 和文件路径。

## 七、保留策略

每次成功备份后按后端单独裁剪，数量上限和时间上限任一命中即删除：

* 按创建时间倒序，超出数量上限的记录进入删除列表。
* 超过保留天数的记录进入删除列表。
* 先删索引行，再删文件；文件不存在视为已清理，文件删除失败只记告警。

这样不会出现索引指向已删文件的记录；文件删除失败最多留下孤儿文件，不影响列表可信度。

## 八、查询与手动拉取

管理端接口：

* `GET /api/v1/system/backends`：后端视图含 `last_backup_ms` 与 `backup_error`，顶层含 `backup_round_error`。
* `GET /api/v1/system/backups`：全部备份索引。
* `GET /api/v1/system/backends/{name}/backups`：单个后端的备份索引。

CLI：

* `wf remote backups [--backend NAME]` 展示 id、后端、执行、时间、大小、SHA-256 与路径。
* `wf remote status` 展示每行的 `last_backup_ms` 与 `backup_error`。
* `wf remote pull ID DEST` 是独立的手动路径：从当前连接地址拉取通用 bundle 与检查点链并写本地文件，不写索引、不参与裁剪；任一步失败都直接报错。
