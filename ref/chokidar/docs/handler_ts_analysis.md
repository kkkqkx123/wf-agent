# handler.ts 文件处理逻辑分析

## 概述
`handler.ts` 文件是 chokidar 库的核心处理模块，主要负责文件系统事件的实际处理逻辑。它提供了 Node.js 原生 `fs.watch` 和 `fs.watchFile` 的封装，以及对符号链接、二进制文件等特殊情况的处理。

## 核心功能模块

### 1. 平台检测与常量定义
- 定义了跨平台相关的常量，如 `isWindows`、`isMacos`、`isLinux` 等
- 定义了事件类型常量 `EVENTS`，包括 `ADD`、`CHANGE`、`UNLINK` 等
- 定义了二进制文件扩展名集合 `binaryExtensions`

### 2. 工具函数
- `foreach`: 遍历 Set 或单个值的通用函数
- `addAndConvert`: 将值添加到容器中，如果容器不是 Set 则转换为 Set
- `clearItem`: 清空容器中的项
- `delFromSet`: 从 Set 中删除指定项
- `isEmptySet`: 检查容器是否为空

### 3. fs.watch 实例管理
- `FsWatchInstances` Map 存储每个路径对应的 fs.watch 实例
- `createFsWatchInstance`: 创建 fs.watch 实例并设置事件处理器
- `fsWatchBroadcast`: 向特定路径的所有监听器广播事件
- `setFsWatchListener`: 设置或复用 fs.watch 监听器

### 4. fs.watchFile 实例管理
- `FsWatchFileInstances` Map 存储每个路径对应的 fs.watchFile 实例
- `setFsWatchFileListener`: 设置或复用 fs.watchFile 监听器

### 5. NodeFsHandler 类
这是文件系统处理的主要类，包含以下核心方法：

#### `_watchWithNodeFs`
- 使用 `fs.watch` 或 `fs.watchFile` 监视文件或目录
- 根据配置选项决定使用哪种监视方式
- 对二进制文件使用不同的轮询间隔

#### `_handleFile`
- 处理单个文件的监视
- 当文件发生更改时触发 `CHANGE` 事件
- 处理 inode 变化的情况（在 macOS、Linux、FreeBSD 上）

#### `_handleSymlink`
- 处理符号链接
- 如果不跟随符号链接，则直接监视符号链接本身
- 如果跟随符号链接，则解析实际路径

#### `_handleRead`
- 读取目录内容
- 过滤掉被忽略的路径
- 为新发现的文件/目录添加监视

#### `_handleDir`
- 处理目录监视
- 发出 `ADD_DIR` 事件
- 递归监视子目录

#### `_addToNodeFs`
- 添加路径到 Node.js 文件系统监视器
- 检测路径类型（文件、目录、符号链接）
- 调用相应的处理方法

## 关键特性

### 1. 实例共享机制
通过 `FsWatchInstances` 和 `FsWatchFileInstances` 实现了监视实例的共享，避免对同一路径重复创建监视器。

### 2. 错误处理
- 特殊处理 Windows 平台上的 `EPERM` 错误
- 提供统一的错误处理机制

### 3. 性能优化
- 使用节流机制防止重复事件
- 共享监视实例减少资源消耗
- 二进制文件使用不同轮询间隔

### 4. 符号链接支持
- 支持跟随或不跟随符号链接
- 正确处理符号链接的变化

## 数据结构

### FsWatchContainer
```typescript
{
  listeners: (path: string) => void | Set<any>;
  errHandlers: (err: unknown) => void | Set<any>;
  rawEmitters: (ev: WatchEventType, path: string, opts: unknown) => void | Set<any>;
  watcher: NativeFsWatcher;
  watcherUnusable?: boolean;
}
```

### WatchHandlers
```typescript
{
  listener: (path: string) => void;
  errHandler: (err: unknown) => void;
  rawEmitter: (ev: WatchEventType, path: string, opts: unknown) => void;
}
```

## 与其他模块的关系
- 与 `index.ts` 中的 `FSWatcher` 类紧密协作
- 通过 `NodeFsHandler` 类提供底层文件系统操作的具体实现
- 处理由主类传递过来的监视请求并返回事件

## 总结
`handler.ts` 是 chokidar 库中负责具体文件系统监视操作的核心模块，它抽象了 Node.js 原生 API 的复杂性，提供了跨平台一致的监视接口，并处理了各种边界情况和性能优化。