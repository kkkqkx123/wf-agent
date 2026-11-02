# opencode 临时对话（Draft Session）逻辑分析

> 分析对象：`ref/opencode` 快照（sst/opencode 仓库）Web App 前端。  
> 分析目的：梳理 opencode 中"尚未落库的临时对话"从创建、持久化、渲染到晋升为真实会话的完整逻辑。  
> 说明：opencode 把这种临时对话称为 **draft session**（草稿会话），以 `DraftTab` + `draftID` 表示；本文统一用"临时对话"指代。

---

## 1. 概述

opencode 的 Web App 不会在用户点"新建会话"时立即创建服务端 session。相反，它先创建一个**只存在于前端的临时对话**（draft）：

- 生成一个前端 UUID `draftID`；
- 把该对话作为浏览器 Tab（`DraftTab`）加入 Tab 栏；
- 把输入框内容、模型选择、上下文、文件视图、布局等状态持久化到 draft 专属存储；
- 路由跳转到 `/new-session?draftId=...`，渲染新建会话页面；
- 只有在用户真正提交第一条消息时，才调用后端 `session.create` 创建真实 session，并把 draft **晋升**（promote）为 `SessionTab`。

因此，draft 是"未提交的第一条消息之前"的临时会话容器，也是 Tab 系统、路由系统、持久化系统三者协作的结果。

---

## 2. 核心数据结构

### 2.1 Tab 类型

文件：`packages/app/src/context/tabs.tsx`

```ts
export type SessionTab = {
  type: "session"
  server: ServerConnection.Key
  sessionId: string
}

export type DraftTab = {
  type: "draft"
  draftID: string
  server: ServerConnection.Key
  directory: string
  worktree?: string
}

export type Tab = SessionTab | DraftTab
```

- `DraftTab` 不含 `sessionId`，因为它尚未对应服务端 session；
- `draftID` 是前端生成的 UUID，是临时对话的唯一键；
- `directory` / `worktree` 记录该草稿所属项目目录与工作树，供后续创建 session 时使用。

### 2.2 路由类型

文件：`packages/app/src/context/layout.tsx`

```ts
export type LayoutRoute =
  | { type: "home" }
  | { type: "draft"; draftID: string; server?: ServerConnection.Key }
  | ...
```

`currentRoute()` 识别 `/new-session?draftId=...` 并返回 `{ type: "draft", draftID }`。如果 Tab 栏中找不到对应 draft，则路由被判定为无效并回退到首页。

### 2.3 Prompt 作用域

文件：`packages/app/src/context/prompt-state.ts`

```ts
export type PromptScope = { draftID: string } | { dir: string; id?: string }
```

- draft 对话的 prompt 状态使用 `{ draftID }` 作用域；
- `createDraftPromptSession(draftID, initial)` 把 prompt/model/context 持久化到 draft 专属存储；
- 真实 session 使用 `{ dir, id }` 作用域。

---

## 3. 生命周期

### 3.1 创建

文件：`packages/app/src/context/tabs.tsx`，`actions.newDraft`

```ts
async newDraft(draft: Omit<DraftTab, "type" | "draftID">, prompt?: string, model?: PromptModel) {
  const draftID = uuid()
  const tab = { type: "draft" as const, draftID, ...draft }
  memory.ensure(tabKey(tab), "prompt", () => createDraftPromptSession(draftID, { prompt, model }))
  await startTransition(() => {
    setStore(produce((tabs) => { tabs.push(tab) }))
    navigate(draftHref(draftID))
  })
  return tab
}
```

关键点：

- `draftID` 由 `uuid()` 生成；
- `memory.ensure(...)` 在 Tab 内存中创建 draft prompt session，并把其生命周期绑定到 Tab key；
- Tab 被 push 到持久化的 tabs store；
- 路由跳转到 `/new-session?draftId=<draftID>`。

创建入口包括：

| 入口 | 文件 |
| :--- | :--- |
| 首页"新建会话" | `packages/app/src/pages/home/home-controller.ts` `openProjectNewSession` / `openNewSession` |
| 标题栏"新建 Tab" | `packages/app/src/components/titlebar.tsx` `openNewTab` |
| 归档当前会话后 | `packages/app/src/pages/session/session-archive.ts` `navigateAfterRemoval` |
| 应用启动恢复 | `packages/app/src/app.tsx` `DraftRoute` |

### 3.2 路由与渲染

文件：`packages/app/src/app.tsx`

```ts
function DraftRoute() {
  ...
  <Show when={tabs.store.find((tab) => tab.type === "draft" && tab.draftID === search.draftId)} ...>
    {(draft) => <ResolvedDraftRoute draft={draft} />}
  </Show>
}
```

- 若 tabs 中不存在该 draft，回退到首页；
- `ResolvedDraftRoute` 根据 draft 的 `server` / `directory` 建立 Server SDK / Sync / Models 等 Provider；
- 最终渲染 `NewSession` 页面（`packages/app/src/pages/new-session.tsx`）；
- 页面注释明确："draft-only V2 session page. Submitting promotes the draft into a real session."

`NewSession` 页面组合：

- `createNewSessionWorkspaceController()`：工作树选择（main / create / 已有 worktree）；
- `createNewSessionDraftController()`：prompt 输入、模型选择、项目控制；
- `NewSessionView`：视图渲染。

### 3.3 状态持久化

文件：`packages/app/src/utils/persist.ts`

draft 存储使用独立的 draft storage：

```ts
function draftStorage(draftID: string) {
  const head = (draftID.slice(0, 12) || "draft").replace(/[^a-zA-Z0-9._-]/g, "-")
  const sum = checksum(draftID) ?? "0"
  return `opencode.draft.${head}.${sum}.dat`
}
```

```ts
Persist.draft(draftID, key, legacy?) // storage = draftStorage(draftID), key = `draft:${key}`
Persist.prompt(target)               // { ...target, draft: true }
```

持久化键集合：

```ts
const DRAFT_PERSISTED_KEYS = ["prompt", "comments", "file-view", "layout"]
```

- 当 `target.draft === true` 且平台提供 `draftStore`（桌面端）时，读写走 `platform.draftStore`；否则走 localStorage；
- `createDraftPromptSession` 使用 `Persist.prompt(Persist.draft(draftID, "prompt"))`，因此 prompt/model/context 会被持久化；
- 其他 comments / file-view / layout 也通过相同 draft storage 持久化。

这意味着临时对话的输入内容与上下文在页面刷新、Tab 关闭前可恢复。

清理函数：

```ts
removeDraftPersisted(draftID) // tabs.tsx
```

遍历 `draftPersistedKeys()`，删除对应 draft storage 下的 prompt/comments/file-view/layout。

### 3.4 提交晋升为真实会话

文件：`packages/app/src/components/prompt-input/submit.ts`

提交流程中，若当前没有 session 且是新建会话：

1. 根据工作树选择创建/选择 session directory；
2. 调用 `sdk().api.session.create(...)` 创建服务端 session；
3. `seed(sessionDirectory, created)` 把 session 写入 sync cache；
4. `local.session.promote(sessionDirectory, session.id, { agent, model, variant })` 保存 agent/model 选择；
5. `layout.handoff.setTabs(...)` 交接 Tab 状态；
6. 若 URL 中有 `draftId`：

   ```ts
   const draftID = search.draftId
   if (draftID) tabs.promoteDraft(draftID, { server: tabs.draft(draftID).server, sessionId: session.id })
   ```

7. 否则直接导航到 session 路由；
8. `submission.retarget(...)` 把当前提交目标切换到新 session。

文件：`packages/app/src/context/tabs.tsx`，`promoteDraft`

```ts
promoteDraft(draftID, session) {
  const active = location.pathname === "/new-session" && location.query.draftId === draftID
  const next = { type: "session", ...session }
  void startTransition(() => {
    setStore(produce((tabs) => {
      const index = tabs.findIndex((tab) => tab.type === "draft" && tab.draftID === draftID)
      if (index !== -1) tabs[index] = next
    }))
    if (recent.key === `draft:${draftID}`) setRecentKey(tabKey(next))
    if (active) navigateTab(next)
  })
  memory.remove(`draft:${draftID}`)
  removeDraftPersisted(draftID)
}
```

晋升效果：

- 原 `DraftTab` 在 Tab 栏中被替换为 `SessionTab`；
- 若当前正在该 draft 页面，则导航到新 session 路由；
- 释放 draft 的 Tab 内存（`memory.remove`）；
- 删除 draft 专属持久化状态（`removeDraftPersisted`）。

### 3.5 关闭与清理

文件：`packages/app/src/context/tabs.tsx`，`removeTab`

关闭 draft Tab 时：

```ts
const draftID = tab.type === "draft" ? tab.draftID : undefined
...
memory.remove(key)
removeInfo(key)
if (draftID) removeDraftPersisted(draftID)
```

- 从 tabs store 移除；
- 释放内存；
- 删除 draft 持久化状态；
- 若关闭后无 Tab，则导航回首页。

### 3.6 Tab 迁移与服务端清理

- `migrateTabs()`（`packages/app/src/context/tab-migration.ts`）在读取持久化 tabs 时校验 draft 字段（`draftID`、`directory`、可选 `worktree`）；
- `tabs.tsx` 的 effect 会过滤掉属于已移除服务端的 Tab，并同步清理 `info` / `memory`；
- 服务端移除或 session 缺失时，draft 不会自动晋升，仍可作为临时对话保留或被用户关闭。

---

## 4. 临时对话与真实会话的对比

| 维度 | 临时对话（Draft） | 真实会话（Session） |
| :--- | :--- | :--- |
| 标识 | `draftID`（前端 UUID） | `sessionId`（服务端 ID） |
| Tab 类型 | `DraftTab` | `SessionTab` |
| 路由 | `/new-session?draftId=...` | `/<dir>/session/<id>` |
| 服务端状态 | 无 | 已创建 session |
| Prompt 存储作用域 | `{ draftID }` | `{ dir, id }` |
| 持久化存储 | draft 专属 storage | server/workspace storage |
| 生命周期终点 | 提交第一条消息后晋升，或关闭时清理 | 归档/删除 |

---

## 5. 关键文件索引

| 文件 | 作用 |
| :--- | :--- |
| `packages/app/src/context/tabs.tsx` | `DraftTab` 定义、`newDraft`、`promoteDraft`、`removeTab`、`removeDraftPersisted` |
| `packages/app/src/context/prompt-state.ts` | `PromptScope`、`createDraftPromptSession` |
| `packages/app/src/context/tab-memory.ts` | Tab 级内存状态管理 |
| `packages/app/src/context/tab-migration.ts` | 持久化 tabs 迁移/校验 |
| `packages/app/src/context/layout.tsx` | `LayoutRoute` draft 分支、`currentRoute` |
| `packages/app/src/app.tsx` | `DraftRoute` / `ResolvedDraftRoute` |
| `packages/app/src/pages/new-session.tsx` | 临时对话页面入口 |
| `packages/app/src/pages/new-session/new-session-draft-controller.ts` | 临时对话的 prompt/model 控制器 |
| `packages/app/src/pages/new-session/new-session-workspace-controller.ts` | 工作树选择 |
| `packages/app/src/components/prompt-input/submit.ts` | 提交时创建 session 并晋升 draft |
| `packages/app/src/utils/persist.ts` | `Persist.draft`、`draftStorage`、`draftPersistedKeys` |
| `packages/app/src/pages/home/home-controller.ts` | 首页新建临时对话 |
| `packages/app/src/components/titlebar.tsx` | 标题栏新建临时对话 Tab |
| `packages/app/src/pages/session/session-archive.ts` | 归档后进入临时对话 |

---

## 6. 对 wf-agent 的借鉴点

- **延迟创建服务端会话**：先在前端创建 draft，避免每次"新建"都产生空 session；
- **Tab 级持久化**：draft 的输入/模型/上下文使用独立 storage，刷新不丢失；
- **晋升原子性**：`promoteDraft` 在同一 transition 中替换 Tab + 导航 + 清理，避免中间态；
- **内存与持久化协同**：`tab-memory` 管理运行时对象，`persist` 管理可恢复状态，关闭时双方同步清理。
