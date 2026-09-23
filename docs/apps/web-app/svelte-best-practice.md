# Svelte 核心最佳实践（Svelte 5 + SvelteKit，适用 apps/web-app）

> 本文面向本仓库的 Svelte 5（Runes）与 SvelteKit 2。旧版基于 Svelte 4（`export let`、`$:`、`on:click`、`createEventDispatcher`）的写法**一律不再使用**。样式与组件的项目级约束以 `docs/spec/web/style-guide.md` 与 `docs/spec/web/component-guide.md` 为准。

## 一、组件与语法

### 1. 组件拆分：小而单一

- 一个组件只做一件事；遵循组件规范三层：原子层不感知业务域，业务层只感知单域模型，装配层负责路由与数据。
- 禁止巨型组件：接近职责边界时优先拆分，而不是加注释硬撑。

### 2. Props：`$props` + 解构默认值

```svelte
<script lang="ts">
	let { title = '默认标题', count = 0 } = $props<{ title?: string; count?: number }>();
</script>
```

- 必写默认值与 TS 类型；不再使用 `export let`。
- 命名输出用 snippet（替代 slot / `createEventDispatcher`）：

```svelte
<script lang="ts">
	let { children, header }: { children: () => any; header?: () => any } = $props();
</script>

{@render header?.()}
{@render children()}
```

### 3. 事件：直接属性，不手动 addEventListener

- 原生事件用 `onclick` / `onkeydown`（全小写属性），自带自动清理；不再用 `on:click`。
- 组件通信优先回调 props 或 snippet，不再用 `createEventDispatcher`。

### 4. 样式作用域

- `<style>` 默认 scoped；全局样式只放根组件或专门的 global 样式文件；穿透用 `:global()` 且收敛。
- 颜色、圆角、阴影、动效时长一律走 Token，禁止硬编码（样式规范禁止事项）。

## 二、响应式（Runes）

### 1. `$state`：细粒度响应式

```svelte
<script lang="ts">
	let count = $state(0);
	let list = $state<string[]>([]);
	let options = $state.raw({ theme: 'dark' }); // 大对象/不可变数据用 raw，避免深层代理开销
</script>
```

- `$state` 创建深层响应式代理：对普通对象/数组的**原地修改会触发更新**（与 Svelte 4 必须整体重赋值不同）。
- `$state.raw` 不代理深层：适合配置、大缓存、外部库对象；修改后需整体替换引用才更新。
- 从外部（非组件）传入的可变对象不要假设已被代理，进入组件状态时显式拷贝或包装。

### 2. `$derived`：自动计算（替代 `$:`）

```svelte
let total = $derived(list.length + count);
let heavy = $derived.by(() => expensive(list)); // 函数体放 .by
```

- 派生值只读；不要在 `$derived` 里做副作用。

### 3. `$effect`：副作用（替代 `$:` 中的副作用写法）

- 只用于与外部系统同步：DOM 测量、订阅、定时器、流读取器接线。
- 清理函数写法与 `onDestroy` 等价，务必成对：

```svelte
$effect(() => {
	const timer = setInterval(tick, 1000);
	return () => clearInterval(timer);
});
```

- **禁止**用 `$effect` 做“本可以 `$derived`”的派生计算；禁止在其中做无法清理的订阅。
- 组件销毁清理：内置事件与 store 订阅自动清理，仅定时器、流读取器、第三方实例需手动返回清理函数。

### 4. 不变数据不要响应式化

- 静态配置、常量、不参与渲染的数据用 `const`，不进 `$state`。

## 三、状态管理

1. **组件内**：`$state` / `$derived`。
2. **父子/跨组件**：props + 回调/snippet；深层共享用 `setContext`/`getContext`。
3. **全局共享**：Svelte store（`writable`/`readable`）仍可用，按域拆分文件（`stores/<domain>.ts`）；模板中 `$store` 自动订阅。
4. **服务端数据**：优先 SvelteKit `load`（`+page.ts` / `+layout.ts`）拉取，页面组件消费；不在模板里发请求。
5. **分层约束**（组件规范）：组件不直接持有流读取器；流式数据限速合并后写 store；本地偏好走统一偏好封装，不直读写 storage。

## 四、SvelteKit 约定

- 严格文件系统路由；布局用 `+layout.svelte`，数据用 `load`，不在页面 `onMount` 里做首屏关键请求。
- 装配层（routes）负责：路由参数、`load`、领域 API 调用、store 组装；不写原子渲染细节。
- 本应用为静态部署（`adapter-static` + SPA 回退），按客户端渲染模型设计，不依赖 SSR SEO。
- API 访问一律走 `src/lib/api/client.ts` 封装，禁止组件内散落 `fetch('/api/...')`。

## 五、性能

- 模板中不写复杂函数：先 `$derived` 再渲染；`{#each}` 带稳定 `key`。
- 大列表虚拟滚动（公共封装），不整页渲染万级行。
- 流式更新限速合并进 store，组件观察合并结果，避免逐帧重渲染；长文本可分段渲染。
- 图片与重型预览按需加载；详情分面按需 `load`。
- `$state.raw` 用于大体量快照数据，减少代理成本。

## 六、工程与质量

- 目录：`src/lib/api`（契约）、`src/lib/components`（原子 + 业务）、`src/lib/stores`、`src/routes`（装配）。
- 组件文件大驼峰，与组件名一致；代码英文，注释只描述意图、不引用文档章节编号。
- 门禁：`npm run check`（svelte-check）、`npm run typecheck`、`npm run lint`、`npm run test`；合入前全绿。
- 禁止 `unwrap` 式空断言蔓延到前端等价物：不做无意义兜底分支掩盖错误，错误走错误态组件显式呈现。

## 七、反模式清单（禁止）

1. 使用 Svelte 4 语法：`export let`、`$:`、`on:click`、`createEventDispatcher`、slot。
2. 组件内直接 `fetch` 后端接口或绕过 `client.ts`。
3. 手写与 `schema.d.ts` 重复的响应模型。
4. `$effect` 做派生计算、或写不可清理的副作用。
5. 对 `$state.raw` 数据做原地修改并期望更新。
6. 硬编码色值、自创圆角/阴影/动效时长（绕过 Token）。
7. 巨型组件、跨域拼装业务组件、组件直连流读取器。
8. 用空态/兜底值吞掉错误，不走错误态与重试。

## 八、一句话总结

**Runes 表达状态，`$derived` 做计算，`$effect` 只对接外部世界；数据经 `client.ts` 与 `load` 进装配层，组件只消费视图模型；样式走 Token，流式进 store，门禁全绿再合入。**
