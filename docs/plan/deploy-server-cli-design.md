# 服务端与 CLI 部署方案

## 背景

当前 `deploy/` 只覆盖配置如何到达真实路径（`render-config.sh` 按层渲染、
`bootstrap-dev.sh` 开发自举、两份 systemd 单元模板、环境文件模板），缺失
从产物获取到服务上线的完整链路：没有包下载、没有二进制与前端产物的安装、
没有面向 root 的一键装机脚本、没有卸载与状态诊断脚本、没有系统包与日志
轮转配置。本方案参考 linkrs 的 `deploy/` 实现（systemd 单元、安装卸载与
状态脚本、DEB/RPM 打包、logrotate、Windows NSSM），补齐服务端与 CLI 模块
的部署能力，同时保留本项目已有的多层渲染机制。

## 现状与差异

linkrs 是单文件配置（`/etc/linkrs/config.toml`）加单服务二进制，
安装即复制模板；本项目是多层多文件配置（托管目录、个人设置目录、
项目 `.wf/` 三层，由 `render-config.sh` 渲染，密钥只进环境文件），
产物是多二进制（`wf-server` 服务端、`wf`、`wf-mini`、`wf-headless`、
`wf-debug` 四个 CLI）加一个前端静态目录（`apps/web-app/build`，
经 `--static-dir` 或 `WF_SERVER_STATIC_DIR` 由服务端托管）。
因此不能照搬复制逻辑：配置初始化必须复用 `render-config.sh`，
产物安装必须区分服务端 profile 与 CLI profile。

## 目标与非目标

目标：离线可装（本地构建产物）、在线可装（包下载，地址未定先占位）、
可重复执行（已存在文件默认不覆盖）、可诊断（状态脚本）、可打包
（DEB/RPM 元数据随 workspace 版本同步）、日志可轮转。

非目标：Windows 服务（NSSM 包装的可行性已在 linkrs 得到验证，
待产品确认需要后再移植，本期只在本文记录结论，不实现）；
systemd 之外的进程管理器；多机编排。

## 总体结构

新增脚本按特权与 profile 划分，不复用旧脚本名，避免职责混淆：

- 包下载：`deploy/scripts/fetch-package.sh`，服务端与 CLI 共用。
- 服务端（root 运行）：`install-server.sh`、`uninstall-server.sh`、
  `service-status.sh`，对标 linkrs 同名脚本。
- CLI（普通用户运行）：`install-cli.sh`，只放二进制，不碰服务与系统目录。
- 打包：`deploy/packaging/` 下放 `gen_metadata.sh`、debian 元数据与
  维护钩子、rpm spec，对标 linkrs。
- 日志：`deploy/logrotate/wf-server`，对标 linkrs。

## 包下载

下载方案未定，因此 `fetch-package.sh` 只定契约，不定地址。
默认基地址用占位符（环境变量 `WF_PACKAGE_BASE_URL` 覆盖），
产物按模块分两个压缩包（服务端包内含 `wf-server` 与前端静态目录，
CLI 包内含四个 CLI 二进制），配套同名校验文件。
校验语义取严格默认：有校验文件必须通过才继续；缺校验文件直接失败，
只允许显式传入跳过开关绕过并打印警告，避免静默接受不可验证的产物。
版本默认从 workspace `Cargo.toml` 读取，可用参数覆盖。
下载产物解压到暂存目录，供两份安装脚本消费，也支持安装脚本跳过下载、
直接使用本地构建产物（查找顺序为参数指定目录、`./bin`、当前目录、
已安装路径）。

## 产物移动

服务端安装默认前缀取 `/usr/local`，与现有单元模板中的
`/usr/local/bin/wf-server` 保持一致；改前缀时单元在渲染期同步改写，
不存在模板与安装位置漂移。CLI 安装默认前缀取 `$HOME/.local`，
root 运行才回退到 `/usr/local`。前端静态目录默认安装到
`/usr/share/wf/web`，安装后通过改写单元的 `--static-dir` 参数生效，
关闭开关则不接线、服务端按原有优先级自行解析。
二进制缺失时的行为区分场景：开发自举只警告；`START_SERVICE=yes`
要求启动时缺失直接失败，与 linkrs 一致。

## 配置初始化

服务端配置初始化全程调用 `render-config.sh --to managed`，
不复制任何单文件，渲染出的环境文件权限保持 `0600`。
与 linkrs 最大的不同是安装脚本不直接写业务值：数据目录与日志目录
通过环境文件中的路径锚定变量固定（sqlite 库路径、输出目录），
密钥项保持留空由运维在目标机填写；只有新建的环境文件才写入锚定，
已存在文件一律不动。CLI 不需要系统级配置初始化，
个人设置与项目增量继续沿用既有的 `--to settings`、`--to project` 与
开发自举脚本，安装脚本只负责二进制落位并提示补渲染。

## 系统服务设置

系统单元继续使用现有模板，安装脚本在渲染期做三处实例化：
替换托管目录与二进制路径；项目根为空时去掉 `--config` 参数及其读写路径，
保持无默认工作区的失败语义；静态目录启用时追加参数。
数据与日志目录追加进读写路径，使收紧后的沙箱仍能写入。
用户与目录沿用 linkrs 的做法（专用系统用户、家目录指向数据目录、
`0750` 目录权限、`027` umask）。启用与启动行为由环境变量显式控制，
默认只安装不自启不启动。卸载脚本只停服务、删单元，
数据与配置默认保留，按变量显式清理，二进制永远手动删除。

## 打包

版本号单一来源保持 workspace `Cargo.toml`，由 `gen_metadata.sh`
同步到 debian control 与 rpm spec，禁止手改包版本。
与 linkrs 有一处 deliberate 差异：托管配置树是安装期渲染生成的，
不属于 dpkg/rpm 追踪文件，因此不声明 conffile；
`postinst` 在配置目录缺失时调用随包附带的渲染脚本生成，
`postrm` 只在 purge 时删除数据与配置目录，`remove` 保留。
spec 的用户创建、目录权限、systemd 宏行为与 DEB 侧对齐。

## 日志轮转

服务端文件日志经输出目录变量指向 `/var/log/wf` 后，
由 logrotate 接管：每日轮转、保留份数与压缩策略对标 linkrs，
采用截断方式以适配常驻进程持有的文件句柄。

## 验收

每份脚本支持试运行式检查（状态脚本只读、安装脚本可先看计划再执行），
打包元数据同步脚本可独立运行验证版本一致性；
`render-config.sh` 的行为与职责边界保持不变，
`deploy/README.md` 更新目录结构与快速开始说明。
