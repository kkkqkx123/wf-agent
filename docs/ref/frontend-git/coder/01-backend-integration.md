# 后端集成方式分析（code-server ↔ VSCode ↔ git）

> 所有 `file:line` 引用：code-server 见 `fa7caee4`；VSCode git 扩展见 `04c0d99f`（`extensions/git/src/`）。

## 0. 核心发现

code-server 的"后端集成"实际是**两层委托**，每一层都给出了可借鉴的设计：

- **第一层：code-server ↔ VSCode 服务端**——把 VSCode 当库 `import` 进来，拿到一个极简服务门面后，把**全部 HTTP / WebSocket 流量**委托给它，自己只在前面套一层鉴权与静态资源。
- **第二层：git 扩展 ↔ `git` 可执行文件**——git 扩展不内嵌任何 git 库，而是**用子进程编排 `git` CLI**，并通过**带外通道**（env 注入的助手脚本 + IPC/文件管道）处理本应卡在 stdin 的交互式输入（凭据、编辑器、SSH）。

## 1. 第一层：把 VSCode 当库加载并委托全部流量

### 1.1 动态加载 VSCode 服务端

- 定义极简门面 `IVSCodeServerAPI`：`handleRequest` / `handleUpgrade` / `handleServerError` / `dispose` —— `src/node/routes/vscode.ts:27-32`。
- `loadVSCode()` 用 `eval(import("…/out/server-main.js"))` 加载 VSCode 的 `server-main.js`，调用 `createServer()` 得到 `IVSCodeServerAPI` 实例 —— `src/node/routes/vscode.ts:55-76`。注释明确说明：为了能和自己的 web server 集成，他们对 `server-main.js` 打了 patch 以便 `require` 并直接调用其函数。
- 单例缓存：`vscodeServerPromise` / `vscodeServer`（`vscode.ts:80-84`），`ensureVSCodeLoaded` 中间件保证只加载一次（`vscode.ts:89-116`）。

### 1.2 全流量委托

```ts
// src/node/routes/vscode.ts:240-242  HTTP 全量委托
router.all(/.*/, ensureAuthenticated, ensureVSCodeLoaded, async (req, res) => {
  vscodeServer!.handleRequest(req, res)
})

// src/node/routes/vscode.ts:244-253  WebSocket 全量委托
const socketProxyProvider = new SocketProxyProvider()
wsRouter.ws(/.*/, ensureOrigin, ensureAuthenticated, ensureVSCodeLoaded, async (req) => {
  const wrappedSocket = await socketProxyProvider.createProxy(req.ws)
  vscodeServer!.handleUpgrade(req, wrappedSocket as net.Socket)
  req.ws.resume()
})
```

- 挂载点：在 `src/node/routes/index.ts:171-172` 中对 `["/vscode", "/"]` 两个前缀 `app.router.use(routePrefix, vscode.router)`，作为 catch-all 兜底，置于鉴权/代理/health 路由之后。

### 1.3 WebSocket 桥接与反向代理

- `src/node/wsRouter.ts:16-32` 的 `handleUpgrade`：把每个 `upgrade` 事件包装成 fake request 重新送回 Express，从而让 `ensureOrigin` / `ensureAuthenticated` 等中间件对 WebSocket **同样生效**——这是"WS 也要走鉴权"的标准做法。
- `src/node/socket.ts:14` `SocketProxyProvider`：当 TLS socket 无法被传递给子进程时，起一个本地 unix socket 代理做 `pipe`（`socket.ts:35-75`）。说明 code-server 把"把 socket 交给 VSCode"当作一等公民对待。
- 反向代理：`src/node/routes/domainProxy.ts:97-100` / `:114-117` 把 `code-{{port}}.{{host}}` 子域流量 `proxy.web` / `proxy.ws` 到 `http://0.0.0.0:${port}`——即 VSCode "Ports" 标签的端口转发模型。

### 1.4 这一层可借鉴什么

| 设计 | 对 wf-agent 的借鉴 |
|---|---|
| **薄边界 + 委托**：VSCode 逻辑零重写，仅包一层门面 | 与 `frontend/04` 一致——把 git 当外部子系统，用**窄而稳定的 API 门面**（类比 `IVSCodeServerAPI`）包裹，别把 git 逻辑塞进前端组件 |
| **鉴权在前置中间件** | 与 `frontend/04` 的 `destructive` 后端属性一致：鉴权/授权在边界做，git 逻辑不自己判断权限 |
| **WS 也走中间件** | 我们的 git WS 事件流同样需要 `ensureOrigin` + 鉴权，不能裸暴露 |
| **端口/域反向代理** | 若 wf-agent 要内嵌预览服务器，可直接套用此模型 |

## 2. 第二层：git 扩展如何驱动 git（最值得借鉴）

### 2.1 一律编排 `git` CLI，绝不内嵌库

```ts
// extensions/git/src/git.ts:676-702  Git.spawn
spawn(args, options) {
  // ...
  options.env = assign({}, process.env, this.env, options.env || {}, {
    VSCODE_GIT_COMMAND: args[0],
    LANGUAGE: 'en', LC_ALL: 'en_US.UTF-8', LANG: 'en_US.UTF-8',
    GIT_PAGER: 'cat',                     // 禁用分页，避免卡住
  })
  const cwd = this.getCwd(options)
  if (cwd) options.cwd = sanitizePath(cwd)
  return cp.spawn(this.path, args, options)   // 永远是子进程 git
}
```

- `git.ts:387` `export class Git`；`this.env` 在构造时合并（含 `GIT_ASKPASS`，见 §2.2）。
- 其他 env 用法：`git.ts:2145` 用 `{ env: { GIT_EDITOR: 'true' } }` 抑制交互；`git.ts:2749` `GIT_OPTIONAL_LOCKS: '0'` 避免锁等待；`git.ts:2395` `GIT_HTTP_USER_AGENT` 自定义。
- **结论**：VSCode 不用 `libgit2` / `nodegit`，完全靠子进程 `git`。理由：语义与系统 git 完全一致、易调试、随系统升级。

### 2.2 带外交互通道：askpass（凭据 / 口令）★最重要

**问题**：`git push/pull` 需要用户名密码或 SSH 私钥口令时，进程会卡在 stdin 等待——而真实 git 跑在服务器、输入在浏览器。VSCode 的解法是一套**带外通道**：

1. **注入 env**：`askpass.ts:36-43` 把 `GIT_ASKPASS=askpass.sh`、`SSH_ASKPASS=ssh-askpass.sh` 连同 `VSCODE_GIT_ASKPASS_NODE` / `VSCODE_GIT_ASKPASS_MAIN` 写入子进程环境。
2. **git 触发**：git 需要凭据时调用 `askpass.sh <prompt> <uri>`。
3. **助手脚本**（`askpass.sh` 全文）：
   ```sh
   #!/bin/sh
   VSCODE_GIT_ASKPASS_PIPE=`mktemp`          # 建临时管道文件
   ELECTRON_RUN_AS_NODE="1" VSCODE_GIT_ASKPASS_PIPE="$VSCODE_GIT_ASKPASS_PIPE" \
     VSCODE_GIT_ASKPASS_TYPE="https" "$VSCODE_GIT_ASKPASS_NODE" \
     "$VSCODE_GIT_ASKPASS_MAIN" $VSCODE_GIT_ASKPASS_EXTRA_ARGS $*
   cat $VSCODE_GIT_ASKPASS_PIPE              # 把答案回传给 git
   rm $VSCODE_GIT_ASKPASS_PIPE
   ```
4. **Node 端回传**：`askpass-main.ts:15-43` 起 `IPCClient('askpass').call({askpassType, argv})`，拿到答案后 `fs.writeFileSync(pipe, res)`。
5. **扩展主机弹 UI**：`Askpass.handleAskpass`（`askpass.ts:68-115`）→ `window.showInputBox()` 在 VSCode UI 弹出输入框收集；SSH 同理用 `showQuickPick` 处理 host authenticity / passphrase（`askpass.ts:117-159`）。
6. **兜底**：无 IPC 时使用 `askpass-empty.sh`（直接返回空），保证无头环境不卡死。

> 这是"**无头服务端 git 进程从远程浏览器 UI 获取交互输入**"的标准范本。它绕开了 PTY，用"env 注入的助手脚本 + 文件管道 + IPC 请求/响应"完成凭据回流。

### 2.3 带外交互通道：gitEditor（交互式 rebase / `commit -e`）

- `gitEditor.ts:29-34` 注入 `GIT_EDITOR=git-editor.sh`。
- `gitEditor.ts:37-54` `handle()`：收到 commit message 路径 → 在 VSCode 打开文本编辑器 → 用 `Promise` 等待该 tab 关闭才返回 `true`，git 继续。
- 兜底：无 IPC 用 `git-editor-empty.sh` 直接成功。
- 效果是：把"交互式 git（`rebase -i`、`commit --edit`）"转化为"编辑器中的结构化操作"，而非塞进 PTY 硬撑。

### 2.4 操作串行化与乐观更新

```ts
// extensions/git/src/repository.ts:44-46  极简状态机
export const enum RepositoryState { Idle, Disposed }

// repository.ts:2709-2748  run(operation, runOperation)
if (this.state !== RepositoryState.Idle) throw new Error('Repository not initialized')
this._operations.start(operation)          // :2721 串行化入口
// ... retryRun ...
if (!operation.readOnly) await this.updateModelState(...)  // :2728 乐观更新
// ... finally: this._operations.end(operation)  :2744
```

- 仓库必须 `Idle` 才能开始新操作，用 `_operations` 跟踪器串行化，**防并发 git 命令竞态**。
- `repository.ts:3192-3202` `eventuallyUpdateWhenIdleAndWait`：仅在空闲时刷新模型，避免并发刷新抖动。
- `operation.readOnly` 是一等公民（决定要不要刷模型）——与"危险/只读"分层天然对应。

### 2.5 这一层可借鉴什么

| 设计 | 对 wf-agent 的借鉴 |
|---|---|
| **CLI 编排而非内嵌库** | 后端 git 域应直接 `exec('git', [...])`（可复用 `wf-shell` 的进程/PTY 能力），不要引入 `git2`/`gix`；与 `frontend/04` 一致 |
| **askpass 带外通道** | 直接解决 `frontend/03` 的 `P5`（send_input 卡死）与交互式命令（`rebase -i`/`add -p`）：注入 `GIT_ASKPASS`/`GIT_EDITOR` 助手，凭据/编辑经 WS 请求-响应回传，而非强塞 PTY |
| **空脚本兜底** | 无浏览器应答时 git 不能挂起——必须有 timeout / empty 兜底 |
| **操作串行化** | git 动作端点按 仓库/会话 加互斥（操作队列），防"连点两次 push"竞态；空闲才刷状态 |
| **readOnly 分层** | 对应 `frontend/04` 的 `destructive` 分级：只读动作不进危险队列、不刷模型 |

## 3. 小结

后端集成 = **"边界委托（code-server ↔ VSCode）"** + **"CLI 编排 + 带外交互（git 扩展 ↔ git）"**。

- 前者告诉我们：**边界要薄、鉴权在前置、git 当外部子系统包一层稳定门面**。
- 后者告诉我们：**交互式 git 必须用带外通道（助手脚本 + WS 回流），而不是用 PTY 硬撑**——这是本次分析对 wf-agent 最具修正价值的一点。
