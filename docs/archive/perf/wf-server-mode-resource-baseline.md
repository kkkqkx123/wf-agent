# wf-server 各运行模式资源占用对照

本文把已完成的 release 实测按统一口径整理成基线，供后续改动前后对照。数据来自 devs（32 核、32 GiB、glibc 2.27、stable Rust）；所有数值均为空闲态：进程启动完成并响应一次 `/health` 后读取 `/proc/<pid>/status`、`smaps_rollup` 与按映射聚合的 `smaps`。单位统一为 MiB（1 MiB = 1024 KiB），二进制大小按字节换算后取整。

测量脚本留在 devs 的 `/data/kkkqkx/wf-agent/measure.sh` 与 `maps.sh`，二进制分别以 `wf-server-*` 与 `wf-server-embedded-*` 命名，不回传仓库。

## 一、口径说明

不同批次的构建配置不同，跨批次比较二进制大小时需先看构建配置：

| 批次 | 构建配置 | 运行环境 |
|---|---|---|
| A：改造前基线 | 默认 release，未 strip、未启用 LTO | 32 核、glibc 默认 arena |
| B：改造后 | release + `strip = "symbols"` + `lto = "thin"` + `codegen-units = 1` | 2 核（`taskset`）、`MALLOC_ARENA_MAX=1` |

批次 A 中额外测过 strip 后的二进制体积，用于分离“符号剥离”与“功能裁剪”的收益。批次 B 的 VmSize 下降主要来自 arena 限制与线程数减少，不是 LTO 的直接效果。

## 二、批次 A：改造前基线

### A1 三种 feature 档位，32 核

| 档位 | 二进制 | 依赖数 | RSS | 匿名 | 文件映射 | VmSize | 线程 |
|---|---:|---:|---:|---:|---:|---:|---:|
| 默认 | 151 | 482 | 38.7 | 9.3 | 29.4 | 2313 | 34 |
| 仅 Lua 插件 | 125 | 411 | 36.2 | 8.9 | 27.2 | 2296 | 34 |
| 无插件 | 121 | 401 | 34.0 | 8.1 | 25.9 | 2290 | 34 |

三种档位的 RSS 差（约 2～5 MiB）主要来自插件引擎对象与代码页；虚拟地址空间几乎相同，说明 2.3 GiB 的预留与插件后端无关。

### A2 环境与开关的影响

| 场景 | 二进制 | RSS | 匿名 | 文件映射 | VmSize | 线程 |
|---|---:|---:|---:|---:|---:|---:|
| 默认，限制 2 核 | 151 | 35.2 | 6.6 | 28.5 | 330 | 4 |
| 无插件，限制 2 核 | 121 | 31.8 | 6.0 | 25.8 | 308 | 4 |
| 默认 + SQLite 存储 | 151 | 40.2 | 10.5 | 29.7 | 2578 | 38 |
| 默认，关闭关键词索引与 workspace 域 | 151 | 36.6 | — | — | 2313 | 34 |
| 默认 + `MALLOC_ARENA_MAX=2` | 151 | 38.1 | — | — | 265 | 34 |

要点：

- 2 核场景用 `taskset` 限制亲和性，glibc 仍按 32 核计算 arena 大小，因此 VmSize 从 2313 MiB 降到 330 MiB 主要来自线程数减少。
- `MALLOC_ARENA_MAX=2` 在 34 线程下把 VmSize 从 2313 MiB 降到 265 MiB，RSS 基本不变；确认大块 `PROT_NONE` 映射来自 glibc 每线程 arena，而非 wasmtime。
- SQLite 存储额外增加约 1.5 MiB RSS、4 个线程与约 265 MiB VmSize（连接与页缓存、线程池）。
- 关闭关键词索引只省约 2 MiB RSS，空闲态收益有限；主要价值是避免索引代码路径与后续索引增长。
- 进程最大的常驻映射是二进制代码段（约 23 MiB），其次是约 6 MiB 堆与约 5.7 MiB 只读数据；LuaJIT 仅约 64 KiB 代码页。空闲态 RSS 的大头是链接进来的代码，而非活跃数据结构。

### A3 strip 的收益

| 档位 | 原始二进制 | strip 后 | 缩减 |
|---|---:|---:|---:|
| 默认 | 151 | 110 | 27% |
| 仅 Lua 插件 | 125 | 92 | 26% |
| 无插件 | 121 | 89 | 26% |

符号表不影响 RSS，但直接影响镜像体积与烧写时间。

## 三、批次 B：改造后 release

构建配置为 strip + thin LTO + 单代码生成单元；运行环境为 2 核、`MALLOC_ARENA_MAX=1`。表中“健康请求后”列为请求 `/health` 之后的 `VmRSS`/`VmHWM`。

| 档位 | 二进制 | RSS | 健康请求后 RSS/HWM | 匿名 | 文件映射 | VmSize | 线程 |
|---|---:|---:|---:|---:|---:|---:|---:|
| 默认 | 85 | 29.3 | 32.2 | 6.0 | 23.3 | 114.6 | 4 |
| 仅 Lua 插件 | 62 | 24.7 | 25.3 | 5.1 | 19.7 | 88.8 | 3 |
| 无插件 | 59 | 22.4 | 23.1 | 4.9 | 17.5 | 83.3 | 3 |

与批次 A 的同口径对照（均取 2 核、无插件）：

| 指标 | 改造前 | 改造后 | 变化 |
|---|---:|---:|---:|
| 二进制 | 121 MiB | 59 MiB | −51% |
| RSS | 31.8 MiB | 22.4 MiB | −30% |
| VmSize | 308 MiB | 83 MiB | −73% |
| 线程 | 4 | 3 | −1 |

收益拆解（按可归因的主要来源）：

- strip：无插件 121 → 89 MiB，约占镜像总缩减的一半。
- thin LTO 与单代码生成单元：在 strip 基础上进一步减少约 30 MiB 镜像。
- 路由与初始化裁剪叠加编译配置变化：关闭文件检查点、Git/FS、PTY、RAG 管理、后端管理与指标端点后，配合更紧凑的代码布局，空闲 RSS 合计下降约 9 MiB；当前数据未把两者单独分离。
- `MALLOC_ARENA_MAX=1`：把 2 核场景的 VmSize 从 308 MiB 压到 83 MiB，对 RSS 影响很小。

## 四、未覆盖的组合与注意点

- 批次 B 尚未测 32 核、SQLite 存储、显式关闭关键词索引、以及目标 ARM 硬件；表 B 的数据只代表“最小 API 节点 + 2 核 + arena=1”这一档。
- 负载峰值未测。执行历史、并发预算、24 个实体存储各自的缓存（SQLite 模式默认 1000 条/300 秒）、文件快照（单文件默认上限 10 MiB）、RAG 导入与长会话 PTY 输出都会显著推高堆占用。
- `smaps_rollup` 的 `Rss` 略高于 `/proc/status` 的 `VmRSS`，因为两者读取时点不同；文件映射页多数与二进制页缓存共享，`Pss` 明显低于 `Rss`（批次 A 默认档 Pss 约 38.7 MiB 对应 Rss 约 40.5 MiB）。单机独占部署以 RSS 作为上限更保守。
- 嵌入式的 VmSize 目标主要受线程数与 glibc arena 影响；musl 目标通常没有 arena 预留问题，不必设置 `MALLOC_ARENA_MAX`。

## 五、复测方式

在 devs 上复用现有脚本即可复现表中数据：

```bash
# 默认档位（32 核基线）
bash /data/kkkqkx/wf-agent/measure.sh <binary> <label> <port>

# 嵌入式口径（2 核 + arena 限制）
MALLOC_ARENA_MAX=1 taskset -c 0,1 \
  bash /data/kkkqkx/wf-agent/measure.sh <binary> <label> <port>
```

复测时记录：二进制字节数、`VmRSS`/`VmHWM`、`RssAnon`/`RssFile`、`VmSize`、`Threads`、按映射聚合的 `smaps` 前若干项，以及 `/health` 的响应状态。若新增功能改变了空闲 RSS 或线程数，先确认是否引入了新的后台任务或缓存，再决定是否调整档位配置。

相关文档：改造前分析见 `docs/dev/server依赖与内存占用分析.md`，设计边界见 `docs/archive/perf/wf-server-embedded-memory-design.md`，修改清单与实施状态见 `docs/plan/wf-server-embedded-trim-plan.md`。
