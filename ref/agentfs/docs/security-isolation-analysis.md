# AgentFS 安全隔离措施分析

本文档详细分析 AgentFS 项目采用的安全隔离措施及其工作原理。

## 目录

1. [概述](#概述)
2. [核心隔离机制](#核心隔离机制)
3. [平台特定实现](#平台特定实现)
4. [数据层隔离](#数据层隔离)
5. [安全边界分析](#安全边界分析)

---

## 概述

AgentFS 采用多层次的安全隔离策略，通过组合使用以下技术实现 AI Agent 的安全执行环境：

| 隔离层级 | 技术 | 目的 |
|---------|------|------|
| 文件系统层 | OverlayFS + Copy-on-Write | 隔离文件修改，保护主机文件系统 |
| 命名空间层 | Linux Namespaces / macOS Sandbox | 进程级资源隔离 |
| 数据持久化层 | SQLite + 加密 | 安全存储 agent 状态和操作历史 |
| 网络层 | 本地绑定/NFS 隔离 | 限制网络访问范围 |

---

## 核心隔离机制

### 1. Overlay 文件系统（写时复制）

**实现位置**: `sdk/rust/src/filesystem/overlayfs.rs`

#### 架构设计

```
┌─────────────────────────────────────┐
│         Agent 进程空间               │
├─────────────────────────────────────┤
│         OverlayFS (虚拟层)           │
├──────────────┬──────────────────────┤
│   Delta 层   │     Base 层           │
│  (可写)      │    (只读)             │
│  AgentFS.db  │    HostFS             │
└──────────────┴──────────────────────┘
```

#### 工作原理

1. **分层结构**:
   - **Base 层**: 主机文件系统（只读）
   - **Delta 层**: AgentFS SQLite 数据库（可写）
   - **Overlay 层**: 虚拟合并视图

2. **查找语义** (Lookup Semantics):
   ```
   1. 检查 Delta 层是否存在 → 返回 Delta 条目
   2. 检查是否有 Whiteout → 返回"未找到"
   3. 检查 Base 层是否存在 → 返回 Base 条目
   4. 返回"未找到"
   ```

3. **写时复制 (Copy-on-Write)**:
   - 当修改 Base 层文件时，先复制到 Delta 层
   - 记录原始 inode 映射 (`fs_origin` 表)
   - 后续操作在 Delta 层进行

4. **Whiteout 机制**:
   - 删除 Base 层文件时，创建 whiteout 标记
   - Whiteout 存储在 `fs_whiteout` 表
   - 使用 trie 结构缓存实现 O(depth) 查找

#### 关键代码片段

```rust
// Whiteout 缓存 - trie 结构
struct WhiteoutNode {
    children: HashMap<String, WhiteoutNode>,
    is_whiteout: bool,
}

// 查找语义实现
async fn lookup(&self, path: &str) -> Result<Option<Entry>> {
    // 1. 检查 Delta
    if let Some(entry) = self.delta.stat(path).await? {
        return Ok(Some(entry));
    }
    
    // 2. 检查 Whiteout
    if self.is_whiteout(&path) {
        return Ok(None);
    }
    
    // 3. 回退到 Base
    self.base.stat(path).await
}
```

#### 安全效果

- **主机保护**: Agent 无法修改原始文件
- **可审计性**: 所有修改记录在 SQLite 中
- **可回滚**: 删除 Delta 层即可恢复原始状态

---

### 2. 命名空间隔离 (Linux)

**实现位置**: `cli/src/sandbox/linux.rs`

#### 使用的 Namespaces

| Namespace | 标志 | 作用 |
|-----------|------|------|
| `CLONE_NEWUSER` | 用户命名空间 | 隔离用户/组 ID |
| `CLONE_NEWNS` | 挂载命名空间 | 隔离挂载点 |

#### 隔离流程

```
┌──────────────────────────────────────────────────────────┐
│  父进程                                                   │
│  1. 启动 FUSE 服务器 (OverlayFS)                          │
│  2. fork() 创建子进程                                     │
│  3. 等待子进程 unshare                                    │
│  4. 写入 uid_map/gid_map 映射                             │
└──────────────────────────────────────────────────────────┘
                          │
                          │ fork()
                          ▼
┌──────────────────────────────────────────────────────────┐
│  子进程                                                   │
│  1. unshare(CLONE_NEWUSER | CLONE_NEWNS)                 │
│  2. 等待父进程写入映射                                    │
│  3. mount --make-rprivate / (隔离挂载传播)                │
│  4. bind mount FUSE → CWD                                │
│  5. remount 其他文件系统为只读                            │
│  6. exec 执行命令                                         │
└──────────────────────────────────────────────────────────┘
```

#### 用户命名空间映射

```rust
// 写入 uid_map (将真实 UID 映射到命名空间内)
fn write_namespace_mappings(child_pid: i32, uid: u32, gid: u32) {
    // 格式：内部 UID  外部 UID  数量
    // 例如：1000  1000  1  (用户保持为自己，不是 root)
    fs::write("/proc/{pid}/uid_map", "{uid} {uid} 1");
    fs::write("/proc/{pid}/gid_map", "{gid} {gid} 1");
    fs::write("/proc/{pid}/setgroups", "deny");
}
```

#### 文件系统只读化

```rust
fn remount_all_readonly_except(writable_path: &Path, allowed_paths: &[PathBuf]) {
    // 1. 先 bind-mount 允许的路径 (创建独立挂载点)
    for allowed in allowed_paths {
        mount(allowed, allowed, MS_BIND);
        mount(allowed, allowed, MS_REMOUNT | MS_BIND | MS_RDONLY);
    }
    
    // 2. 然后 remount / 为只读
    mount("/", "/", MS_REMOUNT | MS_RDONLY);
}
```

#### 安全效果

- **挂载隔离**: 子进程的挂载变更不影响主机
- **权限隔离**: 进程在命名空间内无特权
- **文件系统保护**: 除允许路径外全部只读

---

### 3. macOS Sandbox 隔离

**实现位置**: `cli/src/sandbox/darwin.rs`

#### Sandbox-exec 配置文件生成

```rust
pub fn generate_sandbox_profile(config: &SandboxConfig) -> String {
    let mut profile = Vec::new();
    
    // 默认拒绝写入
    profile.push("(version 1)");
    profile.push(r#"(deny default (with message "agentfs-xxx: write denied"))"#);
    
    // 允许大多数操作
    profile.push("(allow process*)");
    profile.push("(allow file-read*)");  // 允许所有读取
    
    // 仅允许特定写入路径
    profile.push(r#"(allow file-write* (subpath "/tmp"))"#);
    profile.push(r#"(allow file-write* (subpath "/var/folders"))"#);
    profile.push(format!(
        r#"(allow file-write* (subpath "{}"))"#,
        config.mountpoint  // NFS 挂载点
    ));
    
    // 网络限制
    if !config.allow_network {
        profile.push(r#"(allow network* (remote ip "localhost:*"))"#);
    }
    
    profile.join("\n")
}
```

#### 配置文件结构

```lisp
(version 1)

; 默认拒绝写入 (带日志标签)
(deny default (with message "agentfs-session123: write denied"))

; 允许的操作
(allow process*)
(allow file-read*)
(allow mach*)
(allow sysctl*)
(allow signal)

; 允许的写入路径
(allow file-write* (subpath "/private/tmp"))
(allow file-write* (subpath "/Users/me/.agentfs/run/xxx/mnt"))

; 网络限制
(allow network* (remote ip "localhost:*"))
```

#### 安全效果

- **内核级强制**: sandbox-exec 由 macOS 内核执行
- **细粒度控制**: 可精确控制文件/网络/进程权限
- **违规日志**: 所有拒绝操作带会话标签记录

---

## 平台特定实现

### Linux vs macOS 对比

| 特性 | Linux | macOS |
|------|-------|-------|
| 文件系统挂载 | FUSE | NFS |
| 进程隔离 | Namespaces (user+mount) | sandbox-exec |
| 强制执行 | 命名空间边界 | 内核 Sandbox 配置文件 |
| 网络隔离 | 可选 | 通过配置文件限制 |

### 实验性 Ptrace 沙箱 (Linux)

**实现位置**: `sandbox/src/sandbox/mod.rs`

使用 Reverie 框架进行系统调用拦截：

```rust
#[reverie::tool]
impl Tool for Sandbox {
    async fn handle_syscall_event<T: Guest<Self>>(
        &self,
        guest: &mut T,
        syscall: Syscall,
    ) -> Result<i64, Error> {
        // 拦截并虚拟化系统调用
        syscall::dispatch_syscall(guest, syscall, mount_table, &fd_table).await
    }
}
```

---

## 数据层隔离

### 1. SQLite 本地加密

**支持的加密算法**:

| 算法 | 密钥长度 | 模式 |
|------|---------|------|
| AES-256-GCM | 256 位 | 认证加密 |
| AES-128-GCM | 128 位 | 认证加密 |
| AEGIS-256 | 256 位 | 后量子安全 |
| AEGIS-128L | 128 位 | 轻量级 |

#### 加密配置

```bash
# 生成密钥
KEY=$(openssl rand -hex 32)

# 创建加密文件系统
agentfs init --key $KEY --cipher aes256gcm my-secure-agent

# 访问加密文件系统
agentfs fs my-secure-agent --key $KEY --cipher aes256gcm ls /
```

#### 环境变量方式

```bash
export AGENTFS_KEY=$(openssl rand -hex 32)
export AGENTFS_CIPHER=aes256gcm
```

### 2. 审计追踪 (Tool Call Audit Trail)

**表结构**:

```sql
CREATE TABLE tool_calls (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,           -- 工具名称
  parameters TEXT,              -- JSON 参数
  result TEXT,                  -- JSON 结果
  error TEXT,                   -- 错误信息
  started_at INTEGER NOT NULL,  -- 开始时间戳
  completed_at INTEGER NOT NULL,-- 完成时间戳
  duration_ms INTEGER NOT NULL  -- 执行时长
);
```

#### 安全价值

- **完整审计**: 所有 agent 操作可追溯
- **性能分析**: 可查询工具执行性能
- **故障诊断**: 错误操作可精确定位

### 3. 会话隔离

每个会话创建独立的运行目录：

```
~/.agentfs/run/<session-id>/
├── delta.db      # 增量数据库
└── mnt/          # FUSE/NFS 挂载点
```

#### 会话管理

```bash
# 创建新会话
agentfs run --session my-session /bin/bash

# 加入现有会话
agentfs run --session my-session /bin/bash
```

---

## 安全边界分析

### 信任边界

```
┌─────────────────────────────────────────────────────────┐
│                    主机系统 (可信)                       │
│  ┌───────────────────────────────────────────────────┐  │
│  │              AgentFS CLI (可信)                    │  │
│  │  ┌─────────────────────────────────────────────┐  │  │
│  │  │          FUSE/NFS 服务器 (可信)              │  │  │
│  │  └─────────────────────────────────────────────┘  │  │
│  │                      │                             │  │
│  │              ═══════════════                       │  │
│  │              隔离边界                              │  │
│  │              ═══════════════                       │  │
│  │                      │                             │  │
│  │  ┌─────────────────────────────────────────────┐  │  │
│  │  │         Sandbox 进程 (不可信)                │  │  │
│  │  │         - AI Agent 代码                      │  │  │
│  │  │         - 用户命令                           │  │  │
│  │  └─────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

### 允许的可写路径 (默认)

| 路径 | 用途 |
|------|------|
| `~/.amp` | Amp 配置 |
| `~/.cache` | XDG 缓存 |
| `~/.claude` | Claude Code 配置 |
| `~/.codex` | OpenAI Codex 配置 |
| `~/.local` | 本地数据 |
| `~/.npm` | npm 本地仓库 |
| `/tmp` | 临时文件 |

### 潜在安全风险

1. **FUSE/NFS 服务器漏洞**: FUSE/NFS 实现中的漏洞可能绕过隔离
2. **内核漏洞**: Namespace 或 Sandbox 的内核漏洞可能导致逃逸
3. **侧信道攻击**: 共享内核可能泄露信息
4. **资源耗尽**: Agent 可能耗尽磁盘/内存资源

### 缓解措施

| 风险 | 缓解措施 |
|------|---------|
| 文件系统逃逸 | OverlayFS + 只读 remount |
| 权限提升 | 用户命名空间映射 (非 root) |
| 数据泄露 | 本地加密 (AES-256-GCM) |
| 审计缺失 | Tool Call 审计追踪 |

---

## 总结

AgentFS 通过以下多层隔离机制保障安全：

1. **文件系统层**: OverlayFS 写时复制，保护主机文件
2. **命名空间层**: Linux namespaces / macOS sandbox-exec 隔离进程
3. **数据层**: SQLite 加密存储，审计所有操作
4. **会话层**: 独立运行目录，会话间隔离

这些措施共同构成了一个深度防御的安全架构，使 AI Agent 能够在受控环境中安全执行。

---

## 参考文档

- [SPEC.md](../SPEC.md) - AgentFS 规范
- [MANUAL.md](../MANUAL.md) - 用户手册
- [sandbox/src/](../sandbox/src/) - 沙箱实现源码
- [cli/src/sandbox/](../cli/src/sandbox/) - CLI 沙箱模块
