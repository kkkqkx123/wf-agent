# Prompt Tree 设计思想分析

本文档对 Prompt Tree 项目的核心设计思想进行系统性的分析总结。

## 一、核心理念

### 1.1 从线性对话到树形对话

传统的 AI 聊天产品采用**线性对话模式**：用户发送消息 -> AI 回复 -> 用户继续发送消息 -> ...，对话呈单向线性延伸。

Prompt Tree 的核心创新在于将对话建模为**节点化的 DAG（树 + 分支）结构**：

- 每条消息都是图中的一个**节点（Node）**
- 节点之间通过 `parentId` 建立父子关系，形成树形结构
- 用户可以从任意历史节点创建新分支，实现对比实验
- 同一父节点可以有多个子节点（多分支）

### 1.2 显式上下文管理

传统聊天产品的上下文管理是**隐式**的：发送给模型的完整历史对话，开发者难以精细控制。

Prompt Tree 引入 **Context Box（上下文组装台）**的概念：

- Context Box 是一个独立的容器，包含多个 **ContextBlock**
- ContextBlock 可以是 **NodeBlock**（引用树中的节点）或 **FileBlock**（文件内容）
- 用户可以**显式地**将哪些节点加入 Context Box
- 可以**拖拽排序**、**预览最终上下文**、**查看 Token 占用**
- 实现上下文管理的**可控性**和**可审计性**

## 二、核心数据结构

### 2.1 ConversationTree（对话树）

```typescript
interface ConversationTree {
  id: string;           // 树唯一标识
  rootId: string;       // 根节点 ID（通常是 SYSTEM 节点）
  title: string;        // 对话标题
  folderId?: string | null;  // 所属文件夹
  createdAt: number;    // 创建时间戳
  updatedAt: number;    // 更新时间戳
}
```

**设计要点**：
- 树本身仅存储元数据，不存储实际内容
- 通过 `rootId` 关联根节点，根节点通常是一个 SYSTEM 节点
- `folderId` 支持文件夹组织管理

### 2.2 Node（节点）

```typescript
interface Node {
  id: string;
  type: NodeType;       // USER | ASSISTANT | SYSTEM | COMPRESSED
  createdAt: number;
  updatedAt: number;
  parentId: string | null;  // 父节点 ID，null 表示根节点
  content: string;      // 消息内容
  summary?: string;     // 摘要（用于压缩节点）
  metadata: NodeMetadata;  // 元数据（标签、元指令、模型信息等）
  tokenCount: number;   // Token 计数
  position?: NodePosition;  // 画布位置
  style?: CSSProperties;    // 样式（画布渲染用）
}
```

**设计要点**：
- 通过 `parentId` 形成树形结构
- `COMPRESSED` 类型用于表示被压缩的连续消息序列
- `metadata` 支持丰富的扩展信息（标签、元指令、工具日志等）
- `tokenCount` 支持精细的上下文大小管理

### 2.3 ContextBox（上下文盒子）

```typescript
interface ContextBox {
  id: string;           // 与 Tree ID 对应（一对一关系）
  blocks: ContextBlock[];  // 上下文块列表
  totalTokens: number;  // 当前总 Token 数
  maxTokens: number;    // 最大 Token 上限
  createdAt: number;
}

type ContextBlock = ContextNodeBlock | ContextFileBlock;

interface ContextNodeBlock {
  id: string;
  kind: "node";
  nodeId: string;       // 引用树中的节点
}

interface ContextTextFileBlock {
  id: string;
  kind: "file";
  fileKind: "text" | "markdown" | "pdf";
  filename: string;
  content: string;
  tokenCount: number;
  truncated: boolean;
}
```

**设计要点**：
- ContextBox 与 Tree 一对一关系，每个树有独立的上下文
- blocks 是有序列表，支持拖拽排序
- 支持混合引用（节点 + 文件）
- `totalTokens` 实时追踪上下文大小

## 三、架构设计思想

### 3.1 分层架构

Prompt Tree 采用清晰的分层架构：

```
┌─────────────────────────────────────────┐
│            UI Layer (React)              │
│  useTree / useNode / useContext Hooks    │
├─────────────────────────────────────────┤
│         State Layer (Zustand)           │
│   treeSlice / nodeSlice / llmSlice     │
├─────────────────────────────────────────┤
│         Service Layer (Business Logic)   │
│  TreeService / NodeService / ContextBoxService │
├─────────────────────────────────────────┤
│           Storage Layer (IndexedDB)      │
│           AIChatClientDB                 │
└─────────────────────────────────────────┘
```

**各层职责**：
- **UI Layer**：React 组件和 Hooks，提供状态消费接口
- **State Layer**：Zustand store，管理应用状态
- **Service Layer**：业务逻辑处理，数据转换
- **Storage Layer**：IndexedDB 持久化存储

### 3.2 服务化设计

每个核心实体都有对应的 Service 类封装业务逻辑：

| Service | 职责 |
|---------|------|
| TreeService | 树的 CRUD、加载树节点、更新标题 |
| NodeService | 节点的 CRUD、获取子节点、获取路径、搜索 |
| ContextBoxService | 上下文的读取、更新、持久化 |

**设计优势**：
- 业务逻辑与状态管理分离
- 便于单元测试
- 支持依赖注入（Service 作为 deps 传入 store）

### 3.3 本地优先存储

```
┌─────────────────────────────────────────┐
│              IndexedDB                   │
│  ┌─────────┐  ┌─────────┐  ┌────────┐ │
│  │  trees  │  │  nodes  │  │context  │ │
│  │         │  │         │  │ boxes   │ │
│  └─────────┘  └─────────┘  └────────┘ │
│                                         │
│  ┌─────────┐  ┌──────────────────────┐ │
│  │ profiles│  │    memoryBank       │ │
│  └─────────┘  └──────────────────────┘ │
└─────────────────────────────────────────┘
```

**存储策略**：
- **对话数据**：IndexedDB，支持大容量存储
- **Provider 配置**：localStorage（不含敏感信息）
- **API Keys**：localStorage（用户自行管理）

## 四、关键设计模式

### 4.1 DAG 结构实现

虽然用户感知到的是"分支"，但底层通过**严格的树结构**实现：

```typescript
// BFS 遍历构建节点列表（treeService.loadTreeNodes）
const queue: string[] = [tree.rootId];
const seen = new Set<string>();

while (queue.length) {
  const id = queue.shift();
  if (!id || seen.has(id)) continue;
  seen.add(id);

  const node = allById.get(id);
  if (!node) continue;

  result.push(node);
  const children = nodesByParent.get(id) ?? [];
  for (const child of children) queue.push(child.id);
}
```

**关键点**：
- 每个节点只有一个 `parentId`（严格的树）
- 多分支通过"同一父节点有多个子节点"实现
- 加载时通过 BFS 保证节点顺序

### 4.2 Context Box 的动态组装

Context Box 支持灵活的上下文组装：

```typescript
// 同步节点到 Context Box
syncContextToNode: (nodeId) => {
  const node = get().nodes.get(nodeId);
  const box = get().contextBox;
  // 将节点添加到 blocks
}
```

**特性**：
- 节点与 Context Box 分离：节点存储在树中，Context Box 按需引用
- 支持拖拽排序
- 支持 Token 计数和上限控制
- 支持压缩整个 Context 或选中链路

### 4.3 压缩与解压机制

```typescript
enum NodeType {
  USER = "user",
  ASSISTANT = "assistant",
  SYSTEM = "system",
  COMPRESSED = "compressed",  // 压缩节点
}
```

**压缩流程**：
1. 选中连续的链路（必须是同一路径上的连续节点）
2. 将这些节点压缩为一个 `COMPRESSED` 类型的新节点
3. `summary` 字段存储压缩后的摘要
4. `compressedNodeIds` 记录被压缩的原始节点 ID

**解压流程**：
1. 将 COMPRESSED 节点的 `compressedNodeIds` 展开
2. 恢复原始节点或直接用 summary 内容

## 五、设计优势

### 5.1 分支对比能力

```
          [System]
              │
          [User:A]
           /     \
     [Assistant:B]  [Assistant:C]   <- 多分支
         |             |
       [User:D]      [User:E]        <- 继续分支
```

用户可以：
- 同时探索多个不同的回答方向
- 对比不同模型/提示词的效果
- 轻松回溯到任意节点重新开始

### 5.2 精细的上下文控制

- 不再是"发送全部历史"的粗放模式
- 可以选择性加入相关节点到 Context Box
- 实时预览和 Token 计数
- 支持文件作为上下文补充

### 5.3 可视化与可审计

- 画布展示节点结构
- Context Box 显示最终上下文
- 每次变更可追踪
- 支持压缩减少视觉复杂度

## 六、与传统聊天产品的区别

| 维度 | 传统产品 | Prompt Tree |
|------|----------|-------------|
| 对话结构 | 线性 | 树形（DAG） |
| 上下文管理 | 隐式，全量发送 | 显式，可控组装 |
| 分支支持 | 无或有限 | 原生支持 |
| 上下文预览 | 无 | Token 计数 + 预览 |
| 数据存储 | 服务器 | 本地（IndexedDB） |

## 七、总结

Prompt Tree 的设计思想可以概括为：

1. **结构化**：将线性对话演进为树形结构，支持自然的分支与对比
2. **可组合**：通过 Context Box 实现上下文的选择性组装
3. **可控制**：Token 计数、压缩机制、拖拽排序提供精细控制
4. **本地优先**：IndexedDB 存储，数据主权归用户
5. **可扩展**：NodeMetadata 支持丰富的元数据扩展

这一设计让 AI 对话从"你问我答"的简单交互，升级为"可探索、可对比、可管理"的复杂工作流。
