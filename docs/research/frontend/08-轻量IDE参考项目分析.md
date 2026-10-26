# 轻量 IDE 功能参考项目分析

本文件分析为前端补充「终端执行、文件树查看、文件编辑」这类轻量 IDE 能力而参考的项目与库。对应设计方案见 `docs/plan/lightweight-ide-design.md`。

## 1. 研究范围

| 类别 | 对象 |
| --- | --- |
| 当前项目 | `apps/web-app`（SvelteKit + Svelte 5 + Tailwind v4），已有 PTY 终端与 xterm.js |
| 参考项目 A | `coder/code-server`：浏览器中的 VS Code |
| 参考项目 B | `filebrowser/filebrowser`：轻量 Web 文件管理器 |
| 参考库 C | `microsoft/monaco-editor`：VS Code 编辑器内核 |
| 参考库 D | `xtermjs/xterm.js`：浏览器终端模拟器（项目已使用） |

本分析聚焦前端交互、API 形态与工程成本，不包含后端执行语义与权限模型的完整设计。

## 2. code-server

### 2.1 基本形态

code-server 把完整 VS Code 运行在远端，浏览器只作为前端壳。终端、文件树、编辑器、扩展、调试器等能力全部来自 VS Code 本体，code-server 负责 HTTP/WebSocket 代理、认证与进程生命周期。

### 2.2 可借鉴点

- **终端通过 PTY + WebSocket 提供**：与当前项目已实现的 PTY 通道一致，说明现有 `/api/v1/pty` 与 xterm.js 路线可直接复用。
- **文件访问走 VS Code 文件服务**：不是额外的「文件 API 产品」，而是编辑器内核自带的文件系统抽象；轻量实现不应复制整套 VS Code。
- **URL 可定位文件**：支持 `payload` 查询参数打开指定文件与行列，便于从聊天、执行、Git 页面跳转到编辑器。
- **多标签共享同一后端会话**：每个浏览器标签独立 WebSocket，但共享同一个 VS Code 服务进程；轻量实现可以按会话/标签隔离前端状态，后端保持无状态文件 API。
- **反向代理与 WebSocket**：部署时必须正确转发 `Upgrade` 头，现有 PTY 已有同类要求。

### 2.3 不建议照搬

- 完整 VS Code 体积与复杂度过高，不符合「轻量」目标。
- 扩展宿主、语言服务、调试器等能力超出本期范围。

## 3. File Browser

### 3.1 基本形态

File Browser 是轻量 Web 文件管理器，提供目录浏览、文件读写、上传下载、重命名、删除、命令执行等能力，API 面向资源路径。

### 3.2 可借鉴点

- **资源型 API**：`GET /api/resources/{path}` 对目录返回列表、对文件返回内容与元数据；`PUT` 更新内容；`POST` 创建；`DELETE` 删除；`PATCH` 重命名/复制。这种按路径资源建模的方式适合前端文件树。
- **目录与文件统一路径**：用尾斜杠区分目录，前端可以复用同一套路径字符串。
- **类型检测**：`FileInfo.Type` 区分 `video/audio/image/pdf/text/textImmutable/blob`，前端据此决定是否可编辑、是否只读。
- **权限与规则**：读写权限、路径规则在后端集中校验，前端只消费结果。
- **递归列表**：`/api/resources/recursive/{path}` 提供扁平递归列表，适合搜索与全局文件定位。

### 3.3 不建议照搬

- File Browser 的权限/分享/缩略图等能力超出本期范围。
- 其前端是独立应用，不与当前 Agent 工作流 UI 集成；我们应嵌入现有 AppShell 与导航。

## 4. Monaco Editor

### 4.1 基本形态

Monaco 是 VS Code 的编辑器内核，提供语法高亮、IntelliSense、diff、多模型等能力，但需要 Web Worker 与较大 bundle。

### 4.2 可借鉴点

- **按文件创建 Model**：一个文件对应一个 `ITextModel`，切换标签时复用 model，避免重复解析。
- **语言自动识别**：根据文件扩展名/路径设置 language，提升编辑体验。
- **懒加载**：通过动态 import 和 worker 配置控制初始 bundle。
- **只读模式**：对二进制、超大文件或无权限文件使用只读 editor。

### 4.3 成本与风险

- bundle 与 worker 配置复杂，对 SvelteKit/Vite 需要额外适配。
- 若只做轻量编辑，CodeMirror 6 是更小的替代；但 Monaco 的生态与 VS Code 一致性更强。设计阶段保留选型决策点。

## 5. xterm.js

### 5.1 当前状态

项目已使用 `@xterm/xterm`、`addon-fit`、`addon-serialize`、`addon-web-links`，并有完整 PTY WebSocket 协议与重连逻辑。

### 5.2 可借鉴点

- **FitAddon**：容器尺寸变化时重新计算行列数。
- **SerializeAddon**：保存/恢复终端缓冲，便于标签切换与刷新恢复。
- **WebLinksAddon**：终端中的 URL 可点击。
- **已有协议**：光标恢复、溢出重连、退出帧等机制可直接复用，IDE 只需把终端嵌入新布局。

## 6. 对当前项目的总体判断

1. **终端能力已具备**：不需要新增后端终端协议，只需复用 PTY 与 xterm 组件。
2. **文件能力缺失**：当前只有 Git 文件状态与 checkpoint 文件视图，没有通用文件系统浏览/读写 API。
3. **编辑器能力缺失**：当前只有 JSON 编辑器，没有通用代码编辑器。
4. **最轻量落地方案**：后端增加受控文件 API，前端增加文件树 + 编辑器 + 复用终端，组成 `/ide` 页面。

## 7. 借鉴来源对照

- 向 code-server 借终端/文件/编辑器组合形态与 URL 打开文件能力，不借完整 VS Code。
- 向 File Browser 借资源型文件 API、路径安全校验、类型检测与只读判定。
- 向 Monaco 借 model-per-file 与懒加载策略。
- 向 xterm.js 借已有终端组件与缓冲恢复策略。
