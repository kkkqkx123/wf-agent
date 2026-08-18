# Web App 实施方案 - 第三阶段

## 阶段目标

在第二阶段基础上,实现触发器管理、变量管理、Human Relay、高级可视化和性能优化等增强功能,进一步完善系统的功能完整性和用户体验。

## 一、触发器管理

### 1.1 触发器配置界面

#### 触发器列表页 (src/routes/triggers/+page.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { triggerStore } from '$lib/stores/trigger';
  import { TriggerAdapter } from '$lib/adapters/trigger-adapter';
  import TriggerCard from '$lib/components/triggers/TriggerCard.svelte';
  import CreateTriggerModal from '$lib/components/triggers/CreateTriggerModal.svelte';

  const adapter = new TriggerAdapter();
  
  let showCreateModal = $state(false);
  let filterType = $state<string>('all');

  onMount(async () => {
    triggerStore.setLoading(true);
    try {
      const triggers = await adapter.listTriggers();
      triggerStore.setTriggers(triggers);
    } catch (error) {
      triggerStore.setError(error.message);
    }
  });

  async function handleCreateTrigger(trigger: Trigger) {
    const newTrigger = await adapter.createTrigger(trigger);
    triggerStore.addTrigger(newTrigger);
  }

  async function handleToggleTrigger(id: string, enabled: boolean) {
    await adapter.updateTrigger(id, { enabled });
    triggerStore.updateTrigger(id, { enabled });
  }
</script>

<div class="space-y-6">
  <div class="flex justify-between items-center">
    <h1 class="text-2xl font-bold">触发器管理</h1>
    <button 
      on:click={() => showCreateModal = true}
      class="px-4 py-2 bg-blue-500 text-white rounded"
    >
      创建触发器
    </button>
  </div>
  
  <!-- 类型筛选 -->
  <div class="flex space-x-2">
    <button 
      on:click={() => filterType = 'all'}
      class="px-3 py-1 rounded {filterType === 'all' ? 'bg-blue-500 text-white' : 'bg-gray-200'}"
    >
      全部
    </button>
    <button 
      on:click={() => filterType = 'event'}
      class="px-3 py-1 rounded {filterType === 'event' ? 'bg-blue-500 text-white' : 'bg-gray-200'}"
    >
      事件触发器
    </button>
    <button 
      on:click={() => filterType = 'schedule'}
      class="px-3 py-1 rounded {filterType === 'schedule' ? 'bg-blue-500 text-white' : 'bg-gray-200'}"
    >
      定时触发器
    </button>
  </div>
  
  {#if $triggerStore.loading}
    <div>Loading...</div>
  {:else}
    <div class="space-y-4">
      {#each $triggerStore.triggers.filter(t => filterType === 'all' || t.type === filterType) as trigger}
        <TriggerCard 
          {trigger}
          onToggle={() => handleToggleTrigger(trigger.id, !trigger.enabled)}
        />
      {/each}
    </div>
  {/if}
</div>

<CreateTriggerModal 
  isOpen={showCreateModal}
  onClose={() => showCreateModal = false}
  onCreate={handleCreateTrigger}
/>
```

### 1.2 触发器创建向导

#### 创建触发器对话框 (CreateTriggerModal.svelte)
```svelte
<script lang="ts">
  import Modal from '$lib/components/common/Modal.svelte';
  import TriggerConditionEditor from './TriggerConditionEditor.svelte';
  import TriggerActionEditor from './TriggerActionEditor.svelte';
  import type { Trigger, TriggerType } from '@modular-agent/types';

  let { isOpen, onClose, onCreate } = $props<{
    isOpen: boolean;
    onClose: () => void;
    onCreate: (trigger: Trigger) => void;
  }>();

  let step = $state(1);
  let triggerType = $state<TriggerType>('event');
  let triggerName = $state('');
  let condition = $state({});
  let action = $state({});

  function handleNext() {
    if (step < 3) {
      step++;
    }
  }

  function handleBack() {
    if (step > 1) {
      step--;
    }
  }

  function handleCreate() {
    onCreate({
      name: triggerName,
      type: triggerType,
      condition,
      action,
      enabled: true,
    });
    onClose();
  }
</script>

<Modal {isOpen} {onClose} title="创建触发器" size="large">
  <div class="space-y-6">
    <!-- 步骤指示器 -->
    <div class="flex items-center justify-center space-x-4">
      <div class="flex items-center">
        <div class="w-8 h-8 rounded-full flex items-center justify-center {step >= 1 ? 'bg-blue-500 text-white' : 'bg-gray-300'}">
          1
        </div>
        <span class="ml-2">基本信息</span>
      </div>
      <div class="w-8 h-0.5 bg-gray-300"></div>
      <div class="flex items-center">
        <div class="w-8 h-8 rounded-full flex items-center justify-center {step >= 2 ? 'bg-blue-500 text-white' : 'bg-gray-300'}">
          2
        </div>
        <span class="ml-2">触发条件</span>
      </div>
      <div class="w-8 h-0.5 bg-gray-300"></div>
      <div class="flex items-center">
        <div class="w-8 h-8 rounded-full flex items-center justify-center {step >= 3 ? 'bg-blue-500 text-white' : 'bg-gray-300'}">
          3
        </div>
        <span class="ml-2">触发动作</span>
      </div>
    </div>
    
    <!-- 步骤内容 -->
    {#if step === 1}
      <div class="space-y-4">
        <div>
          <label class="block text-sm font-medium mb-1">触发器名称</label>
          <input 
            type="text" 
            bind:value={triggerName}
            class="w-full px-3 py-2 border rounded"
            placeholder="输入触发器名称"
          />
        </div>
        
        <div>
          <label class="block text-sm font-medium mb-1">触发器类型</label>
          <select 
            bind:value={triggerType}
            class="w-full px-3 py-2 border rounded"
          >
            <option value="event">事件触发器</option>
            <option value="schedule">定时触发器</option>
            <option value="webhook">Webhook 触发器</option>
          </select>
        </div>
      </div>
    {:else if step === 2}
      <TriggerConditionEditor 
        type={triggerType}
        bind:value={condition}
      />
    {:else if step === 3}
      <TriggerActionEditor 
        bind:value={action}
      />
    {/if}
    
    <!-- 操作按钮 -->
    <div class="flex justify-between">
      <button 
        on:click={handleBack}
        class="px-4 py-2 border rounded"
        disabled={step === 1}
      >
        上一步
      </button>
      
      {#if step < 3}
        <button 
          on:click={handleNext}
          class="px-4 py-2 bg-blue-500 text-white rounded"
        >
          下一步
        </button>
      {:else}
        <button 
          on:click={handleCreate}
          class="px-4 py-2 bg-green-500 text-white rounded"
        >
          创建
        </button>
      {/if}
    </div>
  </div>
</Modal>
```

### 1.3 触发条件编辑器

#### 条件编辑器 (TriggerConditionEditor.svelte)
```svelte
<script lang="ts">
  import type { TriggerType, TriggerCondition } from '@modular-agent/types';

  let { type, value } = $props<{
    type: TriggerType;
    value: TriggerCondition;
  }>();

  // 根据类型显示不同的条件编辑界面
</script>

<div class="space-y-4">
  {#if type === 'event'}
    <div>
      <label class="block text-sm font-medium mb-1">事件类型</label>
      <select bind:value={value.eventType} class="w-full px-3 py-2 border rounded">
        <option value="thread:completed">线程完成</option>
        <option value="thread:error">线程错误</option>
        <option value="node:executed">节点执行完成</option>
        <option value="tool:called">工具调用</option>
      </select>
    </div>
    
    <div>
      <label class="block text-sm font-medium mb-1">条件表达式</label>
      <textarea 
        bind:value={value.expression}
        class="w-full px-3 py-2 border rounded"
        rows="3"
        placeholder="输入条件表达式，例如: event.data.success === true"
      />
    </div>
  {:else if type === 'schedule'}
    <div>
      <label class="block text-sm font-medium mb-1">Cron 表达式</label>
      <input 
        type="text" 
        bind:value={value.cron}
        class="w-full px-3 py-2 border rounded"
        placeholder="0 0 * * *"
      />
    </div>
    
    <div>
      <label class="block text-sm font-medium mb-1">时区</label>
      <select bind:value={value.timezone} class="w-full px-3 py-2 border rounded">
        <option value="Asia/Shanghai">Asia/Shanghai</option>
        <option value="UTC">UTC</option>
      </select>
    </div>
  {:else if type === 'webhook'}
    <div>
      <label class="block text-sm font-medium mb-1">Webhook 路径</label>
      <input 
        type="text" 
        bind:value={value.path}
        class="w-full px-3 py-2 border rounded"
        placeholder="/webhook/my-trigger"
      />
    </div>
    
    <div>
      <label class="block text-sm font-medium mb-1">HTTP 方法</label>
      <select bind:value={value.method} class="w-full px-3 py-2 border rounded">
        <option value="POST">POST</option>
        <option value="GET">GET</option>
      </select>
    </div>
  {/if}
</div>
```

## 二、变量管理

### 2.1 变量列表页

#### 变量管理页面 (src/routes/variables/+page.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { variableStore } from '$lib/stores/variable';
  import { VariableAdapter } from '$lib/adapters/variable-adapter';
  import VariableCard from '$lib/components/variables/VariableCard.svelte';
  import CreateVariableModal from '$lib/components/variables/CreateVariableModal.svelte';

  const adapter = new VariableAdapter();
  
  let showCreateModal = $state(false);
  let searchQuery = $state('');

  onMount(async () => {
    variableStore.setLoading(true);
    try {
      const variables = await adapter.listVariables();
      variableStore.setVariables(variables);
    } catch (error) {
      variableStore.setError(error.message);
    }
  });

  async function handleCreateVariable(variable: Variable) {
    const newVariable = await adapter.createVariable(variable);
    variableStore.addVariable(newVariable);
  }

  async function handleDeleteVariable(id: string) {
    await adapter.deleteVariable(id);
    variableStore.deleteVariable(id);
  }
</script>

<div class="space-y-6">
  <div class="flex justify-between items-center">
    <h1 class="text-2xl font-bold">变量管理</h1>
    <div class="flex space-x-2">
      <input 
        type="text" 
        bind:value={searchQuery}
        class="px-3 py-2 border rounded"
        placeholder="搜索变量"
      />
      <button 
        on:click={() => showCreateModal = true}
        class="px-4 py-2 bg-blue-500 text-white rounded"
      >
        创建变量
      </button>
    </div>
  </div>
  
  {#if $variableStore.loading}
    <div>Loading...</div>
  {:else}
    <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      {#each $variableStore.variables.filter(v => v.name.includes(searchQuery)) as variable}
        <VariableCard 
          {variable}
          onDelete={() => handleDeleteVariable(variable.id)}
        />
      {/each}
    </div>
  {/if}
</div>

<CreateVariableModal 
  isOpen={showCreateModal}
  onClose={() => showCreateModal = false}
  onCreate={handleCreateVariable}
/>
```

### 2.2 变量编辑器

#### 创建变量对话框 (CreateVariableModal.svelte)
```svelte
<script lang="ts">
  import Modal from '$lib/components/common/Modal.svelte';
  import type { Variable, VariableType } from '@modular-agent/types';

  let { isOpen, onClose, onCreate } = $props<{
    isOpen: boolean;
    onClose: () => void;
    onCreate: (variable: Variable) => void;
  }>();

  let name = $state('');
  let type = $state<VariableType>('string');
  let value = $state('');
  let description = $state('');

  function handleCreate() {
    onCreate({
      name,
      type,
      value: parseValue(value, type),
      description,
    });
    onClose();
  }

  function parseValue(val: string, type: VariableType): any {
    switch (type) {
      case 'number':
        return parseFloat(val);
      case 'boolean':
        return val === 'true';
      case 'json':
        return JSON.parse(val);
      default:
        return val;
    }
  }
</script>

<Modal {isOpen} {onClose} title="创建变量">
  <div class="space-y-4">
    <div>
      <label class="block text-sm font-medium mb-1">变量名称</label>
      <input 
        type="text" 
        bind:value={name}
        class="w-full px-3 py-2 border rounded"
        placeholder="变量名称"
      />
    </div>
    
    <div>
      <label class="block text-sm font-medium mb-1">变量类型</label>
      <select bind:value={type} class="w-full px-3 py-2 border rounded">
        <option value="string">字符串</option>
        <option value="number">数字</option>
        <option value="boolean">布尔值</option>
        <option value="json">JSON</option>
      </select>
    </div>
    
    <div>
      <label class="block text-sm font-medium mb-1">变量值</label>
      {#if type === 'json'}
        <textarea 
          bind:value={value}
          class="w-full px-3 py-2 border rounded"
          rows="4"
          placeholder='{"key": "value"}'
        />
      {:else if type === 'boolean'}
        <select bind:value={value} class="w-full px-3 py-2 border rounded">
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
      {:else}
        <input 
          type="text" 
          bind:value={value}
          class="w-full px-3 py-2 border rounded"
          placeholder="变量值"
        />
      {/if}
    </div>
    
    <div>
      <label class="block text-sm font-medium mb-1">描述</label>
      <textarea 
        bind:value={description}
        class="w-full px-3 py-2 border rounded"
        rows="2"
        placeholder="变量描述"
      />
    </div>
    
    <div class="flex justify-end space-x-2">
      <button on:click={onClose} class="px-4 py-2 border rounded">取消</button>
      <button on:click={handleCreate} class="px-4 py-2 bg-blue-500 text-white rounded">创建</button>
    </div>
  </div>
</Modal>
```

## 三、Human Relay

### 3.1 Human Relay 管理页面

#### Relay 列表页 (src/routes/human-relays/+page.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { humanRelayStore } from '$lib/stores/human-relay';
  import { HumanRelayAdapter } from '$lib/adapters/human-relay-adapter';
  import RelayCard from '$lib/components/human-relay/RelayCard.svelte';

  const adapter = new HumanRelayAdapter();
  
  onMount(async () => {
    humanRelayStore.setLoading(true);
    try {
      const relays = await adapter.listRelays();
      humanRelayStore.setRelays(relays);
    } catch (error) {
      humanRelayStore.setError(error.message);
    }
  });

  async function handleSubmitResponse(id: string, response: string) {
    await adapter.submitResponse(id, response);
    humanRelayStore.updateRelay(id, { status: 'completed', response });
  }
</script>

<div class="space-y-6">
  <h1 class="text-2xl font-bold">Human Relay</h1>
  
  {#if $humanRelayStore.loading}
    <div>Loading...</div>
  {:else}
    <div class="space-y-4">
      {#each $humanRelayStore.relays as relay}
        <RelayCard 
          {relay}
          onSubmit={(response) => handleSubmitResponse(relay.id, response)}
        />
      {/each}
    </div>
  {/if}
</div>
```

### 3.2 Relay 响应对话框

#### 响应对话框 (RelayResponseModal.svelte)
```svelte
<script lang="ts">
  import Modal from '$lib/components/common/Modal.svelte';
  import type { HumanRelay } from '@modular-agent/types';

  let { relay, isOpen, onClose, onSubmit } = $props<{
    relay: HumanRelay;
    isOpen: boolean;
    onClose: () => void;
    onSubmit: (response: string) => void;
  }>();

  let response = $state('');

  function handleSubmit() {
    onSubmit(response);
    onClose();
  }
</script>

<Modal {isOpen} {onClose} title="提交响应">
  <div class="space-y-4">
    <div>
      <h3 class="font-bold mb-2">问题</h3>
      <p class="text-gray-700">{relay.question}</p>
    </div>
    
    <div>
      <h3 class="font-bold mb-2">上下文</h3>
      <pre class="bg-gray-100 p-2 rounded text-sm">{JSON.stringify(relay.context, null, 2)}</pre>
    </div>
    
    <div>
      <label class="block text-sm font-medium mb-1">响应</label>
      <textarea 
        bind:value={response}
        class="w-full px-3 py-2 border rounded"
        rows="4"
        placeholder="输入您的响应"
      />
    </div>
    
    <div class="flex justify-end space-x-2">
      <button on:click={onClose} class="px-4 py-2 border rounded">取消</button>
      <button on:click={handleSubmit} class="px-4 py-2 bg-blue-500 text-white rounded">提交</button>
    </div>
  </div>
</Modal>
```

## 四、高级可视化

### 4.1 统计图表组件

#### 图表组件库结构
```
src/lib/components/charts/
├── LineChart.svelte              # 折线图
├── BarChart.svelte               # 柱状图
├── PieChart.svelte               # 饼图
├── GanttChart.svelte             # 甘特图
└── utils/
    ├── chart-utils.ts            # 图表工具
    └── data-transform.ts         # 数据转换
```

#### 折线图组件 (LineChart.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import * as d3 from 'd3';

  let { 
    data = [],
    width = 600,
    height = 400,
    xKey = 'x',
    yKey = 'y',
    title = ''
  } = $props<{
    data: any[];
    width?: number;
    height?: number;
    xKey?: string;
    yKey?: string;
    title?: string;
  }>();

  let svgRef: SVGSVGElement;

  onMount(() => {
    const svg = d3.select(svgRef);
    const margin = { top: 20, right: 30, bottom: 30, left: 40 };
    const innerWidth = width - margin.left - margin.right;
    const innerHeight = height - margin.top - margin.bottom;

    // 清空 SVG
    svg.selectAll('*').remove();

    // 创建比例尺
    const x = d3.scaleLinear()
      .domain(d3.extent(data, d => d[xKey]) as [number, number])
      .range([0, innerWidth]);

    const y = d3.scaleLinear()
      .domain(d3.extent(data, d => d[yKey]) as [number, number])
      .range([innerHeight, 0]);

    // 创建容器
    const g = svg.append('g')
      .attr('transform', `translate(${margin.left},${margin.top})`);

    // 添加 X 轴
    g.append('g')
      .attr('transform', `translate(0,${innerHeight})`)
      .call(d3.axisBottom(x));

    // 添加 Y 轴
    g.append('g')
      .call(d3.axisLeft(y));

    // 添加折线
    const line = d3.line()
      .x(d => x(d[xKey]))
      .y(d => y(d[yKey]))
      .curve(d3.curveMonotoneX);

    g.append('path')
      .datum(data)
      .attr('fill', 'none')
      .attr('stroke', '#3b82f6')
      .attr('stroke-width', 2)
      .attr('d', line);

    // 添加数据点
    g.selectAll('circle')
      .data(data)
      .enter()
      .append('circle')
      .attr('cx', d => x(d[xKey]))
      .attr('cy', d => y(d[yKey]))
      .attr('r', 4)
      .attr('fill', '#3b82f6');
  });
</script>

<div>
  {#if title}
    <h3 class="font-bold mb-2">{title}</h3>
  {/if}
  <svg bind:this={svgRef} {width} {height}></svg>
</div>
```

### 4.2 执行统计仪表板

#### 统计仪表板 (src/routes/dashboard/+page.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import LineChart from '$lib/components/charts/LineChart.svelte';
  import BarChart from '$lib/components/charts/BarChart.svelte';
  import PieChart from '$lib/components/charts/PieChart.svelte';
  import { statsAdapter } from '$lib/adapters/stats-adapter';

  let threadStats = $state(null);
  let agentLoopStats = $state(null);
  let eventStats = $state(null);

  onMount(async () => {
    threadStats = await statsAdapter.getThreadStats();
    agentLoopStats = await statsAdapter.getAgentLoopStats();
    eventStats = await statsAdapter.getEventStats();
  });
</script>

<div class="space-y-6">
  <h1 class="text-2xl font-bold">统计仪表板</h1>
  
  <!-- 线程统计 -->
  <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
    <div class="bg-white p-4 rounded shadow">
      <h3 class="font-bold mb-4">线程执行趋势</h3>
      {#if threadStats}
        <LineChart 
          data={threadStats.trend}
          xKey="date"
          yKey="count"
          title="每日执行次数"
        />
      {/if}
    </div>
    
    <div class="bg-white p-4 rounded shadow">
      <h3 class="font-bold mb-4">线程状态分布</h3>
      {#if threadStats}
        <PieChart 
          data={threadStats.statusDistribution}
          valueKey="count"
          labelKey="status"
        />
      {/if}
    </div>
  </div>
  
  <!-- Agent Loop 统计 -->
  <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
    <div class="bg-white p-4 rounded shadow">
      <h3 class="font-bold mb-4">Agent Loop 执行时长</h3>
      {#if agentLoopStats}
        <BarChart 
          data={agentLoopStats.durationDistribution}
          xKey="range"
          yKey="count"
        />
      {/if}
    </div>
    
    <div class="bg-white p-4 rounded shadow">
      <h3 class="font-bold mb-4">工具调用统计</h3>
      {#if agentLoopStats}
        <BarChart 
          data={agentLoopStats.toolCallStats}
          xKey="tool"
          yKey="count"
        />
      {/if}
    </div>
  </div>
  
  <!-- 事件统计 -->
  <div class="bg-white p-4 rounded shadow">
    <h3 class="font-bold mb-4">事件频率</h3>
    {#if eventStats}
      <LineChart 
        data={eventStats.frequency}
        xKey="time"
        yKey="count"
        width={1000}
      />
    {/if}
  </div>
</div>
```

## 五、性能优化

### 5.1 前端性能优化

#### 虚拟滚动组件 (VirtualList.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';

  let { 
    items = [],
    itemHeight = 50,
    containerHeight = 400,
    renderItem
  } = $props<{
    items: any[];
    itemHeight?: number;
    containerHeight?: number;
    renderItem: (item: any, index: number) => any;
  }>();

  let containerRef: HTMLDivElement;
  let scrollTop = $state(0);

  const visibleCount = Math.ceil(containerHeight / itemHeight);
  const totalHeight = items.length * itemHeight;
  
  $derived startIndex = Math.floor(scrollTop / itemHeight);
  $derived endIndex = Math.min(startIndex + visibleCount + 1, items.length);
  $derived visibleItems = items.slice(startIndex, endIndex);
  $derived offsetY = startIndex * itemHeight;

  function handleScroll(event: Event) {
    scrollTop = (event.target as HTMLDivElement).scrollTop;
  }
</script>

<div 
  bind:this={containerRef}
  class="overflow-auto"
  style="height: {containerHeight}px"
  on:scroll={handleScroll}
>
  <div style="height: {totalHeight}px; position: relative;">
    <div style="position: absolute; top: {offsetY}px;">
      {#each visibleItems as item, i}
        <div style="height: {itemHeight}px;">
          {@render renderItem(item, startIndex + i)}
        </div>
      {/each}
    </div>
  </div>
</div>
```

#### 懒加载组件 (LazyLoad.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';

  let { 
    load,
    placeholder = null
  } = $props<{
    load: () => Promise<any>;
    placeholder?: any;
  }>();

  let component = $state(null);
  let loading = $state(true);

  onMount(async () => {
    component = await load();
    loading = false;
  });
</script>

{#if loading}
  {@render placeholder?.()}
{:else}
  {@render component?.()}
{/if}
```

### 5.2 后端性能优化

#### 缓存中间件 (cache-middleware.ts)
```typescript
import { Request, Response, NextFunction } from 'express';

interface CacheEntry {
  data: any;
  timestamp: number;
  ttl: number;
}

export class CacheMiddleware {
  private cache: Map<string, CacheEntry> = new Map();
  private defaultTTL: number;

  constructor(defaultTTL: number = 60000) {
    this.defaultTTL = defaultTTL;
  }

  middleware() {
    return (req: Request, res: Response, next: NextFunction) => {
      const key = this.generateKey(req);
      const entry = this.cache.get(key);

      if (entry && Date.now() - entry.timestamp < entry.ttl) {
        res.json(entry.data);
        return;
      }

      // 拦截 res.json
      const originalJson = res.json.bind(res);
      res.json = (data: any) => {
        this.cache.set(key, {
          data,
          timestamp: Date.now(),
          ttl: this.defaultTTL,
        });
        return originalJson(data);
      };

      next();
    };
  }

  private generateKey(req: Request): string {
    return `${req.method}:${req.url}:${JSON.stringify(req.body)}`;
  }

  clear() {
    this.cache.clear();
  }
}
```

#### 流式响应优化 (stream-response.ts)
```typescript
import { Response } from 'express';

export async function streamJSON<T>(
  res: Response,
  data: AsyncIterable<T>,
  transformer?: (item: T) => any
) {
  res.setHeader('Content-Type', 'application/json');
  res.setHeader('Transfer-Encoding', 'chunked');

  res.write('[');

  let first = true;
  for await (const item of data) {
    if (!first) {
      res.write(',');
    }
    first = false;

    const transformed = transformer ? transformer(item) : item;
    res.write(JSON.stringify(transformed));
  }

  res.write(']');
  res.end();
}
```

### 5.3 SSE 优化

#### 增量更新 (incremental-update.ts)
```typescript
import { diff, patch } from 'jsondiffpatch';

export class IncrementalUpdate {
  private previousState: any = null;

  computeUpdate(newState: any): any {
    if (!this.previousState) {
      this.previousState = newState;
      return { type: 'full', data: newState };
    }

    const delta = diff(this.previousState, newState);
    this.previousState = newState;

    if (!delta) {
      return { type: 'none' };
    }

    return { type: 'delta', data: delta };
  }

  applyUpdate(base: any, update: any): any {
    if (update.type === 'full') {
      return update.data;
    }

    if (update.type === 'delta') {
      return patch(base, update.data);
    }

    return base;
  }
}
```

## 六、开发任务清单

### 6.1 触发器管理
- [ ] 实现触发器列表页面
- [ ] 实现触发器创建向导
- [ ] 实现触发条件编辑器
- [ ] 实现触发动作编辑器
- [ ] 实现触发器启用/禁用
- [ ] 实现触发器测试

### 6.2 变量管理
- [ ] 实现变量列表页面
- [ ] 实现变量创建对话框
- [ ] 实现变量编辑对话框
- [ ] 实现变量类型验证
- [ ] 实现变量搜索

### 6.3 Human Relay
- [ ] 实现 Relay 列表页面
- [ ] 实现 Relay 响应对话框
- [ ] 实现 Relay 超时处理
- [ ] 实现 Relay 通知

### 6.4 高级可视化
- [ ] 实现折线图组件
- [ ] 实现柱状图组件
- [ ] 实现饼图组件
- [ ] 实现甘特图组件
- [ ] 实现统计仪表板
- [ ] 实现数据导出

### 6.5 性能优化
- [ ] 实现虚拟滚动
- [ ] 实现懒加载
- [ ] 实现缓存中间件
- [ ] 实现流式响应
- [ ] 实现 SSE 事件流优化
- [ ] 实现增量更新

## 七、验收标准

### 7.1 功能验收
- ✅ 触发器管理功能完整
- ✅ 变量管理功能完整
- ✅ Human Relay 功能完整
- ✅ 统计图表正常显示
- ✅ 性能优化生效

### 7.2 性能验收
- ✅ 大列表渲染流畅(虚拟滚动)
- ✅ 页面加载快速(懒加载)
- ✅ API 响应快速(缓存)
- ✅ SSE 事件流传输高效

## 八、时间估算

- 触发器管理: 3 天
- 变量管理: 2 天
- Human Relay: 2 天
- 高级可视化: 4 天
- 性能优化: 3 天
- **总计: 约 14 天**

## 九、下一步计划

完成第三阶段后,进入第四阶段:
- 实现多用户支持
- 实现权限管理
- 实现国际化
- 实现插件系统
- 实现移动端适配
