# wf-plugin 条件编译特性化改造设计方案

> 目的：把 `crates/infra/wf-plugin` 的三条后端（lua / native / wasm）从「事实上无法裁剪」改造为「可独立排除、可按默认集编入」，并把该语义沿 `wf-runtime`、`wf-server` 逐层透传到最终二进制。
> 约定：本文遵循 AGENTS.md——中文文档、以自然语言描述为主、避免完整代码段；事实声明均附 `file:line` 引用。

---

## 一、结论

需要改造，且属于「当前配置自相矛盾」级别的问题，不是可选优化。

`wf-plugin` 声明了 `lua` / `native` / `wasm` 三个后端特性，但**当前只有开启 `wasm` 才能编译通过**。也就是说 wasmtime 事实上是无条件参与编译的，另外两条后端反倒是「附带」存在。三条后端名义上可裁剪、实际上一条都裁不掉，这正是「完全无法排除特定模块」的成因。

同时，`wf-plugin` 与 `wf-runtime` 的 `default` 都为空集，导致「默认构建」既不含 wasm，也不含 lua，更不含 native——`default` 这一档位目前完全没有承载任何产品集合。

改造方向：让每一条后端特性都能单独成立（任意子集均可编译），并把「产品真正需要的模块集合」写进 `default`，使默认构建可用、裁剪构建可达。

---

## 二、现状实测

### 2.1 特性开关的声明位置

三个 crate 各自持有一张特性表：

| crate | 位置 | 现状 |
|---|---|---|
| `wf-plugin` | `crates/infra/wf-plugin/Cargo.toml:45-49` | `default = []`；`lua`、`native`、`wasm` 各自绑定本后端所需的依赖 |
| `wf-runtime` | `crates/app/wf-runtime/Cargo.toml:41-46` | `default = []`；`plugins = ["wf-plugin/lua"]`；另有 `lua` / `native` / `wasm` 三个透传特性 |
| `wf-server` | `crates/app/wf-server/Cargo.toml:21` | 依赖 `wf-runtime` 时未关闭默认特性，但 `wf-runtime` 的 `default` 为空，等价于不带任何插件能力 |

后端模块在 `crates/infra/wf-plugin/src/lib.rs:31-38` 按特性挂载：`lua`、`native`、`wasm` 三个模块声明各自被 `#[cfg(feature = "...")]` 包裹。

### 2.2 wasm 被无条件编译的真实原因

关键不在于依赖声明，而在于**一个纯清单辅助函数被安放在 wasm 特性门控的模块内，却在中立模块里被无条件调用**：

- 函数定义在 `crates/infra/wf-plugin/src/wasm/loader.rs:91-99`，名为 `all_entry_points`，作用是把清单的默认入口与全部版本化入口合成一个列表。它不触碰 wasmtime、不读 wasm 二进制，与后端实现毫无关系。
- 但它被以下位置无条件引用：
  - `crates/infra/wf-plugin/src/engine/loader.rs:32`（发现后装载）
  - `crates/infra/wf-plugin/src/engine/loader.rs:101`（按路径装载单个插件）
  - `crates/infra/wf-plugin/src/engine/lifecycle.rs:317`（重载插件）
  - `crates/infra/wf-plugin/src/engine/lifecycle.rs:370`（刷新扫描）
  - `crates/infra/wf-plugin/src/package.rs:159`（安装前产物验签）
  - `crates/infra/wf-plugin/src/package.rs:186`（已安装产物复验）
- 而 `crate::wasm` 整个模块在未开启 `wasm` 特性时不存在（`crates/infra/wf-plugin/src/lib.rs:37-38`），因此上述六处引用在编译期直接报「找不到 `wasm`」。

实测编译矩阵（`cargo check -p wf-plugin`）：

| 特性组合 | 结果 |
|---|---|
| 无特性（等价 `--no-default-features`） | 失败，6 处 `E0433` |
| 仅 `lua` | 失败，6 处 `E0433` |
| 仅 `native` | 失败，6 处 `E0433` |
| 仅 `wasm` | 通过 |

即：**wasm 特性是编译通过的前提条件**。wasmtime 因此被无条件拉入编译单元，所谓「wasm 是可选后端」只停留在清单层，未落到编译层。

值得单独指出的是 `package.rs` 的两处引用（`crates/infra/wf-plugin/src/package.rs:159`、`:186`）：它们对**所有后端**的产物做验签，与 wasm 无关，却因为函数住在 wasm 模块里而被一并拖进 wasm 特性依赖。这是典型的「按放置位置而非按职责切分模块」造成的耦合。

### 2.3 下游开关的连带失效

`wf-runtime` 的 `plugins` 特性只开启 `wf-plugin/lua`（`crates/app/wf-runtime/Cargo.toml:43`）。按 2.2 的实测，`wf-plugin` 在只有 `lua` 特性时编译失败，因此：

- `cargo check -p wf-runtime --features plugins` **编译失败**（同为 6 处 `E0433`）。
- `wf-runtime` 的 `plugins` / `lua` / `native` / `wasm` 四个特性目前**没有一个可用**。

`wf-runtime` 侧代码对 `plugins` 特性做了完整的两分支适配（`crates/app/wf-runtime/src/lib.rs:10-14`、`crates/app/wf-runtime/src/bootstrap/inner.rs:200-213`、`crates/app/wf-runtime/src/bootstrap/plugin.rs:5-12`），并且刻意保留了「无插件引擎时」的直接装配回退路径（`crates/app/wf-runtime/src/bootstrap/plugin.rs:19-69`）。也就是说适配逻辑齐备，只差一个能编译通过的特性组合。

### 2.4 依赖归属本身是干净的

按后端拆分的依赖声明没有错配：

| 依赖 | 实际使用位置 | 是否只属于单一后端 |
|---|---|---|
| `wasmtime`、`wasmtime-wasi`、`blake3`、`bytes`、`moka` | `src/wasm/**` | 是 |
| `mlua`、`wf-sandbox` | `src/lua/**` | 是 |
| `libloading` | `src/native/**` | 是 |

即依赖声明已经是后端专属的，裁剪后不会留下悬空依赖。问题只在**源码引用**上。

契约层 `wf-plugin-sdk` 的 `wasm.rs`（142 行）与 `native.rs`（50 行）只含类型与常量声明，不引入 wasmtime / libloading，保持无条件编译是正确的：它们是插件作者侧的契约视图，按后端裁剪反而会让作者侧 API 随宿主构建漂移。

### 2.5 运行期与编译期是两件事

`wf-plugin` 的 `PluginSystemConfig` 默认 `lua_enabled = true`、`native_enabled = false`、`wasm_enabled = true`（`crates/infra/wf-plugin/src/engine/config.rs:47-64`），`wf-runtime` 的 `PluginConfig` 默认值与之对齐（`crates/app/wf-runtime/src/bootstrap/config.rs:146-159`）。这是**运行期策略**——宿主主动关掉某个后端。

而「本次构建是否含该后端」是**编译期能力**。当前代码把两者混在一起：运行期闸门（`crates/infra/wf-plugin/src/engine/loader.rs:270-290`）只检查策略开关，装载分派（`crates/infra/wf-plugin/src/engine/loader.rs:233-263`）才用 `#[cfg(not(feature = "..."))]` 兜底「未编入」。两者都存在且语义正交，这个分层本身是对的，问题只在于兜底分支永远不可达（因为 wasm 恒在）。

---

## 三、缺陷清单

| 编号 | 缺陷 | 影响 | 严重度 |
|---|---|---|---|
| D1 | `all_entry_points` 定义在 wasm 门控模块内、被中立模块无条件引用 | `wf-plugin` 除 `wasm` 外任何组合均编译失败；wasmtime 无法排除 | 阻断 |
| D2 | `wf-runtime` 的 `plugins` / `lua` / `native` / `wasm` 四个特性全部指向不可编译的组合 | 插件子系统在运行时装配层完全不可用 | 阻断 |
| D3 | `wf-plugin` 与 `wf-runtime` 的 `default` 均为空集 | 没有「默认构建」这一档；`default` 无法承载产品集合 | 高 |
| D4 | 后端分派处三条 `#[cfg(not(feature = "..."))]` 分支各自手写错误文案 | 同一语义三份文案，易漂移；且因 D1 永远不可达 | 中 |
| D5 | 「遍历入口点并对 `.wasm` 产物验签」在四处重复实现 | 同一逻辑四份，改动易漏改 | 中 |
| D6 | `package.rs` 的通用产物验签依赖 wasm 模块提供的辅助函数 | 后端无关逻辑被绑到 wasm 特性上 | 中 |

---

## 四、目标特性矩阵

### 4.1 语义约定

三条轴互相独立，不可混淆：

- **编译期能力**：本次构建是否编入了某个后端。由 crate 特性决定。
- **运行期策略**：宿主是否允许某个后端装载插件。由 `PluginSystemConfig` / `PluginConfig` 决定。
- **默认集**：产品标准构建应当包含的模块集合。由 `default` 特性承载。

有效能力 = 编译期能力 ∧ 运行期策略。两者都不满足时，后端不可用，但两种原因必须给出可区分的报错：一个是「本次构建没有编入该后端，请用对应特性重新构建」，一个是「宿主策略关闭了该后端」。

命名约定：后端特性一律使用后端自身名称（`lua` / `native` / `wasm`），不引入 `wasm-only`、`no-wasm` 之类的否定式开关——Cargo 特性只可加不可减，否定式开关无法真正裁剪依赖。

### 4.2 `wf-plugin`

| 特性 | 含义 | 展开为 |
|---|---|---|
| `default` | 产品默认后端集合 | `lua`、`native`、`wasm` |
| `lua` | Lua 后端 | 启用 `mlua` 依赖，并开启 `wf-sandbox` 的 `lua` 特性 |
| `native` | 原生后端 | 启用 `libloading` 依赖 |
| `wasm` | Wasm 后端 | 启用 `wasmtime`、`wasmtime-wasi`、`blake3`、`bytes`、`moka` 依赖 |

改造后的性质：

- 任意子集（八种组合）均可编译通过。
- 不带任何后端特性时，产物是一个「引擎骨架」：清单解析、依赖求解、事件总线、贡献注册、包管理、验签全部可用，装载任何后端插件一律返回「未编入」。
- `default` 覆盖三条后端，标准构建与今天的 `wasm` 单后端构建能力等价（今天只有 wasm 一条真正生效），但语义变为「三条后端齐备」而非「意外只能有 wasm」。

### 4.3 `wf-runtime`

`wf-plugin` 依赖需要显式关闭默认特性，否则下游无法裁剪后端。特性表调整为：

| 特性 | 含义 | 展开为 |
|---|---|---|
| `default` | 产品默认集合 | `plugins`、`lua`、`native`、`wasm` |
| `plugins` | 插件引擎与桥接装配 | 启用可选依赖 `wf-plugin`（不带其默认特性） |
| `lua` | Lua 后端可用 | 开启 `wf-sandbox` 的 `lua` 特性，并转发 `wf-plugin/lua` |
| `native` | 原生后端可用 | 转发 `wf-plugin/native` |
| `wasm` | Wasm 后端可用 | 转发 `wf-plugin/wasm` |

这样 `wf-runtime` 具备与 `wf-plugin` 对称的三档后端语义，且运行期装配层（`bootstrap/plugin.rs`、`plugin_bridge.rs`、`resource_assembler_adapter.rs`）无需任何改动即可跟随。

需要留意的一点：只开 `plugins` 而不开任何后端特性时，得到的是「引擎在、但没有可装载的后端」。这不是缺陷，而是让「按后端裁剪」得以表达的必要代价；`default` 已经保证标准构建不会出现这种组合。

### 4.4 `wf-server`

`wf-server` 是 `wf-runtime` 的唯一消费者，也是最终二进制的所在层。裁剪能力必须在此层闭合，否则 `--no-default-features` 传到上游会被默认特性重新加回来。因此 `wf-server` 对 `wf-runtime` 的依赖需关闭默认特性，并由 `wf-server` 自己的特性表逐层转发：

| 特性 | 含义 | 展开为 |
|---|---|---|
| `default` | 产品默认集合 | `plugins`、`lua`、`native`、`wasm` |
| `plugins` | 插件子系统 | 转发 `wf-runtime/plugins` |
| `lua` / `native` / `wasm` | 后端可用 | 转发 `wf-runtime` 的同名特性 |

转发链依赖 Cargo 的两条确定语义（已实测确认）：

- 可选依赖上的 `dep:名称` 只负责「显式启用依赖」，不会带上该依赖的默认特性；
- `名称/子特性` 形式会连带启用该可选依赖本身。

因此「`plugins` 单独开启」得到的是「引擎在、无后端」，「`plugins` 加任一后端特性」得到的是「引擎在、含该后端」，行为完全可预期。

---

## 五、改造任务

### 5.1 入口点枚举归位到中立模块

把 `all_entry_points` 从 `crates/infra/wf-plugin/src/wasm/loader.rs` 移到与后端无关的位置。清单是入口点的唯一来源，该函数应与清单结构同处一个模块（`crates/infra/wf-plugin/src/manifest.rs`），由清单模块对外提供。

调用方改为指向新位置：`engine/loader.rs` 两处、`engine/lifecycle.rs` 两处、`package.rs` 两处。wasm 后端内部若有自用（`crates/infra/wf-plugin/src/wasm/loader.rs:427` 的用例），改为引用新位置。

这一步单独完成即可解除 D1：wasm 特性不再是编译前提。

### 5.2 后端分派与缺失后端的统一报错

装载分派（`crates/infra/wf-plugin/src/engine/loader.rs:233-263`）保留现有的 `#[cfg]` 分臂结构——`PluginType` 是清单层的三值分类（`crates/foundation/wf-plugin-sdk/src/manifest.rs:7-11`），枚举变体恒定存在，分派必须穷尽，因此编译期缺臂与运行期缺臂都要留。

改动只在报错构造：三条 `#[cfg(not(feature = "..."))]` 分支不再各自手写文案，改为调用同一个构造入口，产出结构一致、可被上层程序化识别的错误（后端种类 + 缺失的特性名）。错误类型定义在契约层 `crates/foundation/wf-plugin-sdk/src/error.rs`，与既有的 `LuaError` / `NativeError` / `WasmError` 同一形状。

同时补一条「后端 → 特性名」的映射，作为该构造入口的唯一事实来源，避免文案里出现与 Cargo.toml 不一致的特性名。

运行期闸门（`check_backend_gate`）保持原样：它表达的是宿主策略，与编译期缺臂是两回事，错误形态必须能让运维一眼分辨「策略关的」与「没编进来的」。

### 5.3 合并重复的 wasm 产物验签逻辑

四处「遍历入口点 → 筛出 `.wasm` → 逐个验签」的代码（`engine/loader.rs:32`、`engine/loader.rs:101`、`engine/lifecycle.rs:317`、`engine/lifecycle.rs:370`）逻辑完全相同，仅错误处理方式不同（前三处向上抛，第四处记日志并跳过该插件）。

做法：在 `crates/infra/wf-plugin/src/engine.rs` 中，紧邻既有的 `verify_wasm_signature` 增加一个「按清单批量验签 wasm 产物」的引擎方法，四个调用点统一改为调用它。第四处（刷新扫描）把该方法返回的错误转成日志并跳过，语义与现状一致。

该方法不依赖 wasm 运行时——验签是纯文件哈希与签名校验（`sha2` / `ed25519-dalek` 均为无条件依赖），因此在未编入 wasm 后端的构建中依然可用，签名校验不会因为裁剪而失效。

### 5.4 特性表改写

按 4.2 / 4.3 / 4.4 改写三张特性表。要点：

- `wf-plugin` 的 `default` 填入三条后端；
- `wf-runtime` 对 `wf-plugin` 的依赖加 `default-features = false`，并在自身特性表中显式转发三条后端；
- `wf-server` 对 `wf-runtime` 的依赖加 `default-features = false`，并在自身特性表中显式转发；
- 三层的 `default` 都表达同一个产品集合，语义一致、逐层可读。

### 5.5 开发期依赖的说明

`wf-plugin` 的四个开发期依赖（`wat`、`wit-component`、`wit-parser`、`wasmparser`）只被 wasm 模块内的用例使用（`crates/infra/wf-plugin/src/wasm/pool.rs:545`、`crates/infra/wf-plugin/src/wasm/loader.rs:475`、`crates/infra/wf-plugin/src/wasm/component.rs:1189`、`crates/infra/wf-plugin/src/wasm/component.rs:1340`）。Cargo 不支持按特性裁剪开发期依赖——在目标表的 `cfg` 中写 `feature = ...` 会被显式警告为「不支持且不会按预期工作」——因此保持无条件声明。

代价仅限于「裁剪构建下跑 `cargo test`」时会多编译一次这几个开发期依赖，不进入产物二进制。可接受的替代方案是把 wasm 用例夹具独立成单独 crate，但那会违背「不新建替代模块」的约束，收益又仅限测试期编译时间，故不做。

---

## 六、验证矩阵

改造后必须全部通过，且尽量合并为少数几次编译。

`wf-plugin` 特性组合（每一格都必须编译通过）：

| 组合 | 检查点 |
|---|---|
| 无特性 | 引擎骨架可编译；装载任意后端插件返回「未编入」错误 |
| 仅 `lua` | Lua 后端可编译可装载；wasm 插件返回「未编入」 |
| 仅 `native` | 原生后端可编译可装载；其余返回「未编入」 |
| 仅 `wasm` | Wasm 后端可编译可装载；其余返回「未编入」 |
| `lua` + `native` | 两条后端同时可用 |
| `lua` + `wasm` | 两条后端同时可用 |
| `native` + `wasm` | 两条后端同时可用 |
| `default`（三条齐备） | 与今日「仅 wasm 可编译」的能力面一致，但语义为三后端齐备 |

`wf-runtime` 特性组合：

| 组合 | 期望 |
|---|---|
| 无特性 | 走「无插件引擎」直装配回退路径 |
| `plugins` | 引擎在、无后端 |
| `plugins` + `lua` | 引擎在、仅 Lua 可装载 |
| `plugins` + `wasm` | 引擎在、仅 Wasm 可装载 |
| `default` | 引擎在、三后端齐备 |

`wf-server`：

| 组合 | 期望 |
|---|---|
| `default` | 标准服务端，插件子系统齐备 |
| `--no-default-features --features "plugins,lua"` | 服务端仅带 Lua 后端 |
| `--no-default-features` | 服务端不带插件子系统，运行期走回退路径 |

执行方式：先跑一次 `cargo clippy --all-targets --all-features` 作为总检查，再针对上述组合各跑一次 `cargo check`（可用同一命令配合不同 `--no-default-features` / `--features` 参数）。改动集中在 `wf-plugin` 的特性与引用点，不需要新写大量用例；只需补一条针对「未编入后端的装载报错」的用例，以及一条针对入口点枚举归位后仍覆盖全部版本化入口的用例。

---

## 七、边界与非目标

- **不引入运行时后端探测**。编译期是否编入是构建事实，运行时探测只会带来「有时有有时没有」的行为分叉。当前设计已让两种不可用原因各自有确定且可区分的报错。
- **不改动 `PluginType` 枚举形状**。它是清单层分类，与宿主构建能力正交；按特性裁剪枚举变体会让契约层与插件作者侧随宿主构建漂移。
- **不改动 `wf-plugin-sdk` 的模块可见性**。契约层保持无条件编译。
- **不改动后端的运行期策略默认值**。`native_enabled = false` 保持不变；编译期编入原生后端不等于运行期允许装载原生插件。
- **不做向后兼容**。特性表改写后，旧的无特性构建路径直接消失，不保留双路径。

---

## 八、风险与待确认

1. **`default` 纳入三条后端会显著增加标准构建成本**。wasmtime 加 cranelift 是本仓最重的依赖之一。方案把「默认包含」与「可否排除」同时满足：默认齐备，裁剪可达。若后续认为标准构建不应默认携带 wasmtime，则应把 `wasm` 从 `wf-plugin` 的 `default` 中移出，改由 `wf-runtime` / `wf-server` 的 `default` 显式补上——两者语义等价，区别只在默认集落在哪一层。
2. **`wf-server` 关闭 `wf-runtime` 默认特性后，`wf-runtime` 的 `default` 只在被直接依赖时生效**。需要在 `wf-server` 的特性表中把完整集合显式写出，避免两处默认值漂移。
3. **插件引擎进入服务端默认构建是行为变化**。今日 `wf-server` 不带插件子系统，走的是直装配回退路径；改造后默认走引擎装配路径（`crates/app/wf-runtime/src/bootstrap/plugin.rs:118-149`）。两条路径在产物安装语义上一致，但需要确认「启动时扫描 `./plugins` 目录并记录装载失败日志」这一行为符合服务端预期。
4. **`plugins` 单独开启得到无后端引擎**属于有意保留的组合，不应被误用为「完整插件支持」的构建指令。需要在 `wf-runtime` 与 `wf-server` 的特性表注释中写明这一点。
