# 远端部署写路径（第二轮）：远端下载优先、源目录分发、全量卸载

本文是 `docs/plan/remote-deploy-write-path.md` 的**替换版**。第一轮确立了 Zed / VS Code 式用户态安装与六个动词；本轮按新的产品决策调整产物来源、文件传递、卸载语义与验证方式，并已同步落地代码。所有现状判断均附 `文件:行号`。

## 0. 结论

- **不维护远端 `desiredState`**。管理面只保存"最近一次探测 / 最近一次执行结果"的内存缓存（`crates/app/wf-server/src/backends/manager.rs:1586`），没有期望状态字段、没有收敛循环、没有失败自愈。`probe` 是独立的验证动词：一次 SSH 往返输出平台、安装布局、进程与关键检查项（`crates/app/wf-server/src/backends/deploy.rs:907`）。
- **产物默认远端下载**。`ArtifactSpec.kind` 缺省为 `release`（`deploy.rs:1027,1019`），远端按自身 `uname` 拉取 `<base>/wf-agent-server-<version>-<os>-<arch>.tar.gz`（`deploy.rs:1011`），与 `deploy/scripts/fetch-package.sh:9-16` 的包名契约一致。本地上传退为显式 `--artifact`（`crates/app/cli/wf-cli-shared/src/args_remote.rs:167`）。
- **文件传递改为"源目录 + 范围"**。`DeployRequest` 删除自由 `files` 列表，新增 `scope: none|config|full` 与 `source`（`deploy.rs:215-231`）。服务端从本机 `deploy_source_root` 下的目录读 `server/*.toml`，`full` 再加 `scripts/*`（`deploy.rs:1327`）。`config` / `full` 是仅有的两种范围，更细的修改走 `ssh` 直连。
- **卸载是全量清理**。`uninstall` 固定为"停止 + 删除整个安装根目录"，没有 `purge` 开关（`deploy.rs:1617`）。配置、脚本、日志、pid、`install.json` 与二进制一起消失。
- **`upgrade` 保留**；部署路径**不引入 `--config`**（项目层）。远端启动垫片只传 `--settings-dir`（`deploy.rs:1536`）；本地单机安装脚本仅在运维显式设置 `PROJECT_ROOT` 时才渲染 `--config`，为空时删掉该参数（`deploy/scripts/install-server.sh:210-214`），本轮不改动这个既有可选语义。项目层意味着某一工作区的 `.wf/` 覆盖，会把服务实例绑死到单一目录，因此不应成为远端服务的默认启动参数。

## 1. 本轮决策与代码事实

| 决策 | 上一版 | 本轮 | 代码依据 |
| ---- | ------ | ---- | -------- |
| 远端状态 | 内存缓存结果，无 desiredState | 保持；探测承担验证 | `manager.rs:1586`；`deploy.rs:806,907` |
| 默认产物 | 本地上传自身可执行文件 | 远端下载 release | `deploy.rs:1033`；`args_remote.rs:171` |
| 本地产物 | 默认 | 显式 `--artifact`，默认 `local` | `deploy.rs:1084`；`cmd/remote.rs:917-921` |
| 文件载荷 | 请求体携带 `files` 文本 | 服务端读源目录，`scope` 选择 | `deploy.rs:215,1313` |
| 传输粒度 | 任意 `server/`、`scripts/` 路径 | `none`/`config`/`full` | `deploy.rs:177-203` |
| 卸载 | 默认保留配置与日志，`--purge` 全删 | 恒全删，无开关 | `deploy.rs:1617` |
| 启动参数 | 不带 `--config` | 远端不带；本地仅显式 `PROJECT_ROOT` 时保留既有可选语义 | `deploy.rs:1536`；`install-server.sh:210-214` |

## 2. probe：唯一的验证面

`probe` 返回 `verdict` 与 `checks`（`deploy.rs:305-329`）。判定规则（`deploy.rs:806-903`）：

| 判定 | 条件 |
| ---- | ---- |
| `unreachable` | SSH 调用失败、超时或远端脚本报错（`deploy.rs:947-968`） |
| `absent` | 家目录下没有安装根目录 |
| `degraded` | 关键项之一失败：`installed`、`layout`、`running`、`health` |
| `ok` | 四项关键检查全部通过 |

`config`、`scripts`、`version` 三项是**信息项**而不决定判定：只传配置的部署本来就允许没有脚本，运维也可能有意装非管理面版本。远端 `curl` 缺失时，`health` 退化为进程存活，并在 `detail` 里写明"liveness only"，不会伪装成 HTTP 健康。

安装布局检查来自探测脚本新增的一行 `for d in bin current server scripts run logs`（`deploy.rs:702`）；`RemoteInstallation.dirs` 接收它（`deploy.rs:289`）。因此 `probe` 能区分"装了但目录残缺"与"完全没装"。

**为什么不引入 desiredState**：期望状态需要一个写时落库、读时比对的真相源，以及"探测失败 → 重建"的判定阈值。远端服务是人工通过显式动词管理的（人在环上），缓存的探测结果只用于界面渲染；自动重建会在网络抖动时删掉正在服务的目录。下一阶段若要做保活，应作为**独立任务**设计明确的冷却期与人工确认，而不是塞进部署模块。

## 3. 产物策略

`resolve_artifact`（`deploy.rs:1027`）支持四种 `kind`：

| kind | 默认？ | 行为 | 适用 |
| ---- | ------ | ---- | ---- |
| `release` | 是 | 远端 `curl` 拉取发布包，存在同名 `.sha256` 则校验（缺省时仅提示），`tar -xzf` 后取包内 `wf-server` | 跨架构、常规部署 |
| `url` | 否 | 远端下载显式 URL；`.tar.gz` / `.tgz` 后缀自动按归档解包 | 内网镜像、指定包 |
| `remote_path` | 否 | 复制远端已有文件 | 纳入已完成的手工安装 |
| `local` / `local_path` | 否 | 从管理端上传；路径为空取管理面自身可执行文件 | 离线环境、开发中的未打包构建 |

默认基线 `https://packages.example.com/wf-agent` 与 `fetch-package.sh` 的占位保持一致（`deploy.rs:53`；`config.rs:42`）。可用 `deploy_artifact_base_url` 或 `WF_BACKEND_DEPLOY_ARTIFACT_BASE_URL` 覆盖（`config.rs:185,265`）。基线为空时计划返回 `missing_base_url` 并阻塞，提示改用其他产物来源（`deploy.rs:1051-1063`）。

校验层次：

1. `release` 的 URL 由远端平台拼装，避免把管理端架构当成目标架构（`deploy.rs:1011`）。
2. 显式 `sha256` 强制 `sha256sum -c -`；未给时尝试 `<url>.sha256`，缺失仅告警（`deploy.rs:1451-1469`）。
3. 落位前在临时文件上执行 `--version` 并用输出末段作版本目录名（`deploy.rs:1500`），跑不起来就不会换链。
4. 下载类产物要求远端有 `curl`；探测发现缺失时计划直接阻塞并提示改用 `local`/`remote_path`（`deploy.rs:999`），不会在建好目录后才在下载步骤失败。
5. `local` 仍做平台一致性检查，但只在**显式选择**时才会阻塞；不再出现"默认上传自身却在远端 exec format error"。

## 4. 源 deploy 目录与范围

### 4.1 配置

| 配置项 | 默认 | 覆盖方式 | 定义 |
| ------ | ---- | -------- | ---- |
| `deploy_source_root` | `deploy` | `WF_BACKEND_DEPLOY_SOURCE_ROOT` | `config.rs:81,192,268` |
| `deploy_artifact_base_url` | 占位发布基址 | `WF_BACKEND_DEPLOY_ARTIFACT_BASE_URL` | `config.rs:78,185,265` |

源根为管理端本机路径；相对路径按管理面工作目录解析（`deploy.rs:1197`）。请求中的 `source` 只能是源根下的相对目录名，`..`、绝对路径与跑出根外的符号链接均被拒绝（`deploy.rs:1221-1252`）——部署 API 不能变成任意文件读取。

### 4.2 范围语义

| scope | 传输内容 | 约束 |
| ----- | -------- | ---- |
| `none` | 无 | 默认；远端保留既有设置 |
| `config` | `<source>/server/*.toml` → 远端 `server/` | 至少 1 个文件，否则报错（`deploy.rs:1347`） |
| `full` | 上述 + `<source>/scripts/*` → 远端 `scripts/` | 源目录必须完整提供脚本层 |

非递归、只认 `server/*.toml` 与 `scripts/*`；`server/notes.md` 之类不会上传。脚本落地权限 700，配置 600（`deploy.rs:1374`）。脚本**只落盘不执行**——固定动词集不因为携带文件而获得任意命令入口。

`GET .../deploy` 的 `DeployStatusView.sources` 会列出可用源目录及各自文件清单与可用性问题（`api/system/backends.rs:581-589`；`manager.rs:1586`；`deploy.rs:1258`），前端据此渲染下拉框。

### 4.3 初始化脚本

`wf remote deploy init --dir deploy/staging --scope config|full` 在本地生成骨架（`args_remote.rs:198-205`；`cmd/remote.rs:825`）：`config` 生成 `server/server.toml` 与 `server/backends.toml`，`full` 额外生成两个可执行脚本。已存在的文件默认不覆盖，除非 `--force`。生成后由运维编辑填值，再用 `--from staging --scope full` 部署。

**要求源路径完整提供 deploy 功能**：一个可作为 `full` 源的目录必须同时具备 `server/` 与 `scripts/`，否则状态视图给出 `problem`（`deploy.rs:1282-1286`），执行时也会被范围读取拦住。

## 5. 卸载语义

`uninstall` = `stop` + 一次 `rm -rf "$HOME"/<install_root>`（`deploy.rs:1617-1627`）。没有 `purge` 参数，也没有"保留日志"分支：半删除状态（二进制没了但配置还在）对后续安装只有干扰。安装根在 `normalize_install_root` 中已限定为家目录下的相对安全路径（`deploy.rs:642`），`rm -rf` 的目标不可构造出 `/` 或 `$HOME` 本身。

前端在卸载动作下显示全量清理提示，CLI 删除了 `--purge`（`RemoteServersPanel.svelte:326`；`args_remote.rs:159-195`）。

## 6. upgrade 与 `--config`

`upgrade` 的步骤顺序是 `install → stop → start → health`（`deploy.rs:1667-1703` 计划，`deploy.rs:1906-1980` 执行）。先写新版本再停旧进程，保证换链后启动的是新二进制；不停旧进程会"报成功但继续跑旧版本"，这是第一轮自查发现并修复的真实缺陷。

`--config`（项目层）不参与任何启动路径：

- 远端：`run/launch.sh` 只调用 `current/wf-server --addr 127.0.0.1:<port> --settings-dir "<root>"`（`deploy.rs:1536-1542`）。已加单测断言脚本不含 `--config`（`deploy.rs:2264`）。
- 本地：`deploy/scripts/install-server.sh` 在 `PROJECT_ROOT` 为空时从渲染单元里删除 `--config`（`install-server.sh:210-214`），保持无默认工作区时的 fail-loud 语义。显式设置 `PROJECT_ROOT` 时单机安装仍会带 `--config`；那是运维主动绑定某个工作区，与远端服务的多目录语义不同，本轮不改它。
- 理由：项目层代表"某个工作区的 `.wf/` 覆盖"，而远端服务通常同时服务多个工作区；绑定单一项目根会把目录语义写死在服务实例上。需要项目覆盖时，应在请求层携带 directory，而不是在进程启动参数上绑死。

## 7. 三个入口

**HTTP**（均在 `api/system/backends.rs:59-68`，域关闭时 503）：

| 方法与路径 | 变化 |
| ---------- | ---- |
| `POST .../deploy/probe` | 响应新增 `verdict`、`checks` |
| `POST .../deploy/plan` | 请求体改为 `artifact`/`scope`/`source`/`force`；响应新增 `scope`、`source` |
| `POST .../deploy` | 同上；结果依旧附一次新探测 |
| `GET .../deploy` | 响应新增 `sources` |

`DeployStatusView` 在 `api/system/backends.rs:581-589`；`plan`/`apply` 的 400 分类增加了 `unknown deploy scope` 与 `deploy source`（`api/system/backends.rs:634-640,666-672`）。OpenAPI 快照与 `schema.d.ts` 已重生成；重生成需用最小特性集绕开仓库既有的 `mlua 0.12` 编译问题：

```bash
WF_REFRESH_OPENAPI=1 cargo test -p wf-server --no-default-features \
  --features "file-checkpoint,git-fs,pty,rag-vector,backends,metrics" \
  --lib openapi::tests::committed_snapshot_matches_document
cd tools/openapi-codegen && npm run gen
```

**CLI**（`args_remote.rs:159-205`；`cmd/remote.rs:298-335,895-930`）：

```bash
wf remote deploy staging --action probe
wf remote deploy staging --action install --scope full --from staging --apply
wf remote deploy staging --action upgrade --artifact-version 0.2.0 --apply
wf remote deploy staging --action install --artifact ./wf-server --apply   # 显式本地传输
wf remote deploy staging --action uninstall --apply                        # 全量清理
wf remote deploy init --dir deploy/staging --scope full
```

**Web**：`RemoteServersPanel.svelte` 去掉三个配置文本框与 `--purge` 复选框，改为源目录 / 范围 / 产物来源三个选择器；探测结果展示判定与逐项检查；计划展示范围、源、产物与待传文件（`apps/web-app/src/lib/services/backends.ts:76-100,289-325`）。

## 8. 关键 diff

```diff
- pub struct DeployRequest {
-     pub action: String,
-     pub artifact: Option<ArtifactSpec>,
-     pub files: Vec<DeployFile>,
-     pub force: bool,
-     pub purge: bool,
- }
+ pub struct DeployRequest {
+     pub action: String,
+     pub artifact: Option<ArtifactSpec>,
+     pub scope: String,        // none | config | full
+     pub source: Option<String>,
+     pub force: bool,
+ }

- let kind = if spec.kind.trim().is_empty() { "local_path" } else { ... };
+ let kind = if spec.kind.trim().is_empty() { "release" } else { ... };
+ let url = release_artifact_url(base, version, &target.platform);

- let files = validate_files(&request.files)?;
+ let files = read_source_files(settings, request.source.as_deref(), scope)?;

- fn uninstall_steps(layout: &Layout, purge: bool) -> Vec<DeployStepView>
+ fn uninstall_steps(layout: &Layout) -> Vec<DeployStepView>
+ // rm -rf "$HOME"/<install_root>  —— 一次删净
```

## 9. 验证结果

- `cargo check -p wf-server --no-default-features --features backends` 通过，无新增告警。
- `cargo test -p wf-server --no-default-features --features backends --lib backends::`：**83 项全绿**（新增源目录越界、范围筛选、release URL、空基线阻塞、无 `curl` 下载阻塞、显式本地平台校验、卸载全删、启动脚本无 `--config`、probe 判定、范围解析等 14 项）。
- `cargo check -p wf-cli-shared` 通过。
- OpenAPI 快照刷新后一致性测试通过；前端 `svelte-check` **0 error 0 warning**；`scripts/sync-deploy.sh --check` 通过。
- **未做真机联调**（本环境无 SSH 目标）。开发板首轮重点：无 `setsid` 时的 `nohup` 降级、无 `curl` 时 release 包无法下载（需显式 `--artifact-kind local`）、健康等待退化为进程存活、`pkill` 扫尾不误伤同机其他端口、卸载后家目录残留检查。

仓库既有的 `mlua 0.9 → 0.12` 生命周期编译错误（`crates/infra/wf-plugin/src/lua/`）本轮未改；它只影响默认特性集，与本改动无关。

## 10. 与上一版的差异

| 维度 | 上一版 | 本轮 |
| ---- | ------ | ---- |
| 默认产物 | 管理面自身二进制直传 | 远端按平台下载 release |
| 跨平台 | 阻塞，需 `--force` | 正常路径；无需平台校验 |
| 本地传输 | 默认 | 显式 `--artifact` |
| 文件来源 | 请求体文本 / 前端粘贴 | 管理端源目录 + `scope` |
| 精细粒度 | `--file REMOTE=LOCAL` | 不支持，走 ssh 直连 |
| 卸载 | 默认保留配置日志 | 恒全量清理 |
| 验证 | probe 返回原始字段 | probe 给出 verdict + checks |

## 11. 待拍板与下一阶段

**待拍板**：

1. `install.json` 是否记录触发者与请求 id（当前只有版本、平台、产物来源、摘要、时间；`deploy.rs:1797-1806`）。
2. 占位发布基址 `packages.example.com` 何时替换为真实地址；在此之前 release 默认路径仅能用于流程验证。
3. `full` 源是否应强制要求 `scripts/` 非空（当前仅提示 problem，范围读取时若无脚本仍允许 config 部分）。
4. 无 `curl` 的远端是否自动降级为 `wget`；当前是计划阶段阻塞并提示改用本地/远端已有产物，不静默降级。

**下一阶段（已另立文档）**：断连保活与关闭语义见 `docs/plan/remote-keepalive-and-shutdown-semantics.md`；本轮不引入 desiredState、不启动收敛循环。
