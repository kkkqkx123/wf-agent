# 插件版本管理设计方案 — 对齐 Terraform Plugin Protocol

> 结论速览：
>
> 1. 引入 Terraform 的 major/minor 版本分层策略，将协议版本编码进 WIT world 名称，编译期即可发现不一致。
> 2. 增加运行时双向 handshake 协商机制，替代当前宿主单方面判定 sdk_version 的模式。
> 3. 支持单进程多 major 版本共存，为插件升级和回滚提供基础。
> 4. 版本约束全局聚合，启动阶段即检测跨插件冲突。
> 5. 错误信息包含修复建议，降低插件开发者排查成本。

## 一、现状回顾

当前插件系统采用三层版本模型：

- `PluginManifest.version`（semver）：插件身份标识 + 依赖约束。
- `PluginManifest.sdk_version`（semver VersionReq）：宿主 API 兼容性门控，由宿主单方面判定。
- ABI 常量（u32 = 1）：二进制接口破坏性变更门控，native/wasm 各自独立。

核心问题：

- 宿主单方面判定 sdk_version，插件无法表达自身支持的宿主版本范围。
- WIT world 版本与 ABI 常量是两条独立版本轴，容易不一致。
- 同一插件无法同时支持新旧两代宿主协议。
- 无全局约束聚合，跨插件版本冲突只能在运行期逐个暴露。
- 错误信息缺乏修复建议。

## 二、Terraform Plugin Protocol 核心设计

### 2.1 版本分层策略

Terraform 将协议版本分为两层：

- **Major 版本**：编码进 protobuf 包名（`tfplugin5`、`tfplugin6`）。任何导致不兼容的变更必须创建新包名。一个 provider 进程可同时导出多个 major 版本的 gRPC 服务。
- **Minor 版本**：不体现在线上协议中，仅作为人类沟通工具。兼容性通过 feature detection（可选字段 + 默认行为）实现。

这种设计的优势：编译期即可发现不兼容（包名不同），运行时可选择双方都支持的 major 版本。

### 2.2 运行时 Handshake 协商

Terraform Core 与 provider 的启动流程：

1. 宿主启动插件子进程，监听其 stdout。
2. 插件进程在 stdout 输出一行 handshake JSON，包含服务端口和协议版本列表。
3. 宿主连接 gRPC 端口，发送 handshake 请求，包含自身支持的协议 major 版本列表。
4. 插件选择双方都支持的最高 major 版本，返回确认。
5. 后续所有 gRPC 调用使用协商确定的协议版本。

核心理念：双方都声明支持范围，运行时选择交集，而非一方单独判定。

### 2.3 版本约束语法

Terraform 使用 `hashicorp/go-version` 解析 HCL 中的 `version` 属性：

- 支持 `~> 2.0`（ pessimistic constraint，等价于 `>= 2.0, < 3.0`）。
- 支持逗号分隔的多约束组合。
- 错误信息包含具体建议的兼容版本号。

### 2.4 多版本共存

一个 Terraform provider 二进制可同时注册 `tfplugin5` 和 `tfplugin6` 两个 gRPC 服务。宿主根据协商结果连接到对应服务。这使得旧版宿主和新版宿主可以同时使用同一个 provider 二进制，无需维护多个构建产物。

## 三、适配方案

### 3.1 协议版本编码进 WIT World 名称

当前设计：WIT world 为 `wf:plugin@0.1.0-draft`，ABI 常量为 `1`，两者独立演进。

改为：WIT world 名称包含 major 版本号，破坏性变更必须创建新 world。

具体做法：

- 将现有 `wf:plugin@0.1.0-draft` 重命名为 `wf:plugin@1`（语义化 major 版本）。
- 破坏性接口变更（删除字段、修改函数签名等）创建 `wf:plugin@2` world。
- ABI 常量与 WIT major 版本一一对应：`WF_ABI_VERSION_V1 = 1` 对应 `wf:plugin@1`，`WF_ABI_VERSION_V2 = 2` 对应 `wf:plugin@2`。
- 当前仅有一个 major 版本，因此这一步主要是建立约束，为未来演进提供框架。

这种编码方式保证：编译 WIT 绑定时即可发现版本不匹配，无需运行时检查。

### 3.2 运行时 Handshake 协商

当前设计：宿主在 `check_sdk_version()` 中单方面判定插件声明的 `sdk_version` 是否匹配宿主版本。

改为：加载时双方交换支持的协议版本范围，选择交集。

具体做法：

- Wasm 后端：在现有 `wf_abi_version()` 之外，增加 `wf_supported_abi_versions() -> Vec<u32>` 导出。插件返回自身支持的所有 major ABI 版本。宿主选择双方都支持的最高版本。未导出该函数的插件假定为仅支持 v1（向后兼容）。
- Native 后端：在现有 `wf_plugin_abi_version()` 之外，增加 `wf_plugin_supported_abi_versions() -> Vec<u32>` 导出，语义相同。
- Lua 后端：无 ABI 协商需求，保持现有 sdk_version 检查不变。
- 协商失败时（无交集），给出明确错误信息，包含双方支持的版本范围和修复建议。

协商流程：

1. 宿主读取插件 manifest，执行现有的 `validate_manifest()` 和 `check_permissions()`。
2. 宿主加载插件模块（wasmtime 实例化 / dlopen / lua require）。
3. 宿主调用 `wf_supported_abi_versions()` 获取插件支持的版本列表。
4. 宿主与插件取交集，选择最高共同版本。
5. 交集为空则加载失败，输出建议。
6. 协商成功则后续所有调用使用确定的协议版本。

### 3.3 单进程多 Major 版本共存

当前设计：一个插件目录对应一份插件，同 id 重复安装被拒绝。

改为：允许一个插件目录包含多份后端产物，宿主根据协商结果选择加载哪个。

具体做法：

- 插件目录支持多个 `.wasm` 文件，命名约定为 `plugin_v1.wasm`、`plugin_v2.wasm`。
- manifest 中 `entry_point` 字段扩展为支持按版本声明入口：
  - `entry_point` 保持现有语义（单版本插件的入口）。
  - 新增可选字段 `versioned_entry_points: HashMap<u32, String>`，key 为 ABI major 版本。
- 宿主协商确定协议版本后，优先查找 `versioned_entry_points` 中对应版本的入口，找不到则回退到 `entry_point`（假设为 v1）。
- Native 后端：支持单个 `.so`/`.dll` 通过 `wf_plugin_supported_abi_versions()` 声明多版本支持，无需多文件。
- 包管理：`InstalledPlugin` 保持单条记录，`install()` 仍以 id 为唯一键，但 manifest 内部的 `versioned_entry_points` 允许多版本共存。

这一步为未来的协议升级路径打基础：插件作者可以在一个目录中同时发布兼容新旧宿主的两个 wasm 文件，宿主自动选择。

### 3.4 版本约束全局聚合

当前设计：`resolve_dependencies()` 独立校验每个插件的依赖声明，不检测跨插件的全局冲突。

改为：在 `resolve_dependencies()` 中聚合所有插件的版本约束，检测全局冲突。

具体做法：

- 构建全局约束表：`HashMap<插件id, Vec<VersionReq>>`，收集每个插件声明的所有约束。
- 检测冲突：若同一插件 id 的多个约束的交集为空，报告冲突详情。
- 错误信息格式：插件 A 要求 `plugin-x@^1.0`，插件 B 要求 `plugin-x@^2.0`，无交集。
- `strict_dependency_versions=true`（默认）时冲突导致初始化失败；为 false 时警告并继续。

这一步参考 Terraform 的 `AddProviderRequirements` 递归聚合思路：所有子模块的约束汇入根模块统一检测。

### 3.5 错误信息增强

当前错误信息示例：`sdk version '>=2.0.0' not satisfied by host '1.2.0'`。

改为参考 Terraform 的模式，包含修复建议：

- 版本不匹配：说明双方版本范围，建议升级插件或宿主。
- Handshake 协商失败：列出双方支持的版本范围，建议更新插件或降级宿主。
- 全局约束冲突：列出冲突的约束来源，建议升级或降级冲突插件。
- 多版本协商成功但版本低于最新：警告提示插件可能有更新版本可用。

### 3.6 SDK Version 语义调整

当前设计：`sdk_version` 是宿主整体版本（CARGO_PKG_VERSION）的精确匹配。

调整：保留 `sdk_version` 作为向后兼容的声明式检查，但将 handshake 协商作为主要兼容性判定。

具体做法：

- 新插件应使用 handshake 协商（导出 `wf_supported_abi_versions()`）。
- 旧插件未导出协商函数时，回退到现有 `sdk_version` 检查。
- `reject_invalid_sdk_version` 标志继续控制未解析 sdk_version 的处理策略。
- 长期方向：sdk_version 声明作为元数据保留，不再作为加载门控；兼容性完全由 handshake 协商决定。

## 四、实现路径

按优先级分阶段推进，每阶段独立可验证：

### 阶段一：协议版本编码进 WIT World + ABI 常量对齐

- 将 `wf:plugin@0.1.0-draft` 重命名为 `wf:plugin@1`。
- 将 `WF_PLUGIN_ABI_VERSION` / `WF_WASM_ABI_VERSION` 重命名为 `WF_ABI_VERSION_V1`，值保持 1。
- 更新所有引用 WIT world 名称和 ABI 常量的代码。
- 验证：cargo clippy 通过，现有插件加载正常。

### 阶段二：Wasm/Native Handshake 协商

- 在 `wf-plugin-sdk/src/wasm.rs` 增加 `SUPPORTED_ABI_VERSIONS` 导出名常量。
- 在 `wf-plugin-sdk/src/native.rs` 增加对应常量。
- 在 wasm loader 的 `negotiate_abi_version()` 中实现版本列表协商逻辑。
- 在 native loader 的 `check_abi_version()` 中实现版本列表协商逻辑。
- 未导出协商函数的插件假定为仅支持 v1。
- 验证：编写多版本 wasm 插件测试，确认协商正确选择和失败处理。

### 阶段三：版本约束全局聚合

- 在 `resolve_dependencies()` 中增加全局约束表构建和冲突检测。
- 冲突信息包含冲突来源插件和各自的约束声明。
- 验证：构造冲突场景测试，确认错误信息正确。

### 阶段四：多版本入口支持

- `PluginManifest` 增加 `versioned_entry_points` 字段。
- wasm loader 支持按协商版本选择入口文件。
- 包管理无需改动（单条记录不变）。
- 验证：单目录多 wasm 文件场景测试。

### 阶段五：错误信息增强 + sdk_version 语义调整

- 统一错误信息格式，增加修复建议。
- 更新 `check_sdk_version()` 为兼容保留路径，新增协商路径为主路径。
- 验证：错误信息可读性审查，旧插件兼容性测试。

## 五、与现有设计的兼容性

- 现有插件（不导出 `wf_supported_abi_versions()`）假定为 v1，继续正常工作。
- `sdk_version` 字段继续有效，作为旧插件的兼容性检查手段。
- `version` 字段语义不变，继续用于依赖约束和安装注册表。
- 包管理 API 不变，`install()` 仍以 id 为唯一键。
- 现有测试全部通过，不破坏已有行为。

## 六、关键决策记录

- 为什么选择 Terraform 而非 LSP 作为主要参考：两者架构最相似（宿主 + 独立插件 + 进程间通信 + 多后端），且 Terraform 的版本协商策略已被大规模生产验证。
- 为什么 WIT world 编码 major 版本而非 minor：Terraform 经验表明 major 版本才是兼容性分界点，minor 差异通过 feature detection 处理。
- 为什么保留 sdk_version 作为兼容路径：避免一次性破坏所有现有插件，渐进迁移。
- 为什么多版本入口只对 wasm 实现多文件：native 单文件通过导出多版本服务实现，lua 无 ABI 概念。
