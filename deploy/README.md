# deploy/ — 部署机制（从不参与加载）

本目录回答"生产配置如何到达真实路径"，本身永不参与配置加载。
当前实际没有任何生产真文件，全部仍是演示；
真文件后续在目标机器上由本目录的机制渲染，绝不提交进仓库。

## 目录结构

```
deploy/
├── scripts/
│   ├── render-config.sh      # 目标机按层渲染配置（managed/settings/project）
│   ├── bootstrap-dev.sh      # 开发自举渲染
│   ├── fetch-package.sh      # 下载版本化产物包（基地址为占位符，见下）
│   ├── install-server.sh     # 服务端一键安装（root：用户/目录/产物/配置/单元）
│   ├── install-cli.sh        # CLI 安装（普通用户：仅二进制落位）
│   ├── uninstall-server.sh   # 服务端卸载（默认保留数据与配置）
│   └── service-status.sh     # 服务状态诊断
├── systemd/                  # 系统服务与个人单元模板（@@占位符，安装期渲染）
├── env/                      # 环境文件模板（0600，密钥只留空键）
├── templates/                # 生成目录：sync-deploy.sh 单向同步，评审但不手改
├── packaging/                # 系统包（deb/rpm）打包配置与维护钩子
│   ├── debian/control        # DEB 包元数据（版本由 gen_metadata.sh 同步）
│   ├── gen_metadata.sh       # 从 workspace 版本同步包版本号
│   ├── postinst/prerm/postrm # DEB 安装后/卸载前/卸载后钩子
│   └── rpm/wf-agent.spec     # RPM spec（含 systemd scriptlet）
├── logrotate/wf-server       # 日志轮转（/var/log/wf/*.log）
└── windows/                  # 暂无。NSSM 方案待产品确认需要后移植。
```

## 职责边界

- `config-examples/`：值长什么样（唯一的值来源）。
- `deploy/`：值如何到达真实路径（脚本、服务单元、渲染模板、占位目录）。
- 真实路径：`/etc/wf` 或显式设置目录（托管/共享服务）、`~/.config/wf`
  或 `~/.wf`（个人自用）、业务仓库 `.wf/`（项目增量）。
- 代码默认值：以上全缺席时的底线（开发 Development、生产 Production
  两套环境默认，见 `wf-config::processor::infrastructure`）。

## 落位布局

- `deploy/templates/`：由 `scripts/sync-deploy.sh` 从 `config-examples/`、
  `.wf/`、`.wf-user/` 单向同步生成，可评审、不可手改。模板只含加载器
  会读的 live 家族与可导入的种子家族，不含其他目录。
- `deploy/scripts/render-config.sh`：目标机按层渲染
 （`--to managed|settings|project`），部署期占位符用 `@@NAME@@`，
  `${VAR}` 一律透传给进程启动时展开；环境文件 `0600`。
  各层覆盖与加载器的对应关系：
  `managed` 渲染运维 live 层（`infrastructure/`、`server/`、
  `config.toml`、`mcp/skill-settings.json`、`llm-auth`）到托管目录，
  该目录即服务的 `settings_dir`；`settings` 渲染同样的 live 家族另加
  `llm-profiles/llm-providers/` 种子到个人设置目录；`project` 整体渲染
  `templates/project/` 到 `.wf/`（含各资源索引家族的导入起点，加载器
  在此只自动读 `mcp.json/skills.json`），`infrastructure/server` 覆盖
  仅随 `--with-project-overrides` 进入项目。
- Live 与种子的界限：网域九文件、传输四文件、`config.toml`、
  `mcp/skill-settings`、`llm-auth` 由加载器自动读，是 live 配置；
  `llm-profiles/llm-providers` 与各资源索引家族（`workflows`、
  `agent-loops`、`node-templates`、`prompt-templates`、`scripts`）只做
  `import` 起点，网关与注册表的来源是 API 与存储，启动时不扫描它们。
- `deploy/scripts/bootstrap-dev.sh`：开发自举渲染（开发环境默认不足时补齐）。
- `deploy/systemd/`：系统服务与个人单元模板。单单元服务全部业务目录：
  每个工作区请求自带目录（`x-wf-directory` 请求头或 `directory` 查询参数），
  服务端不存在默认工作区，无目录的工作区请求直接失败，绝不静默绑定。
  `WorkingDirectory` 取中性位置、不承载数据语义；`--config` 只选项目层
  配置来源，从不是操作默认。`EnvironmentFile` 给密钥与路径锚定，
  显式传参，不写死任何业务路径。
- `deploy/env/wf-server.env.template`：环境文件模板。`WF_USER_GIT_ENABLED`
  是工作区域总开关（默认开，关则全部工作区路由显式拒绝），
  `WF_STORAGE_SQLITE_DB_PATH` 钉死单全局库位置（相对路径在服务语境下无定义），
  `WF_OUTPUT_DIR` 钉死文件日志目录（logrotate 以此目录为轮转对象），
  `WF_DIRECTORY_ROOTS` 列出可被显式请求的目录根（为空则允许任何已校验目录）；
  密钥只留空键、不带值。

## 服务端安装（install-server.sh，root 运行）

流程：可选下载 → 产物移动 → 托管配置渲染 → 路径锚定 → 单元安装。

1. 建系统用户/组 `wf`（shell 为 nologin，家目录在数据目录）。
2. 建目录 `/etc/wf`、`/var/lib/wf`、`/var/log/wf`，属主 `wf:wf`，`0750`；umask 027。
3. 装二进制：`wf-server` 必装；`INSTALL_CLI=yes` 时连带 `wf`、`wf-mini`、
   `wf-headless`、`wf-debug`。来源按 `BIN_SRC_DIR` → `./bin` →
   仓库 `target/release` → 当前目录 → 已安装路径查找；`FETCH=yes` 时先经
   `fetch-package.sh` 下载。`START_SERVICE=yes` 时缺失直接失败，否则仅警告。
4. 装前端：`INSTALL_WEB=yes`（默认）时把 `apps/web-app/build` 装到
   `/usr/share/wf/web`，并在单元追加 `--static-dir`；缺失则不接线仅警告。
5. 渲染托管配置：调用 `render-config.sh --to managed`；仅新建的环境文件
   写入路径锚定（库路径→数据目录、输出目录→日志目录），密钥留空待运维填写，
   已存在文件一律不动。
6. 渲染并安装单元到 `/etc/systemd/system/wf-server.service`：替换托管目录
   与二进制路径；`PROJECT_ROOT` 为空时去掉 `--config` 参数（保持无默认
   工作区语义）；数据与日志目录追加进读写路径；`daemon-reload`。
7. 按 `ENABLE_SERVICE`/`START_SERVICE` 决定自启与启动（默认都不做）。

可用环境变量覆盖默认行为：

| 变量 | 默认 | 说明 |
|---|---|---|
| `PREFIX` | `/usr/local` | 二进制安装前缀（单元路径同步渲染，改前缀无需手改单元） |
| `CONFIG_DIR` / `DATA_DIR` / `LOG_DIR` | `/etc/wf`、`/var/lib/wf`、`/var/log/wf` | 托管配置/数据/日志目录 |
| `STATIC_DIR` | `/usr/share/wf/web` | 前端静态目录安装位置 |
| `PROJECT_ROOT` | 空 | 项目层配置来源；为空则单元不带 `--config` |
| `SERVICE_USER` / `SERVICE_GROUP` | `wf` | 服务运行账户 |
| `FETCH` / `PACKAGE_VERSION` / `BIN_SRC_DIR` | `no` | `yes` 时先下载；版本默认读 workspace；或直接指定产物目录 |
| `INSTALL_CLI` / `INSTALL_WEB` | `no` / `yes` | 是否连带 CLI 二进制 / 前端静态目录 |
| `SKIP_CONFIG` / `SKIP_UNIT` | `no` | 跳过配置渲染 / 单元安装 |
| `ENABLE_SERVICE` / `START_SERVICE` | `no` | `yes` 开机自启 / 安装后启动 |

## CLI 安装（install-cli.sh，普通用户运行）

只把四个 CLI 二进制落位到 `$PREFIX/bin`（默认 `$HOME/.local`），
不碰服务与系统目录。产物来源与服务端一致（`FETCH=yes` 或本地目录）。
个人设置与项目增量仍用 `render-config.sh --to settings/project` 处理，
本脚本不管配置。

## 包下载（fetch-package.sh）

下载地址尚未确认，`WF_PACKAGE_BASE_URL` 现为占位符
（`https://packages.example.com/wf-agent`），确认后覆盖该变量或传
`--base-url` 即可，产物布局契约不变：按模块分服务端包
（含 `wf-server` 与 `web/`）与 CLI 包（含四个 CLI 二进制），
配套同名 `.sha256` 校验文件。校验严格默认：有校验文件必须通过；
缺校验文件直接失败，仅 `--allow-missing-checksum` 可显式绕过并告警。
版本默认从根 `Cargo.toml` 的 workspace 版本读取。

## 卸载与诊断

- `uninstall-server.sh`（root）：停服务、disable、删单元文件并
  `daemon-reload`。默认保留数据与配置；`PURGE_CONFIG=yes` 删托管目录，
  `PURGE_DATA=yes` 删数据与日志目录。二进制永不自动删除。
- `service-status.sh`：输出单元状态、enable/active 状态与最近 journal 日志。

## 打包

**版本单一来源**：包版本号来自根 `Cargo.toml` 的 `[workspace.package].version`，
由 `gen_metadata.sh` 同步写入 `debian/control` 与 `wf-agent.spec`，不要手工编辑。

**DEB**：包名 `wf-agent`，依赖 `libc6`、`systemd`。托管配置树是安装期
渲染生成的，不属于 dpkg 追踪文件，故不声明 conffile：`postinst` 仅在
`/etc/wf` 缺席时调用随包附带的渲染脚本生成，升级永不覆盖；
`prerm` 卸载前停止并 disable 服务；`postrm` 仅在 `purge` 时删除
数据与配置目录（`remove` 保留）。

**RPM**：`%pre` 创建用户/组与目录；`%post/%preun/%postun` 使用 systemd 宏；
首次安装（`$1 = 1`）渲染托管配置；仅完全擦除（`$1 = 0`）时删除数据与配置。
路径使用 RPM 宏（`%{_bindir}`、`%{_unitdir}`）。打包文件：五个二进制、
单元文件、`/usr/share/wf/web`、`/usr/share/wf/deploy`（模板与渲染脚本）、
数据/日志/托管目录（`%dir`）。

## 日志轮转（logrotate/wf-server）

对 `/var/log/wf/*.log`：每日轮转、保留 7 份、压缩（delaycompress）、
`copytruncate`（服务端持有文件句柄，截断后无需信号即可继续写新文件）、
文件不存在不报错、空文件不轮转；新文件权限 `0640 wf wf`。
前置条件：部署后输出目录已由环境文件锚定到 `/var/log/wf`。

## 快速开始

```shell
# 构建
cargo build --release
npm --prefix apps/web-app run build

# 服务端：安装服务（二进制在 ./bin 或 target/release 下）
sudo ./deploy/scripts/install-server.sh

# 服务端：下载包再安装（地址占位，确认后覆盖 WF_PACKAGE_BASE_URL）
sudo WF_PACKAGE_BASE_URL=https://packages.example.com/wf-agent \
  FETCH=yes ENABLE_SERVICE=yes START_SERVICE=yes \
  ./deploy/scripts/install-server.sh

# CLI：当前用户安装
./deploy/scripts/install-cli.sh

# 管理
sudo systemctl start wf-server
sudo systemctl status wf-server
sudo systemctl restart wf-server     # 配置变更后重启（热重载暂未实现）

# 诊断
./deploy/scripts/service-status.sh

# 卸载
sudo ./deploy/scripts/uninstall-server.sh
```

## 禁忌

- 无秘密、无渲染后的真实值进仓库。
- 不把某台机器的实际配置复制回仓库当"生产基线"。
- 服务单元按 principal 区分：系统服务指向托管目录与项目 `.wf/`，
  个人自用指向 XDG；绝不让共享服务读取 ambient 个人目录。
- 生产部署必须渲染全部网域文件或显式设置运行环境，
  否则回退到代码默认（开发环境默认指向 `./dev-storage` 等开发路径）。
