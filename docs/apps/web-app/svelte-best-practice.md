# Svelte 核心最佳实践（通用 + SvelteKit 全覆盖）
Svelte 和 Vue/React 思路完全不同：**编译时框架、原生式响应式、极简 API**，所以最佳实践也围绕「**少写代码、利用编译特性、避免原生反模式**」展开。

我整理了**最实用、最通用、能直接落地**的最佳实践，覆盖组件、状态、性能、工程化、SvelteKit 全场景。

---

## 一、组件与语法最佳实践（最常用）
### 1. 组件拆分：小而单一，遵循单一职责
- 一个组件只做**一件事**，避免几百行的巨型组件
- 可复用 UI（按钮、卡片、表单）、业务模块（列表、表单）都拆成小组件
- 好处：可读性高、响应式更精准、热更新更快

### 2. 样式天然作用域，不要滥用全局样式
- Svelte `<style>` **默认就是 scoped**，不用加任何修饰
- 全局样式只放在**根组件/专门的 global.css**
- 需要穿透作用域用 `:global()`，不要全量取消作用域
```svelte
<style>
  /* 局部样式，仅当前组件生效 */
  .card {}
  /* 全局生效 */
  :global(.btn) {}
</style>
```

### 3. Props 必写默认值 + 类型（TS）
-  props 必须声明默认值，避免 `undefined` 报错
- 用 TS 时明确类型，让组件更健壮
```svelte
<script lang="ts">
  export let title: string = '默认标题';
  export let count: number = 0;
</script>
```

### 4. 事件使用原生 on: 语法，不要手动 addEventListener
- Svelte 内置事件绑定自带**自动清理**，不会内存泄漏
- 自定义事件用 `createEventDispatcher`，保持规范

---

## 二、响应式最佳实践（Svelte 核心）
### 1. 能用简单变量，就不用对象/数组
Svelte 对**基础类型响应式最丝滑、性能最好**
```svelte
let count = 0; // ✅ 最佳
count++; // 自动更新

let obj = { count: 0 }; // ❌ 必须重新赋值才更新
obj.count++; // 不更新
obj = { ...obj }; // 必须这样写
```

### 2. 复杂更新：坚持「重新赋值」而不是修改引用
对象/数组更新，**最佳实践是覆盖赋值**，而不是修改内部值
```svelte
let list = [1,2,3];

// ✅ 最佳：重新赋值
list = [...list, 4];

// ❌ 不推荐：原地修改
list.push(4);
```

### 3. 用 $: 自动计算，不要手动监听
`$:` 是 Svelte 最强特性，**替代 watch/useEffect**
- 自动追踪依赖
- 代码极简、无心智负担
```svelte
let a = 1;
let b = 2;
$: total = a + b; // ✅ 自动计算
```

### 4. 不要滥用响应式：不变数据不要定义成响应式变量
- 静态配置、常量、不参与渲染的数据，直接用 `const`
- 减少不必要的响应式追踪，提升性能

---

## 三、状态管理最佳实践（Svelte 原生自带）
Svelte **不需要 Pinia/Vuex**，内置状态方案足够用，这是核心最佳实践。

### 1. 组件内状态：普通变量 + $:
### 2. 父子/跨组件：props + 事件 / context
### 3. 全局状态：使用 writable 存储（标准方案）
**最佳实践：按模块拆分 stores**
```js
// stores/user.js
import { writable } from 'svelte/store';
export const user = writable(null);
export const token = writable('');
```
使用时用 `$` 自动订阅，极简：
```svelte
<script>
  import { user } from '@/stores/user';
</script>
{$user?.name}
```

### 4. 大型项目：状态分层
- 页面组件 → 业务 store → 通用 store
- 避免把所有状态塞到一个全局 store

---

## 四、性能最佳实践（Svelte 优化重点）
### 1. 避免大列表直接渲染：使用 {#key} 或 虚拟滚动
- 长列表渲染用 `{#key}` 精准更新
- 超大数据用第三方虚拟滚动库

### 2. 不要在模板中写复杂函数
模板函数会**频繁执行**，复杂逻辑一定要用 `$:` 提前计算
```svelte
{#each list as item}
  {complexFn(item)} <!-- ❌ 差 -->
  {$computedItem}    <!-- ✅ 好 -->
{/each}
```

### 3. 组件卸载自动清理：Svelte 已帮你做 99%
- 内置事件、store 订阅都会自动清理
- 只有**自定义定时器/第三方库**需要手动清理
```svelte
import { onDestroy } from 'svelte';
let timer = setInterval();
onDestroy(() => clearInterval(timer));
```

### 4. 图片/资源懒加载
SvelteKit 自带 `<img>` 优化，普通 Svelte 用原生 `loading="lazy"`

---

## 五、SvelteKit（官方框架）最佳实践
如果你用 Svelte，90% 会用 SvelteKit，这是行业标准。

### 1. 路由严格遵循文件系统路由
- pages 路由、layouts 布局、error 页面、loading 页面都用文件约定
- 不要手动写路由配置

### 2. 数据加载：用 load 函数，不要在页面 fetch
- 页面数据在 `+page.server.js` / `+page.js` 加载
- 支持 SSR/SSG，性能更好、SEO 更强

### 3. 服务端只放敏感逻辑
- 接口请求、数据库、密钥 → 放 `.server.js`
- 客户端逻辑 → 普通文件

### 4. 表单提交：use:enhance 最佳实践
SvelteKit 表单标准方案：渐进式增强、友好、无 JS 也可运行

---

## 六、工程化与代码规范最佳实践
### 1. 统一目录结构（行业通用）
```
src/
├── components/  公共组件
├── lib/         工具函数
├── stores/      状态管理
├── styles/      全局样式
└── routes/      SvelteKit 路由
```

### 2. 使用 ESLint + Prettier
Svelte 官方有配置，直接使用，保持团队代码一致

### 3. 组件命名：大驼峰，文件与组件名一致
`Button.svelte`、`UserList.svelte`

### 4. 逻辑抽离：把复杂业务放到 lib/xxx.js
Svelte 组件专注 UI，逻辑抽离成纯 JS 函数，更好维护、更好测试

---

## 七、最容易踩坑的“反模式”（一定要避开）
1. **直接修改对象/数组内部值** → 界面不更新
2. **在模板里写复杂计算** → 性能爆炸
3. **全局样式乱写** → 样式污染
4. **一个组件几百行** → 难以维护
5. **手动写大量事件监听** → 内存泄漏
6. **过度设计状态** → 明明变量就能用，非要写 store

---

# 核心最佳实践总结（一句话记住）
**用原生思维写代码，用赋值做响应式，用 $: 做计算，用 store 做全局状态，组件拆小，逻辑抽离，遵循 SvelteKit 约定。**

Svelte 的最佳实践本质就是：**越少代码、越少框架 API、越贴近原生，越好。**

---

### 总结
1. **组件**：小而单一、样式作用域、props 默认值
2. **响应式**：基础变量优先、重新赋值、$: 计算
3. **状态**：组件内变量、跨组件 context、全局 store
4. **性能**：模板无复杂函数、长列表优化、自动清理
5. **工程化**：SvelteKit 约定优先、规范目录、逻辑抽离
