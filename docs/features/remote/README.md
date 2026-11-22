# 远端模式功能文档

远端模式采用“远端执行节点 + 本地管理面”的职责划分：远端只保留执行能力，本地管理端持有注册表、SSH 隧道、备份索引和备份文件。

本目录整理当前远程服务模式下与备份相关的实现：

* [`backup.md`](backup.md)：备份数据来源、触发方式、请求与落盘流程、保留策略、查询入口。
* [`error-handling.md`](error-handling.md)：各阶段失败的判定、状态记录、可见性、重试与已知边界。

配置示例见 `config-examples/server/backends.toml`，改造背景见 `docs/plan/remote-backup-hardening.md`，连接与发现的总览见 `docs/features/server-handling-and-discovery.md`。

远端部署（安装、启停、升级、卸载）属于同一 `backends` 域的另一半能力，设计见 `docs/plan/remote-deploy-write-path.md`：产物默认远端下载，配置/脚本从管理端部署源目录按 `config`/`full` 范围传输，卸载一次删净安装根目录，独立 `probe` 给出部署判定；不维护 `desiredState`。连接保活与关闭语义的设计见 `docs/plan/remote-keepalive-and-shutdown-semantics.md`。
