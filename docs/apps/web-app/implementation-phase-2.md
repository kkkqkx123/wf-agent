# Web App 实施方案 - 第二阶段

## 阶段目标

在第一阶段基础上,实现可视化编辑器、资源管理、检查点管理和事件监控等增强功能,提升用户体验和功能完整性。

## 一、工作流可视化编辑器

### 1.1 技术选型

- **图形库**: D3.js (用于节点和边的渲染)
- **布局算法**: Dagre (有向图布局)
- **交互处理**: D3-drag、D3-zoom

### 1.2 编辑器架构

#### 组件结构
```
src/lib/components/workflow/editor/
├── WorkflowEditor.svelte          # 主编辑器组件
├── Canvas.svelte                  # 画布组件
├── NodePalette.svelte             # 节点面板
├── Node.svelte                    # 节点组件
├── Edge.svelte                    # 边组件
├── PropertyPanel.svelte           # 属性面板
├── Toolbar.svelte                 # 工具栏
└── utils/
    ├── layout.ts                  # 布局计算
    ├── graph-utils.ts             # 图形工具
    └── validation.ts              # 验证工具
```

### 1.3 核心实现

#### 工作流编辑器 (WorkflowEditor.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import Canvas from './Canvas.svelte';
  import NodePalette from './NodePalette.svelte';
  import PropertyPanel from './PropertyPanel.svelte';
  import Toolbar from './Toolbar.svelte';
  import { workflowEditorStore } from '$lib/stores/workflow-editor';
  import type { Workflow, Node, Edge } from '@modular-agent/types';

  let { workflow } = $props<{ workflow: Workflow }>();
  
  let canvasRef: HTMLDivElement;
  let selectedNode: Node | null = $state(null);
  let selectedEdge: Edge | null = $state(null);

  onMount(() => {
    workflowEditorStore.initialize(workflow);
  });

  function handleNodeSelect(node: Node) {
    selectedNode = node;
    selectedEdge = null;
  }

  function handleEdgeSelect(edge: Edge) {
    selectedEdge = edge;
    selectedNode = null;
  }

  function handleNodeAdd(nodeType: string, position: { x: number; y: number }) {
    workflowEditorStore.addNode(nodeType, position);
  }

  function handleNodeMove(nodeId: string, position: { x: number; y: number }) {
    workflowEditorStore.updateNodePosition(nodeId, position);
  }

  function handleNodeDelete(nodeId: string) {
    workflowEditorStore.deleteNode(nodeId);
  }

  function handleEdgeCreate(sourceId: string, targetId: string) {
    workflowEditorStore.addEdge(sourceId, targetId);
  }

  function handleEdgeDelete(edgeId: string) {
    workflowEditorStore.deleteEdge(edgeId);
  }
</script>

<div class="flex h-full">
  <!-- 左侧节点面板 -->
  <NodePalette onAdd={handleNodeAdd} />
  
  <!-- 中间画布 -->
  <div class="flex-1 flex flex-col">
    <Toolbar />
    <div class="flex-1 relative" bind:this={canvasRef}>
      <Canvas
        nodes={$workflowEditorStore.nodes}
        edges={$workflowEditorStore.edges}
        onNodeSelect={handleNodeSelect}
        onEdgeSelect={handleEdgeSelect}
        onNodeMove={handleNodeMove}
        onEdgeCreate={handleEdgeCreate}
      />
    </div>
  </div>
  
  <!-- 右侧属性面板 -->
  <PropertyPanel
    selectedNode={selectedNode}
    selectedEdge={selectedEdge}
    onNodeUpdate={workflowEditorStore.updateNode}
    onEdgeUpdate={workflowEditorStore.updateEdge}
    onNodeDelete={handleNodeDelete}
    onEdgeDelete={handleEdgeDelete}
  />
</div>
```

#### 画布组件 (Canvas.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import * as d3 from 'd3';
  import Node from './Node.svelte';
  import Edge from './Edge.svelte';
  import type { Node as NodeType, Edge as EdgeType } from '@modular-agent/types';

  let { 
    nodes = [], 
    edges = [],
    onNodeSelect,
    onEdgeSelect,
    onNodeMove,
    onEdgeCreate
  } = $props();

  let svgRef: SVGSVGElement;
  let zoom = $state(1);
  let translate = $state({ x: 0, y: 0 });

  onMount(() => {
    const svg = d3.select(svgRef);
    
    // 设置缩放和平移
    const zoomBehavior = d3.zoom()
      .scaleExtent([0.1, 4])
      .on('zoom', (event) => {
        zoom = event.transform.k;
        translate = { x: event.transform.x, y: event.transform.y };
      });
    
    svg.call(zoomBehavior);
  });

  function handleNodeDrag(nodeId: string, event: DragEvent) {
    const position = {
      x: (event.x - translate.x) / zoom,
      y: (event.y - translate.y) / zoom
    };
    onNodeMove?.(nodeId, position);
  }
</script>

<svg bind:this={svgRef} class="w-full h-full bg-gray-50">
  <g transform="translate({translate.x}, {translate.y}) scale({zoom})">
    <!-- 渲染边 -->
    {#each edges as edge}
      <Edge {edge} onClick={() => onEdgeSelect?.(edge)} />
    {/each}
    
    <!-- 渲染节点 -->
    {#each nodes as node}
      <Node 
        {node} 
        onClick={() => onNodeSelect?.(node)}
        onDrag={(e) => handleNodeDrag(node.id, e)}
      />
    {/each}
  </g>
</svg>
```

#### 节点组件 (Node.svelte)
```svelte
<script lang="ts">
  import * as d3 from 'd3';
  import type { Node } from '@modular-agent/types';

  let { node, onClick, onDrag } = $props();
  
  let nodeRef: SVGGElement;
  let isDragging = $state(false);

  onMount(() => {
    const drag = d3.drag()
      .on('start', () => {
        isDragging = true;
      })
      .on('drag', (event) => {
        onDrag?.(event);
      })
      .on('end', () => {
        isDragging = false;
      });
    
    d3.select(nodeRef).call(drag);
  });
</script>

<g bind:this={nodeRef} transform="translate({node.position.x}, {node.position.y})">
  <!-- 节点背景 -->
  <rect
    x="-60"
    y="-30"
    width="120"
    height="60"
    rx="8"
    fill="white"
    stroke={isDragging ? '#3b82f6' : '#e5e7eb'}
    stroke-width={isDragging ? 2 : 1}
    on:click={onClick}
  />
  
  <!-- 节点图标 -->
  <text
    x="0"
    y="-5"
    text-anchor="middle"
    font-size="12"
    fill="#374151"
  >
    {node.type}
  </text>
  
  <!-- 节点名称 -->
  <text
    x="0"
    y="10"
    text-anchor="middle"
    font-size="10"
    fill="#6b7280"
  >
    {node.name}
  </text>
  
  <!-- 输入连接点 -->
  <circle cx="-60" cy="0" r="5" fill="#3b82f6" class="cursor-pointer" />
  
  <!-- 输出连接点 -->
  <circle cx="60" cy="0" r="5" fill="#3b82f6" class="cursor-pointer" />
</g>
```

### 1.4 布局算法

#### 自动布局 (utils/layout.ts)
```typescript
import * as dagre from 'dagre';
import type { Node, Edge } from '@modular-agent/types';

export function calculateLayout(nodes: Node[], edges: Edge[]): Map<string, { x: number; y: number }> {
  const g = new dagre.graphlib.Graph();
  
  // 设置图参数
  g.setGraph({
    rankdir: 'TB',  // 从上到下布局
    nodesep: 80,
    ranksep: 100,
    marginx: 50,
    marginy: 50,
  });
  
  // 设置默认节点参数
  g.setDefaultEdgeLabel(() => ({}));
  
  // 添加节点
  nodes.forEach(node => {
    g.setNode(node.id, {
      width: 120,
      height: 60,
    });
  });
  
  // 添加边
  edges.forEach(edge => {
    g.setEdge(edge.source, edge.target);
  });
  
  // 计算布局
  dagre.layout(g);
  
  // 提取节点位置
  const positions = new Map<string, { x: number; y: number }>();
  nodes.forEach(node => {
    const pos = g.node(node.id);
    positions.set(node.id, { x: pos.x, y: pos.y });
  });
  
  return positions;
}
```

## 二、线程执行流程可视化

### 2.1 执行流程图

#### 组件结构
```
src/lib/components/thread/execution-flow/
├── ExecutionFlow.svelte           # 执行流程主组件
├── FlowNode.svelte                # 流程节点
├── FlowEdge.svelte                # 流程边
├── Timeline.svelte                # 时间线
└── ExecutionStats.svelte          # 执行统计
```

#### 执行流程组件 (ExecutionFlow.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import FlowNode from './FlowNode.svelte';
  import FlowEdge from './FlowEdge.svelte';
  import Timeline from './Timeline.svelte';
  import ExecutionStats from './ExecutionStats.svelte';
  import type { Thread, ExecutionStep } from '@modular-agent/types';

  let { thread } = $props<{ thread: Thread }>();
  
  let steps = $state<ExecutionStep[]>([]);
  let currentStepIndex = $state(-1);

  onMount(() => {
    // 订阅执行步骤更新
    // sseClient.subscribe('thread:step', handleStepUpdate);
  });

  function handleStepUpdate(step: ExecutionStep) {
    steps = [...steps, step];
    currentStepIndex = steps.length - 1;
  }

  function getNodeStatus(nodeId: string): 'pending' | 'running' | 'completed' | 'failed' {
    const step = steps.find(s => s.nodeId === nodeId);
    if (!step) return 'pending';
    return step.status;
  }
</script>

<div class="flex flex-col h-full">
  <!-- 执行统计 -->
  <ExecutionStats steps={steps} />
  
  <!-- 流程图 -->
  <div class="flex-1 relative">
    <svg class="w-full h-full">
      <!-- 渲染边 -->
      {#each thread.workflow.edges as edge}
        <FlowEdge {edge} />
      {/each}
      
      <!-- 渲染节点 -->
      {#each thread.workflow.nodes as node}
        <FlowNode 
          {node} 
          status={getNodeStatus(node.id)}
          isCurrent={steps[currentStepIndex]?.nodeId === node.id}
        />
      {/each}
    </svg>
  </div>
  
  <!-- 时间线 -->
  <Timeline {steps} bind:currentIndex={currentStepIndex} />
</div>
```

#### 流程节点 (FlowNode.svelte)
```svelte
<script lang="ts">
  import type { Node } from '@modular-agent/types';

  let { 
    node, 
    status = 'pending',
    isCurrent = false
  } = $props<{
    node: Node;
    status?: 'pending' | 'running' | 'completed' | 'failed';
    isCurrent?: boolean;
  }>();

  const statusColors = {
    pending: '#e5e7eb',
    running: '#3b82f6',
    completed: '#10b981',
    failed: '#ef4444',
  };
</script>

<g transform="translate({node.position.x}, {node.position.y})">
  <!-- 节点背景 -->
  <rect
    x="-60"
    y="-30"
    width="120"
    height="60"
    rx="8"
    fill="white"
    stroke={statusColors[status]}
    stroke-width={isCurrent ? 3 : 2}
  />
  
  <!-- 运行中动画 -->
  {#if status === 'running'}
    <circle cx="0" cy="0" r="20" fill="none" stroke="#3b82f6" stroke-width="2">
      <animateTransform
        attributeName="transform"
        type="rotate"
        from="0 0 0"
        to="360 0 0"
        dur="1s"
        repeatCount="indefinite"
      />
    </circle>
  {/if}
  
  <!-- 节点名称 -->
  <text
    x="0"
    y="5"
    text-anchor="middle"
    font-size="12"
    fill="#374151"
  >
    {node.name}
  </text>
</g>
```

## 三、资源管理功能

### 3.1 工具管理

#### 工具列表页 (src/routes/tools/+page.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { toolStore } from '$lib/stores/tool';
  import { ToolAdapter } from '$lib/adapters/tool-adapter';
  import ToolCard from '$lib/components/tools/ToolCard.svelte';

  const adapter = new ToolAdapter();
  
  onMount(async () => {
    toolStore.setLoading(true);
    try {
      const tools = await adapter.listTools();
      toolStore.setTools(tools);
    } catch (error) {
      toolStore.setError(error.message);
    }
  });
</script>

<div class="space-y-6">
  <h1 class="text-2xl font-bold">工具管理</h1>
  
  {#if $toolStore.loading}
    <div>Loading...</div>
  {:else}
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      {#each $toolStore.tools as tool}
        <ToolCard {tool} />
      {/each}
    </div>
  {/if}
</div>
```

#### 工具配置对话框
```svelte
<script lang="ts">
  import Modal from '$lib/components/common/Modal.svelte';
  import type { Tool } from '@modular-agent/types';

  let { tool, isOpen, onClose, onSave } = $props<{
    tool: Tool;
    isOpen: boolean;
    onClose: () => void;
    onSave: (tool: Tool) => void;
  }>();

  let editedTool = $state({ ...tool });

  function handleSave() {
    onSave(editedTool);
    onClose();
  }
</script>

<Modal {isOpen} {onClose} title="工具配置">
  <div class="space-y-4">
    <div>
      <label class="block text-sm font-medium mb-1">工具名称</label>
      <input 
        type="text" 
        bind:value={editedTool.name}
        class="w-full px-3 py-2 border rounded"
      />
    </div>
    
    <div>
      <label class="block text-sm font-medium mb-1">描述</label>
      <textarea 
        bind:value={editedTool.description}
        class="w-full px-3 py-2 border rounded"
        rows="3"
      />
    </div>
    
    <div>
      <label class="flex items-center">
        <input type="checkbox" bind:checked={editedTool.enabled} />
        <span class="ml-2">启用</span>
      </label>
    </div>
    
    <div>
      <label class="flex items-center">
        <input type="checkbox" bind:checked={editedTool.autoExecute} />
        <span class="ml-2">自动执行</span>
      </label>
    </div>
    
    <div class="flex justify-end space-x-2">
      <button on:click={onClose} class="px-4 py-2 border rounded">取消</button>
      <button on:click={handleSave} class="px-4 py-2 bg-blue-500 text-white rounded">保存</button>
    </div>
  </div>
</Modal>
```

### 3.2 LLM Profile 管理

#### Profile 列表页 (src/routes/profiles/+page.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { profileStore } from '$lib/stores/profile';
  import { ProfileAdapter } from '$lib/adapters/profile-adapter';
  import ProfileCard from '$lib/components/profiles/ProfileCard.svelte';
  import CreateProfileModal from '$lib/components/profiles/CreateProfileModal.svelte';

  const adapter = new ProfileAdapter();
  
  let showCreateModal = $state(false);

  onMount(async () => {
    profileStore.setLoading(true);
    try {
      const profiles = await adapter.listProfiles();
      profileStore.setProfiles(profiles);
    } catch (error) {
      profileStore.setError(error.message);
    }
  });

  async function handleCreateProfile(profile: Profile) {
    const newProfile = await adapter.createProfile(profile);
    profileStore.addProfile(newProfile);
  }
</script>

<div class="space-y-6">
  <div class="flex justify-between items-center">
    <h1 class="text-2xl font-bold">LLM Profile 管理</h1>
    <button 
      on:click={() => showCreateModal = true}
      class="px-4 py-2 bg-blue-500 text-white rounded"
    >
      创建 Profile
    </button>
  </div>
  
  {#if $profileStore.loading}
    <div>Loading...</div>
  {:else}
    <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
      {#each $profileStore.profiles as profile}
        <ProfileCard {profile} />
      {/each}
    </div>
  {/if}
</div>

<CreateProfileModal 
  isOpen={showCreateModal}
  onClose={() => showCreateModal = false}
  onCreate={handleCreateProfile}
/>
```

### 3.3 脚本管理

#### 脚本编辑器
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import CodeEditor from '$lib/components/common/CodeEditor.svelte';
  import type { Script } from '@modular-agent/types';

  let { script, onSave } = $props<{
    script: Script;
    onSave: (script: Script) => void;
  }>();

  let code = $state(script.code);
  let parameters = $state(script.parameters);

  function handleSave() {
    onSave({
      ...script,
      code,
      parameters,
    });
  }

  async function handleTest() {
    // 执行脚本测试
  }
</script>

<div class="flex h-full">
  <!-- 左侧代码编辑器 -->
  <div class="flex-1">
    <CodeEditor 
      bind:value={code}
      language="typescript"
    />
  </div>
  
  <!-- 右侧参数面板 -->
  <div class="w-80 border-l p-4">
    <h3 class="font-bold mb-4">参数定义</h3>
    <!-- 参数编辑 -->
    
    <div class="mt-4 space-x-2">
      <button on:click={handleTest} class="px-4 py-2 border rounded">测试</button>
      <button on:click={handleSave} class="px-4 py-2 bg-blue-500 text-white rounded">保存</button>
    </div>
  </div>
</div>
```

## 四、检查点管理

### 4.1 检查点列表

#### 检查点列表页 (src/routes/checkpoints/+page.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { checkpointStore } from '$lib/stores/checkpoint';
  import { CheckpointAdapter } from '$lib/adapters/checkpoint-adapter';
  import CheckpointCard from '$lib/components/checkpoints/CheckpointCard.svelte';

  const adapter = new CheckpointAdapter();
  
  onMount(async () => {
    checkpointStore.setLoading(true);
    try {
      const checkpoints = await adapter.listCheckpoints();
      checkpointStore.setCheckpoints(checkpoints);
    } catch (error) {
      checkpointStore.setError(error.message);
    }
  });

  async function handleRestore(checkpointId: string) {
    await adapter.restoreCheckpoint(checkpointId);
    // 跳转到恢复的线程
  }
</script>

<div class="space-y-6">
  <h1 class="text-2xl font-bold">检查点管理</h1>
  
  {#if $checkpointStore.loading}
    <div>Loading...</div>
  {:else}
    <div class="space-y-4">
      {#each $checkpointStore.checkpoints as checkpoint}
        <CheckpointCard 
          {checkpoint}
          onRestore={() => handleRestore(checkpoint.id)}
        />
      {/each}
    </div>
  {/if}
</div>
```

### 4.2 检查点详情

#### 检查点详情对话框
```svelte
<script lang="ts">
  import Modal from '$lib/components/common/Modal.svelte';
  import JsonViewer from '$lib/components/common/JsonViewer.svelte';
  import type { Checkpoint } from '@modular-agent/types';

  let { checkpoint, isOpen, onClose } = $props<{
    checkpoint: Checkpoint;
    isOpen: boolean;
    onClose: () => void;
  }>();
</script>

<Modal {isOpen} {onClose} title="检查点详情" size="large">
  <div class="space-y-4">
    <div>
      <h3 class="font-bold mb-2">基本信息</h3>
      <div class="grid grid-cols-2 gap-2">
        <div>ID: {checkpoint.id}</div>
        <div>创建时间: {new Date(checkpoint.createdAt).toLocaleString()}</div>
        <div>关联线程: {checkpoint.threadId}</div>
        <div>大小: {checkpoint.size} bytes</div>
      </div>
    </div>
    
    <div>
      <h3 class="font-bold mb-2">状态快照</h3>
      <JsonViewer data={checkpoint.state} />
    </div>
    
    <div>
      <h3 class="font-bold mb-2">消息历史</h3>
      <div class="max-h-64 overflow-auto">
        {#each checkpoint.messages as message}
          <div class="border-b py-2">
            <div class="font-medium">{message.role}</div>
            <div class="text-sm text-gray-600">{message.content}</div>
          </div>
        {/each}
      </div>
    </div>
  </div>
</Modal>
```

## 五、事件监控

### 5.1 事件流展示

#### 事件监控页 (src/routes/events/+page.svelte)
```svelte
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { eventStore } from '$lib/stores/event';
  import { sseClient } from '$lib/services/sse-client';
  import EventStream from '$lib/components/events/EventStream.svelte';
  import EventFilter from '$lib/components/events/EventFilter.svelte';
  import EventStats from '$lib/components/events/EventStats.svelte';

  let filter = $state({
    types: [],
    startTime: null,
    endTime: null,
  });

  let unsubscribe: (() => void) | null = null;

  onMount(() => {
    sseClient.connect('/api/events', ['event', 'thread:*', 'node:*', 'tool:*']);
    unsubscribe = sseClient.subscribe('event', (event) => {
      eventStore.addEvent(event);
    });
  });

  onDestroy(() => {
    if (unsubscribe) unsubscribe();
    sseClient.disconnect();
  });

  function handleFilterChange(newFilter: EventFilter) {
    filter = newFilter;
    eventStore.setFilter(newFilter);
  }
</script>

<div class="flex h-full">
  <!-- 左侧事件流 -->
  <div class="flex-1 flex flex-col">
    <EventFilter onFilter={handleFilterChange} />
    <EventStream events={$eventStore.filteredEvents} />
  </div>
  
  <!-- 右侧统计 -->
  <div class="w-80 border-l p-4">
    <EventStats events={$eventStore.events} />
  </div>
</div>
```

#### 事件流组件 (EventStream.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import type { Event } from '@modular-agent/types';

  let { events = [] } = $props<{ events: Event[] }>();
  
  let containerRef: HTMLDivElement;
  let autoScroll = $state(true);

  onMount(() => {
    if (autoScroll && containerRef) {
      containerRef.scrollTop = containerRef.scrollHeight;
    }
  });

  function getEventColor(type: string): string {
    const colors: Record<string, string> = {
      'thread:started': 'bg-blue-100',
      'thread:completed': 'bg-green-100',
      'thread:error': 'bg-red-100',
      'node:executed': 'bg-purple-100',
      'tool:called': 'bg-yellow-100',
    };
    return colors[type] || 'bg-gray-100';
  }
</script>

<div bind:this={containerRef} class="flex-1 overflow-auto p-4 space-y-2">
  {#each events as event}
    <div class="p-3 rounded {getEventColor(event.type)}">
      <div class="flex justify-between items-start">
        <div class="font-medium">{event.type}</div>
        <div class="text-sm text-gray-500">
          {new Date(event.timestamp).toLocaleTimeString()}
        </div>
      </div>
      <div class="text-sm text-gray-600 mt-1">
        {JSON.stringify(event.data)}
      </div>
    </div>
  {/each}
</div>
```

## 六、开发任务清单

### 6.1 可视化编辑器
- [ ] 集成 D3.js
- [ ] 实现画布组件(缩放、平移)
- [ ] 实现节点组件(拖拽、选择)
- [ ] 实现边组件(连线)
- [ ] 实现节点面板
- [ ] 实现属性面板
- [ ] 实现工具栏
- [ ] 实现自动布局算法
- [ ] 实现保存和加载

### 6.2 执行流程可视化
- [ ] 实现执行流程图
- [ ] 实现节点状态展示
- [ ] 实现执行动画
- [ ] 实现时间线组件
- [ ] 实现执行统计

### 6.3 资源管理
- [ ] 实现工具管理页面
- [ ] 实现工具配置对话框
- [ ] 实现 Profile 管理页面
- [ ] 实现 Profile 创建对话框
- [ ] 实现脚本管理页面
- [ ] 实现脚本编辑器
- [ ] 实现 Skill 管理页面

### 6.4 检查点管理
- [ ] 实现检查点列表页面
- [ ] 实现检查点详情对话框
- [ ] 实现检查点恢复功能
- [ ] 实现检查点对比功能

### 6.5 事件监控
- [ ] 实现事件流展示
- [ ] 实现事件过滤
- [ ] 实现事件统计
- [ ] 实现实时订阅

## 七、验收标准

### 7.1 功能验收
- ✅ 工作流可视化编辑器可用
- ✅ 支持节点拖拽和连线
- ✅ 支持自动布局
- ✅ 线程执行流程可视化正常
- ✅ 工具管理功能完整
- ✅ Profile 管理功能完整
- ✅ 脚本管理功能完整
- ✅ 检查点管理功能完整
- ✅ 事件监控功能完整

### 7.2 技术验收
- ✅ D3.js 集成正常
- ✅ 图形渲染性能良好
- ✅ 实时事件推送正常
- ✅ 代码类型检查通过

## 八、时间估算

- 可视化编辑器: 5 天
- 执行流程可视化: 3 天
- 资源管理: 4 天
- 检查点管理: 2 天
- 事件监控: 2 天
- **总计: 约 16 天**

## 九、下一步计划

完成第二阶段后,进入第三阶段:
- 实现触发器管理
- 实现变量管理
- 实现 Human Relay
- 实现高级可视化(统计图表)
- 性能优化
