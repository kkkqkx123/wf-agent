# devs 服务器同步与远程编译指南

devs（`ssh devs`）已具备 Rust 编译环境：cargo 已安装（不在默认 PATH），stable 工具链可用；crates.io 可访问，依赖由 devs 自己执行 `cargo fetch --locked` 安装。Lua 运行时已改为 Lua 5.4 源码构建（`vendored`），不再需要用户目录下的 LuaJIT 与 `pkg-config` 探测，只需 C 编译器；下文脚本里遗留的 `PKG_CONFIG_PATH` / `LD_LIBRARY_PATH` 已无作用，可保留亦可移除。本指南只说明 Rust 侧的同步与编译用法；前端不在 devs 构建，也不传输 `node_modules` 或 npm 缓存。同步包不使用校验清单，以包内文件数与解压后体积抽查为准。

## 1. 目录约定

| 用途 | devs 路径 |
|---|---|
| 同步包与构建信息 | `/data/kkkqkx/wf-agent/` |
| 代码解压目录（非 git 仓库，直接覆盖） | `/data/kkkqkx/wf-agent/repo/` |
| 编译产物（`CARGO_TARGET_DIR`） | `/data/kkkqkx/wf-agent/target/`，不回传 |
| 依赖缓存 | devs 默认 `CARGO_HOME`，由 cargo 维护 |
| 编译入口脚本 | `/data/kkkqkx/wf-agent/check.sh`，已存在 |

## 2. 同步方式

| 变更 | 方式 |
|---|---|
| 数个源文件改动 | `scp` 覆盖远端对应路径 |
| 多文件、重构、首次同步 | `tar.gz` 打包 + `scp` + 覆盖解压 |
| 依赖 | 不传输，在 devs 执行 `cargo fetch --locked` |
| 编译日志 | `scp` 取回 |

任何情况下都不传 `target/`、`node_modules/`、前端构建产物与 `e2e/bin/`。

## 3. 打包排除

打包时排除：`target/`、各级 `.git`（含 submodule 的 gitlink）、各级 `node_modules/`、`apps/web-app/build/`、`apps/web-app/.svelte-kit/`、`e2e/bin/`、`ref/`、`logs/`、`test_logs/`、`data/`、`__pycache__/`、`*.pyc`、`.mypy_cache/`、`.cache/`、`*.log`、`*.bak`。

必须进包：`Cargo.lock`（被 gitignore，但离线锁版本依赖它）、`crates/infra/llm-suite/` 与 `crates/infra/simvec/` 的工作树、`crates/app/wf-server/openapi.json`、源码与脚本。

## 4. 同步步骤

本地打包与构建信息：

```bash
cd /home/kkkqkx/code/wf-agent
SHA=$(git rev-parse --short HEAD)
cargo metadata --locked --format-version=1 >/dev/null
mkdir -p ~/sync-pkgs

{ echo "sha=$SHA"; echo "date=$(date -u +%FT%TZ)";
  rustc --version | sed 's/^/local_rustc=/';
  git status --short; git submodule status; } > ~/sync-pkgs/wf-agent-src-$SHA.BUILD_INFO.txt

cd /home/kkkqkx/code
tar -zcf ~/sync-pkgs/wf-agent-src-$SHA.tar.gz \
  --exclude='*/target' --exclude='*/.git' \
  --exclude='*/node_modules' \
  --exclude='wf-agent/apps/web-app/build' \
  --exclude='wf-agent/apps/web-app/.svelte-kit' \
  --exclude='wf-agent/e2e/bin' \
  --exclude='wf-agent/ref' \
  --exclude='wf-agent/logs' --exclude='wf-agent/test_logs' \
  --exclude='wf-agent/data' \
  --exclude='*/__pycache__' --exclude='*/.mypy_cache' \
  --exclude='*.bak' --exclude='*.log' \
  wf-agent
```

传输并覆盖解压：

```bash
ssh devs 'mkdir -p /data/kkkqkx/wf-agent/repo'
scp ~/sync-pkgs/wf-agent-src-$SHA.tar.gz \
    ~/sync-pkgs/wf-agent-src-$SHA.BUILD_INFO.txt \
    devs:/data/kkkqkx/wf-agent/

ssh devs "cd /data/kkkqkx/wf-agent && tar -xzf wf-agent-src-$SHA.tar.gz -C repo --strip-components=1 && cp wf-agent-src-$SHA.BUILD_INFO.txt repo/BUILD_INFO.txt"

# 抽查：比对本地与远端的包内文件数，并查看远端解压体积
tar -tzf ~/sync-pkgs/wf-agent-src-$SHA.tar.gz | wc -l
ssh devs "cd /data/kkkqkx/wf-agent && tar -tzf wf-agent-src-$SHA.tar.gz | wc -l && du -sh repo"
```

单文件改动直接覆盖：

```bash
scp crates/app/wf-api/src/lib.rs \
  devs:/data/kkkqkx/wf-agent/repo/crates/app/wf-api/src/lib.rs
```

## 5. 远端编译

`check.sh` 已固化所需环境（cargo 的 PATH、`RUSTUP_TOOLCHAIN=stable`、外部 `CARGO_TARGET_DIR`；其中 LuaJIT 的 `PKG_CONFIG_PATH` / `LD_LIBRARY_PATH` 在 VM 迁移后已无作用），依次执行 `cargo fetch --locked` 与 `cargo check --locked`。每次同步后直接运行：

```bash
ssh devs "nohup bash /data/kkkqkx/wf-agent/check.sh > /data/kkkqkx/wf-agent/check-$SHA.log 2>&1 < /dev/null &"
ssh devs "tail -n 20 /data/kkkqkx/wf-agent/check-$SHA.log"
```

不用脚本时，单次快速验证：

```bash
ssh devs "cd /data/kkkqkx/wf-agent/repo && env PATH=/home/itc/.cargo/bin:/usr/bin:/bin RUSTUP_TOOLCHAIN=stable CARGO_TARGET_DIR=/data/kkkqkx/wf-agent/target PKG_CONFIG_PATH=/data/kkkqkx/wf-agent/deps/luajit/lib/pkgconfig cargo check --locked"
```

## 6. 反向取回

```bash
mkdir -p ~/devs-logs
scp devs:/data/kkkqkx/wf-agent/check-$SHA.log ~/devs-logs/
```

## 7. 注意事项

- 覆盖解压不会删除远端多余文件；源码改名或删除后需手动清理 `repo/` 下旧文件。清空 `repo/` 重新解压不影响 `target/`。
- submodule 以包内 checkout 为准，不在 devs 上执行 `git submodule update`；升级 submodule 提交后走整包同步。
- `Cargo.lock` 只存在于工作树，务必确认进包；依赖变化后先在远端执行 `cargo fetch --locked`，锁文件过期则回本地更新后再同步。
- `BUILD_INFO.txt` 记录本地与远端工具链版本；远端 stable 版本由 rustup 决定，不要用远端产物反推本机结果。
