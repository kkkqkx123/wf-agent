# Checkpoint 子系统文档

本目录是 `crates/infra/checkpoint/` 子系统的功能与概念语义文档。子系统为工作流与 Agent 循环提供两类正交的检查点能力:

- **执行状态检查点(execution-state checkpoint)**:对结构化执行状态(消息、变量、节点结果)做快照/增量持久化,支持从增量链恢复;
- **文件内容检查点(file-content checkpoint)**:对工作区内文件做基于独立 Git 对象库的文件历史、恢复与溯源。

两套体系共享 `checkpoint-base` 的基础设施(错误、时钟、策略、序列化),但存储、清理与恢复路径完全独立,不共享实现。

## 文档索引

| 文档 | 内容 |
| --- | --- |
| [concepts.md](concepts.md) | 核心概念总表:检查点、增量链、策略、Actor、分区、分支、合并/审批、层级恢复等术语语义 |
| [checkpoint-base.md](checkpoint-base.md) | 基础层:错误、时钟、缓存、ActorId、diff、增量计算/恢复、策略、元数据、序列化、清理策略、版本管理 |
| [checkpoint-state.md](checkpoint-state.md) | 状态层:状态管理器 trait、存储后端实现、Agent/Workflow 检查点类型、层级恢复与恢复策略注册表 |
| [checkpoint-file.md](checkpoint-file.md) | 文件层:Git 对象库、refs 语义、SQLite 元数据、Manager、捕获三通道(精确/脚本/外部轮询)、会话、溯源、审批、GC |
| [wf-checkpoint.md](wf-checkpoint.md) | 门面层:CheckpointCoordinator(Workflow/Agent)、持久化队列、文件投影、错误处理、子级发现与恢复、迁移 |

## Crate 依赖结构

```text
checkpoint-base (叶子:错误、时钟、diff、策略、序列化、ActorId)
    ↑
checkpoint-state (执行状态管理器 + 层级恢复)      checkpoint-file (文件历史 + Git 对象库)
    ↑                                              ↑
    └──────────── wf-checkpoint (门面 + 协调器) ────┘
```

外部约束:上层(`wf-tools` / `wf-execution-shared` / `wf-agent` / `wf-workflow` / `wf-api` / `wf-runtime`)只依赖 `wf-checkpoint` 门面,不直接依赖内部 crate。

## 两类检查点对比

| 维度 | 执行状态检查点 | 文件内容检查点 |
| --- | --- | --- |
| 内容 | 消息、变量、节点结果等结构化状态 | 工作区文件字节 |
| 存储 | `wf-storage` 后端(SQLite 元数据 + 二进制 blob) | 独立 bare Git 库(`<workspace>/.wf-checkpoint-git`)+ 少量 SQLite 元数据 |
| 增量方式 | Full/Delta 链(差分计算 + 链回放) | Git 对象去重 + commit 链 |
| 恢复 | 增量链回放得到快照 | 按状态→文件链接检出树到磁盘 |
| 清理 | `cleanup_policy`(年龄/数量/大小/分层 + 链保护) | `gc`(基于 ref 可达性的对象回收) |
| 概念图 | 线性 previous-id 链(`CheckpointDependencyGraph`) | 多父内容 DAG(Git commit 图) |

## 关键不变式(摘自代码)

- 文件历史的唯一事实来源是 bare Git 库;SQLite 只保存 Git 无法廉价表达的数据(审批状态、源索引缓存、状态→文件链接、KV 元数据);用户的 `.git` 永不读写。
- 状态检查点写库失败默认上抛(`CheckpointErrorHandler` 决定),绝不把失败伪装成已保存;文件投影(state→file)是 best-effort,失败不影响状态检查点。
- `ActorId` 编码(格式 A)必须保持稳定,它是文件分区的 UUIDv5 种子。
- 两个概念图(状态链 vs Git DAG)绝不共享节点、不合并为通用图。
