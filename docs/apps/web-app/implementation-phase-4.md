# Web App 实施方案 - 第四阶段

## 阶段目标

在第三阶段基础上,实现多用户支持、权限管理、国际化、插件系统和移动端适配等扩展功能,使系统具备生产环境部署能力。

## 一、多用户支持

### 1.1 用户认证系统

#### 用户模型
```typescript
// src/types/user.ts
export interface User {
  id: string;
  username: string;
  email: string;
  passwordHash: string;
  role: UserRole;
  createdAt: Date;
  updatedAt: Date;
}

export type UserRole = 'admin' | 'user' | 'guest';

export interface AuthToken {
  userId: string;
  token: string;
  expiresAt: Date;
}
```

#### 认证服务 (src/services/auth-service.ts)
```typescript
import bcrypt from 'bcrypt';
import jwt from 'jsonwebtoken';
import { v4 as uuidv4 } from 'uuid';

export class AuthService {
  private userRepository: UserRepository;
  private tokenRepository: TokenRepository;
  private jwtSecret: string;

  constructor(
    userRepository: UserRepository,
    tokenRepository: TokenRepository,
    jwtSecret: string
  ) {
    this.userRepository = userRepository;
    this.tokenRepository = tokenRepository;
    this.jwtSecret = jwtSecret;
  }

  async register(username: string, email: string, password: string): Promise<User> {
    // 检查用户是否已存在
    const existingUser = await this.userRepository.findByUsername(username);
    if (existingUser) {
      throw new Error('Username already exists');
    }

    // 创建用户
    const passwordHash = await bcrypt.hash(password, 10);
    const user = await this.userRepository.create({
      id: uuidv4(),
      username,
      email,
      passwordHash,
      role: 'user',
      createdAt: new Date(),
      updatedAt: new Date(),
    });

    return user;
  }

  async login(username: string, password: string): Promise<{ user: User; token: string }> {
    // 查找用户
    const user = await this.userRepository.findByUsername(username);
    if (!user) {
      throw new Error('User not found');
    }

    // 验证密码
    const isValid = await bcrypt.compare(password, user.passwordHash);
    if (!isValid) {
      throw new Error('Invalid password');
    }

    // 生成 JWT
    const token = jwt.sign(
      { userId: user.id, role: user.role },
      this.jwtSecret,
      { expiresIn: '7d' }
    );

    // 保存 token
    await this.tokenRepository.create({
      userId: user.id,
      token,
      expiresAt: new Date(Date.now() + 7 * 24 * 60 * 60 * 1000),
    });

    return { user, token };
  }

  async logout(token: string): Promise<void> {
    await this.tokenRepository.deleteByToken(token);
  }

  async validateToken(token: string): Promise<User | null> {
    try {
      const decoded = jwt.verify(token, this.jwtSecret) as { userId: string };
      const user = await this.userRepository.findById(decoded.userId);
      return user;
    } catch {
      return null;
    }
  }
}
```

#### 认证中间件 (src/middleware/auth-middleware.ts)
```typescript
import { Request, Response, NextFunction } from 'express';
import { AuthService } from '../services/auth-service';

export function authMiddleware(authService: AuthService) {
  return async (req: Request, res: Response, next: NextFunction) => {
    const token = req.headers.authorization?.replace('Bearer ', '');

    if (!token) {
      res.status(401).json({ error: 'No token provided' });
      return;
    }

    const user = await authService.validateToken(token);
    if (!user) {
      res.status(401).json({ error: 'Invalid token' });
      return;
    }

    req.user = user;
    next();
  };
}

declare global {
  namespace Express {
    interface Request {
      user?: User;
    }
  }
}
```

### 1.2 用户管理界面

#### 登录页面 (src/routes/login/+page.svelte)
```svelte
<script lang="ts">
  import { goto } from '$app/navigation';
  import { authStore } from '$lib/stores/auth';

  let username = $state('');
  let password = $state('');
  let error = $state('');

  async function handleLogin() {
    try {
      await authStore.login(username, password);
      goto('/');
    } catch (e) {
      error = e.message;
    }
  }
</script>

<div class="min-h-screen flex items-center justify-center bg-gray-100">
  <div class="bg-white p-8 rounded shadow-md w-96">
    <h1 class="text-2xl font-bold mb-6 text-center">登录</h1>
    
    {#if error}
      <div class="bg-red-100 text-red-700 p-3 rounded mb-4">
        {error}
      </div>
    {/if}
    
    <form on:submit|preventDefault={handleLogin} class="space-y-4">
      <div>
        <label class="block text-sm font-medium mb-1">用户名</label>
        <input 
          type="text" 
          bind:value={username}
          class="w-full px-3 py-2 border rounded"
          required
        />
      </div>
      
      <div>
        <label class="block text-sm font-medium mb-1">密码</label>
        <input 
          type="password" 
          bind:value={password}
          class="w-full px-3 py-2 border rounded"
          required
        />
      </div>
      
      <button 
        type="submit"
        class="w-full py-2 bg-blue-500 text-white rounded hover:bg-blue-600"
      >
        登录
      </button>
    </form>
  </div>
</div>
```

#### 用户管理页面 (src/routes/admin/users/+page.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { userStore } from '$lib/stores/user';
  import { userAdapter } from '$lib/adapters/user-adapter';

  onMount(async () => {
    const users = await userAdapter.listUsers();
    userStore.setUsers(users);
  });

  async function handleDeleteUser(id: string) {
    await userAdapter.deleteUser(id);
    userStore.deleteUser(id);
  }
</script>

<div class="space-y-6">
  <h1 class="text-2xl font-bold">用户管理</h1>
  
  <table class="w-full bg-white rounded shadow">
    <thead>
      <tr class="border-b">
        <th class="px-4 py-2 text-left">用户名</th>
        <th class="px-4 py-2 text-left">邮箱</th>
        <th class="px-4 py-2 text-left">角色</th>
        <th class="px-4 py-2 text-left">创建时间</th>
        <th class="px-4 py-2 text-left">操作</th>
      </tr>
    </thead>
    <tbody>
      {#each $userStore.users as user}
        <tr class="border-b">
          <td class="px-4 py-2">{user.username}</td>
          <td class="px-4 py-2">{user.email}</td>
          <td class="px-4 py-2">{user.role}</td>
          <td class="px-4 py-2">{new Date(user.createdAt).toLocaleDateString()}</td>
          <td class="px-4 py-2">
            <button 
              on:click={() => handleDeleteUser(user.id)}
              class="text-red-500 hover:underline"
            >
              删除
            </button>
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
</div>
```

## 二、权限管理

### 2.1 权限模型

#### 权限定义
```typescript
// src/types/permission.ts
export type Permission = 
  | 'workflow:read'
  | 'workflow:write'
  | 'workflow:delete'
  | 'thread:read'
  | 'thread:write'
  | 'thread:delete'
  | 'agent-loop:read'
  | 'agent-loop:write'
  | 'agent-loop:delete'
  | 'tool:read'
  | 'tool:write'
  | 'profile:read'
  | 'profile:write'
  | 'user:manage';

export const ROLE_PERMISSIONS: Record<UserRole, Permission[]> = {
  admin: [
    'workflow:read', 'workflow:write', 'workflow:delete',
    'thread:read', 'thread:write', 'thread:delete',
    'agent-loop:read', 'agent-loop:write', 'agent-loop:delete',
    'tool:read', 'tool:write',
    'profile:read', 'profile:write',
    'user:manage',
  ],
  user: [
    'workflow:read', 'workflow:write',
    'thread:read', 'thread:write',
    'agent-loop:read', 'agent-loop:write',
    'tool:read',
    'profile:read',
  ],
  guest: [
    'workflow:read',
    'thread:read',
    'agent-loop:read',
  ],
};
```

#### 权限检查中间件 (src/middleware/permission-middleware.ts)
```typescript
import { Request, Response, NextFunction } from 'express';
import { Permission, ROLE_PERMISSIONS } from '../types/permission';

export function requirePermission(permission: Permission) {
  return (req: Request, res: Response, next: NextFunction) => {
    const user = req.user;
    
    if (!user) {
      res.status(401).json({ error: 'Unauthorized' });
      return;
    }

    const permissions = ROLE_PERMISSIONS[user.role];
    if (!permissions.includes(permission)) {
      res.status(403).json({ error: 'Forbidden' });
      return;
    }

    next();
  };
}
```

### 2.2 前端权限控制

#### 权限检查组件 (HasPermission.svelte)
```svelte
<script lang="ts">
  import { authStore } from '$lib/stores/auth';
  import { hasPermission } from '$lib/utils/permission';
  import type { Permission } from '$lib/types/permission';

  let { permission, children } = $props<{
    permission: Permission;
    children: () => any;
  }>();
</script>

{#if $authStore.user && hasPermission($authStore.user.role, permission)}
  {@render children()}
{/if}
```

#### 使用示例
```svelte
<script lang="ts">
  import HasPermission from '$lib/components/auth/HasPermission.svelte';
</script>

<div>
  <HasPermission permission="workflow:write">
    <button class="btn-primary">创建工作流</button>
  </HasPermission>
</div>
```

## 三、国际化

### 3.1 国际化架构

#### 语言文件结构
```
src/lib/i18n/
├── index.ts                      # 国际化入口
├── types.ts                      # 类型定义
└── locales/
    ├── en-US.json                # 英语
    ├── zh-CN.json                # 简体中文
    └── ja-JP.json                # 日语
```

#### 语言文件示例 (zh-CN.json)
```json
{
  "common": {
    "save": "保存",
    "cancel": "取消",
    "delete": "删除",
    "edit": "编辑",
    "create": "创建",
    "search": "搜索",
    "loading": "加载中..."
  },
  "workflow": {
    "title": "工作流管理",
    "create": "创建工作流",
    "edit": "编辑工作流",
    "delete": "删除工作流",
    "list": "工作流列表",
    "details": "工作流详情",
    "name": "工作流名称",
    "description": "描述"
  },
  "thread": {
    "title": "线程监控",
    "list": "线程列表",
    "details": "线程详情",
    "status": {
      "running": "运行中",
      "paused": "已暂停",
      "completed": "已完成",
      "failed": "失败",
      "cancelled": "已取消"
    }
  }
}
```

#### 国际化服务 (src/lib/i18n/index.ts)
```typescript
import { writable, derived } from 'svelte/store';
import enUS from './locales/en-US.json';
import zhCN from './locales/zh-CN.json';
import jaJP from './locales/ja-JP.json';

const locales: Record<string, any> = {
  'en-US': enUS,
  'zh-CN': zhCN,
  'ja-JP': jaJP,
};

export const currentLocale = writable<string>('zh-CN');

export const t = derived(currentLocale, ($locale) => {
  return (key: string, params?: Record<string, any>): string => {
    const keys = key.split('.');
    let value: any = locales[$locale];
    
    for (const k of keys) {
      value = value?.[k];
    }
    
    if (typeof value !== 'string') {
      return key;
    }
    
    if (params) {
      return value.replace(/\{(\w+)\}/g, (_, name) => params[name] || '');
    }
    
    return value;
  };
});

export function setLocale(locale: string) {
  if (locales[locale]) {
    currentLocale.set(locale);
  }
}
```

### 3.2 语言切换组件

#### 语言选择器 (LanguageSelector.svelte)
```svelte
<script lang="ts">
  import { setLocale, currentLocale } from '$lib/i18n';

  const languages = [
    { code: 'zh-CN', name: '简体中文' },
    { code: 'en-US', name: 'English' },
    { code: 'ja-JP', name: '日本語' },
  ];
</script>

<select 
  value={$currentLocale}
  on:change={(e) => setLocale(e.target.value)}
  class="px-3 py-1 border rounded"
>
  {#each languages as lang}
    <option value={lang.code}>{lang.name}</option>
  {/each}
</select>
```

#### 使用示例
```svelte
<script lang="ts">
  import { t } from '$lib/i18n';
</script>

<div>
  <h1>{$t('workflow.title')}</h1>
  <button>{$t('common.create')}</button>
</div>
```

## 四、插件系统

### 4.1 插件架构

#### 插件接口定义
```typescript
// src/types/plugin.ts
export interface Plugin {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  
  // 生命周期钩子
  onLoad?: (context: PluginContext) => Promise<void>;
  onUnload?: () => Promise<void>;
  
  // 扩展点
  routes?: PluginRoute[];
  components?: PluginComponent[];
  tools?: PluginTool[];
  menuItems?: PluginMenuItem[];
}

export interface PluginContext {
  api: APIFactory;
  stores: StoreRegistry;
  events: EventEmitter;
  logger: Logger;
}

export interface PluginRoute {
  path: string;
  component: string;
  meta?: any;
}

export interface PluginComponent {
  id: string;
  component: string;
  slot: string;
}

export interface PluginTool {
  id: string;
  name: string;
  description: string;
  parameters: any;
  execute: (params: any) => Promise<any>;
}

export interface PluginMenuItem {
  path: string;
  label: string;
  icon?: string;
  order?: number;
}
```

### 4.2 插件管理器

#### 插件管理服务 (src/services/plugin-manager.ts)
```typescript
import { v4 as uuidv4 } from 'uuid';
import type { Plugin, PluginContext } from '../types/plugin';

export class PluginManager {
  private plugins: Map<string, Plugin> = new Map();
  private context: PluginContext;

  constructor(context: PluginContext) {
    this.context = context;
  }

  async loadPlugin(plugin: Plugin): Promise<void> {
    if (this.plugins.has(plugin.id)) {
      throw new Error(`Plugin ${plugin.id} already loaded`);
    }

    // 调用插件加载钩子
    if (plugin.onLoad) {
      await plugin.onLoad(this.context);
    }

    // 注册插件
    this.plugins.set(plugin.id, plugin);

    // 注册路由
    if (plugin.routes) {
      this.registerRoutes(plugin.routes);
    }

    // 注册工具
    if (plugin.tools) {
      this.registerTools(plugin.tools);
    }

    // 注册菜单项
    if (plugin.menuItems) {
      this.registerMenuItems(plugin.menuItems);
    }
  }

  async unloadPlugin(pluginId: string): Promise<void> {
    const plugin = this.plugins.get(pluginId);
    if (!plugin) {
      return;
    }

    // 调用插件卸载钩子
    if (plugin.onUnload) {
      await plugin.onUnload();
    }

    // 注销插件
    this.plugins.delete(pluginId);
  }

  getPlugin(pluginId: string): Plugin | undefined {
    return this.plugins.get(pluginId);
  }

  listPlugins(): Plugin[] {
    return Array.from(this.plugins.values());
  }

  private registerRoutes(routes: PluginRoute[]) {
    // 注册路由到应用
  }

  private registerTools(tools: PluginTool[]) {
    // 注册工具到工具注册表
  }

  private registerMenuItems(items: PluginMenuItem[]) {
    // 注册菜单项到侧边栏
  }
}
```

### 4.3 插件示例

#### 示例插件 (my-plugin/index.ts)
```typescript
import type { Plugin } from '@modular-agent/types';

const MyPlugin: Plugin = {
  id: 'my-plugin',
  name: 'My Custom Plugin',
  version: '1.0.0',
  description: 'A custom plugin example',
  author: 'Developer',

  async onLoad(context) {
    context.logger.info('My plugin loaded');
  },

  async onUnload() {
    console.log('My plugin unloaded');
  },

  routes: [
    {
      path: '/my-plugin',
      component: './routes/MyPluginPage.svelte',
    },
  ],

  tools: [
    {
      id: 'my-tool',
      name: 'my_tool',
      description: 'A custom tool',
      parameters: {
        type: 'object',
        properties: {
          input: { type: 'string' },
        },
      },
      async execute(params) {
        return { result: `Processed: ${params.input}` };
      },
    },
  ],

  menuItems: [
    {
      path: '/my-plugin',
      label: 'My Plugin',
      icon: 'plugin',
      order: 100,
    },
  ],
};

export default MyPlugin;
```

## 五、移动端适配

### 5.1 响应式设计

#### 响应式布局组件 (ResponsiveLayout.svelte)
```svelte
<script lang="ts">
  import { onMount } from 'svelte';

  let isMobile = $state(false);
  let isTablet = $state(false);
  let isDesktop = $state(true);

  onMount(() => {
    const checkDevice = () => {
      const width = window.innerWidth;
      isMobile = width < 768;
      isTablet = width >= 768 && width < 1024;
      isDesktop = width >= 1024;
    };

    checkDevice();
    window.addEventListener('resize', checkDevice);
    
    return () => {
      window.removeEventListener('resize', checkDevice);
    };
  });
</script>

<div class="min-h-screen">
  {#if isMobile}
    <!-- 移动端布局 -->
    <div class="flex flex-col">
      <MobileHeader />
      <main class="flex-1">
        <slot />
      </main>
      <MobileNav />
    </div>
  {:else if isTablet}
    <!-- 平板布局 -->
    <div class="flex">
      <TabletSidebar />
      <main class="flex-1">
        <slot />
      </main>
    </div>
  {:else}
    <!-- 桌面布局 -->
    <div class="flex">
      <DesktopSidebar />
      <main class="flex-1">
        <slot />
      </main>
    </div>
  {/if}
</div>
```

### 5.2 移动端组件

#### 移动端导航 (MobileNav.svelte)
```svelte
<script lang="ts">
  import { page } from '$app/stores';

  const navItems = [
    { path: '/workflows', label: '工作流', icon: 'workflow' },
    { path: '/threads', label: '线程', icon: 'thread' },
    { path: '/agent-loops', label: 'Agent', icon: 'agent' },
  ];
</script>

<nav class="fixed bottom-0 left-0 right-0 bg-white border-t flex justify-around py-2">
  {#each navItems as item}
    <a 
      href={item.path}
      class="flex flex-col items-center px-4 py-1 { $page.url.pathname === item.path ? 'text-blue-500' : 'text-gray-600' }"
    >
      <div class="w-6 h-6">{item.icon}</div>
      <span class="text-xs mt-1">{item.label}</span>
    </a>
  {/each}
</nav>
```

#### 移动端头部 (MobileHeader.svelte)
```svelte
<script lang="ts">
  let showMenu = $state(false);
</script>

<header class="bg-white border-b px-4 py-3 flex items-center justify-between">
  <button on:click={() => showMenu = !showMenu} class="p-2">
    <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16" />
    </svg>
  </button>
  
  <h1 class="text-lg font-bold">Modular Agent</h1>
  
  <button class="p-2">
    <svg class="w-6 h-6" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 5v.01M12 12v.01M12 19v.01M12 6a1 1 0 110-2 1 1 0 010 2zm0 7a1 1 0 110-2 1 1 0 010 2zm0 7a1 1 0 110-2 1 1 0 010 2z" />
    </svg>
  </button>
</header>

{#if showMenu}
  <div class="fixed inset-0 bg-black bg-opacity-50 z-40" on:click={() => showMenu = false}>
    <div class="absolute left-0 top-0 bottom-0 w-64 bg-white p-4" on:click|stopPropagation>
      <!-- 菜单内容 -->
    </div>
  </div>
{/if}
```

### 5.3 触摸手势支持

#### 手势处理 (src/lib/utils/gestures.ts)
```typescript
export interface GestureHandlers {
  onTap?: (event: TouchEvent) => void;
  onSwipeLeft?: (event: TouchEvent) => void;
  onSwipeRight?: (event: TouchEvent) => void;
  onPinch?: (scale: number) => void;
}

export function setupGestures(element: HTMLElement, handlers: GestureHandlers) {
  let startX = 0;
  let startY = 0;
  let startTime = 0;

  element.addEventListener('touchstart', (e) => {
    const touch = e.touches[0];
    startX = touch.clientX;
    startY = touch.clientY;
    startTime = Date.now();
  });

  element.addEventListener('touchend', (e) => {
    const touch = e.changedTouches[0];
    const deltaX = touch.clientX - startX;
    const deltaY = touch.clientY - startY;
    const deltaTime = Date.now() - startTime;

    // Tap
    if (Math.abs(deltaX) < 10 && Math.abs(deltaY) < 10 && deltaTime < 300) {
      handlers.onTap?.(e);
      return;
    }

    // Swipe
    if (Math.abs(deltaX) > 50 && Math.abs(deltaY) < 50) {
      if (deltaX > 0) {
        handlers.onSwipeRight?.(e);
      } else {
        handlers.onSwipeLeft?.(e);
      }
    }
  });
}
```

## 六、部署配置

### 6.1 Docker 配置

#### Dockerfile (前端)
```dockerfile
# 构建阶段
FROM node:22-alpine AS builder

WORKDIR /app

# 安装依赖
COPY package.json pnpm-lock.yaml ./
RUN npm install -g pnpm && pnpm install --frozen-lockfile

# 构建应用
COPY . .
RUN pnpm build

# 生产阶段
FROM nginx:alpine

# 复制构建产物
COPY --from=builder /app/build /usr/share/nginx/html

# 复制 nginx 配置
COPY nginx.conf /etc/nginx/nginx.conf

EXPOSE 80

CMD ["nginx", "-g", "daemon off;"]
```

#### Dockerfile (后端)
```dockerfile
FROM node:22-alpine

WORKDIR /app

# 安装依赖
COPY package.json pnpm-lock.yaml ./
RUN npm install -g pnpm && pnpm install --frozen-lockfile --prod

# 复制源代码
COPY . .

# 构建应用
RUN pnpm build

EXPOSE 3000 3001

CMD ["node", "dist/index.js"]
```

### 6.2 Kubernetes 配置

#### Deployment (k8s/deployment.yaml)
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: web-app-backend
spec:
  replicas: 3
  selector:
    matchLabels:
      app: web-app-backend
  template:
    metadata:
      labels:
        app: web-app-backend
    spec:
      containers:
      - name: backend
        image: modular-agent/web-app-backend:latest
        ports:
        - containerPort: 3000
        - containerPort: 3001
        env:
        - name: NODE_ENV
          value: production
        - name: JWT_SECRET
          valueFrom:
            secretKeyRef:
              name: web-app-secrets
              key: jwt-secret
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
---
apiVersion: v1
kind: Service
metadata:
  name: web-app-backend
spec:
  selector:
    app: web-app-backend
  ports:
  - port: 80
    targetPort: 3000
    name: http
  - port: 3001
    targetPort: 3001
    name: websocket
  type: LoadBalancer
```

## 七、开发任务清单

### 7.1 多用户支持
- [ ] 实现用户注册功能
- [ ] 实现用户登录功能
- [ ] 实现用户登出功能
- [ ] 实现用户管理页面
- [ ] 实现认证中间件
- [ ] 实现 JWT token 管理

### 7.2 权限管理
- [ ] 定义权限模型
- [ ] 实现权限检查中间件
- [ ] 实现前端权限组件
- [ ] 实现角色管理
- [ ] 实现权限配置界面

### 7.3 国际化
- [ ] 创建语言文件
- [ ] 实现国际化服务
- [ ] 实现语言切换组件
- [ ] 翻译所有界面文本
- [ ] 支持日期和数字格式化

### 7.4 插件系统
- [ ] 定义插件接口
- [ ] 实现插件管理器
- [ ] 实现插件加载机制
- [ ] 实现插件 API
- [ ] 创建示例插件

### 7.5 移动端适配
- [ ] 实现响应式布局
- [ ] 创建移动端组件
- [ ] 优化触摸交互
- [ ] 实现手势支持
- [ ] 优化移动端性能

### 7.6 部署配置
- [ ] 创建 Docker 配置
- [ ] 创建 Kubernetes 配置
- [ ] 配置 CI/CD 流程
- [ ] 编写部署文档

## 八、验收标准

### 8.1 功能验收
- ✅ 多用户系统正常工作
- ✅ 权限控制正确
- ✅ 国际化切换正常
- ✅ 插件系统可用
- ✅ 移动端适配良好

### 8.2 部署验收
- ✅ Docker 镜像构建成功
- ✅ Kubernetes 部署成功
- ✅ 服务可正常访问
- ✅ 性能指标达标

## 九、时间估算

- 多用户支持: 3 天
- 权限管理: 2 天
- 国际化: 2 天
- 插件系统: 3 天
- 移动端适配: 3 天
- 部署配置: 2 天
- **总计: 约 15 天**

## 十、项目总结

### 10.1 总体时间估算

- **第一阶段**: 14 天 (基础框架)
- **第二阶段**: 16 天 (可视化增强)
- **第三阶段**: 14 天 (功能完善)
- **第四阶段**: 15 天 (扩展功能)
- **总计**: 约 59 天 (约 2 个月)

### 10.2 技术栈总结

**前端**:
- Svelte 5 + SvelteKit
- TailwindCSS
- D3.js (可视化)
- SSE Client (EventSource)

**后端**:
- Node.js + Express
- SSE Endpoints
- JWT 认证
- Docker + Kubernetes

### 10.3 核心特性

1. **前后端分离**: 独立开发、独立部署
2. **实时通信**: SSE 服务端推送
3. **可视化编辑**: 工作流图形化编辑
4. **多用户支持**: 完整的用户和权限系统
5. **插件化架构**: 可扩展的插件系统
6. **国际化**: 多语言支持
7. **移动端适配**: 响应式设计

### 10.4 后续优化方向

1. **性能优化**: 进一步优化加载速度和运行性能
2. **测试覆盖**: 完善单元测试和集成测试
3. **监控告警**: 添加系统监控和告警机制
4. **文档完善**: 编写详细的用户和开发文档
5. **社区建设**: 建立插件生态和开发者社区
