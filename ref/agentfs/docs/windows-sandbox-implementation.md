# Windows沙箱实现方案

## 概述

AgentFS当前在Linux和macOS平台上实现了沙箱功能，但Windows平台尚未支持。本文档介绍了在Windows环境下实现类似沙箱功能的多种方案。

## 当前AgentFS沙箱架构

### Linux平台
- 使用FUSE（用户态文件系统）实现文件系统虚拟化
- 利用Linux命名空间（namespaces）实现进程隔离
- 通过复制写入（copy-on-write）机制保护主机文件系统
- 所有操作记录到SQLite数据库中

### macOS平台
- 使用NFS实现文件系统访问
- 利用Apple的sandbox-exec机制实现权限控制
- 同样使用SQLite数据库记录所有操作

### Windows平台
- 当前`agentfs run`命令在Windows上不可用
- 代码中明确返回错误："The `run` command is not supported on Windows"

## Windows沙箱实现方案

### 方案一：Windows Sandbox（推荐）

Windows Sandbox是Windows 10/11内置的轻量级虚拟化环境，提供以下特性：

#### 优势
- **硬件级隔离**：基于hypervisor的虚拟化，提供内核级隔离
- **一次性环境**：每次关闭后自动丢弃所有更改
- **纯净环境**：每次启动都是全新的Windows环境
- **安全可靠**：微软官方支持，企业级安全
- **配置灵活**：通过.wsb配置文件自定义资源分配

#### 配置示例
```xml
<Configuration>
    <VGpu>Enable</VGpu>
    <MappedFolders>
        <MappedFolder>
            <HostFolder>C:\Users\YourName\agentfs_work</HostFolder>
            <ReadOnly>false</ReadOnly>
        </MappedFolder>
    </MappedFolders>
    <LogonCommand>
        <Command>cmd.exe /C C:\Users\WDAGUtilityAccount\Desktop\init_agentfs.bat</Command>
    </LogonCommand>
</Configuration>
```

#### 与AgentFS集成
- 可以将SQLite数据库文件映射到沙箱中
- 通过初始化脚本设置AgentFS环境
- 利用沙箱的临时性实现复制写入效果

### 方案二：Windows容器

利用Docker或Windows容器技术实现进程隔离：

#### 优势
- **轻量级**：比传统虚拟机更轻量
- **快速启动**：秒级启动时间
- **资源控制**：精确的CPU、内存控制
- **镜像管理**：可复用的环境镜像

#### 实现方式
```dockerfile
FROM mcr.microsoft.com/windows/servercore:ltsc2019
COPY agentfs.exe /usr/bin/
RUN mkdir /agentfs_workspace
WORKDIR /agentfs_workspace
ENTRYPOINT ["agentfs", "run"]
```

#### 与AgentFS集成
- 创建专用的AgentFS容器镜像
- 使用卷挂载实现数据持久化
- 通过容器网络实现安全通信

### 方案三：AIO Sandbox集成

AIO Sandbox提供一体化的沙箱环境，包含浏览器、Shell、文件操作等功能：

#### 特性
- **统一接口**：REST API和MCP协议支持
- **多环境集成**：VNC、VSCode、Jupyter、Terminal
- **安全执行**：沙箱化的Python和Node.js环境
- **零配置**：预配置开发工具

#### 与AgentFS集成
```python
class AgentFSWindowsSandbox:
    def __init__(self, base_url: str = "http://localhost:8080"):
        self.base_url = base_url
        
    async def execute_shell(self, command: str) -> dict:
        """在沙箱中执行Shell命令"""
        url = f"{self.base_url}/v1/shell/exec"
        payload = {"command": command}
        # 实现API调用逻辑
        pass
        
    async def read_file(self, file_path: str) -> dict:
        """从沙箱读取文件"""
        # 实现文件读取逻辑
        pass
```

### 方案四：Job Objects + 权限控制

使用Windows原生API实现轻量级隔离：

#### 实现要点
- **Job Objects**：限制进程组的资源使用
- **令牌过滤**：降低进程权限
- **文件系统ACL**：控制目录访问权限
- **注册表虚拟化**：隔离注册表访问

#### 代码示例
```c
HANDLE hJob = CreateJobObject(NULL, NULL);
JOBOBJECT_BASIC_LIMIT_INFORMATION jobLimits = {0};
jobLimits.LimitFlags = JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
jobLimits.ActiveProcessLimit = 1;
SetInformationJobObject(hJob, JobObjectBasicLimitInformation, &jobLimits, sizeof(jobLimits));

// 将进程加入作业对象
AssignProcessToJobObject(hJob, hProcess);
```

## SQLite与虚拟文件系统的集成

### 当前AgentFS实现
AgentFS使用SQLite作为虚拟文件系统的后端存储，通过以下表结构实现：

#### 文件系统表结构
- `fs_inode`：存储文件和目录的元数据
- `fs_dentry`：存储目录项（路径到inode的映射）
- `fs_data`：存储文件内容（按块分割）
- `fs_symlink`：存储符号链接目标

### Windows沙箱中的SQLite集成

无论选择哪种沙箱方案，SQLite都可以作为统一的数据存储层：

#### 优势
- **便携性**：单个文件便于传输和备份
- **事务性**：ACID特性保证数据一致性
- **查询能力**：SQL支持复杂查询和分析
- **审计功能**：结构化存储便于审计

#### 实现策略
1. **沙箱内操作**：所有文件操作在沙箱内转换为SQLite操作
2. **数据同步**：定期或按需同步SQLite数据到主机
3. **隔离保证**：通过沙箱机制防止直接文件系统访问

## 推荐实现路径

### 短期方案：Windows Sandbox集成
1. 创建.wsb配置文件模板
2. 实现初始化脚本生成
3. 集成SQLite数据库映射
4. 提供命令行包装器

### 长期方案：原生Windows沙箱
1. 开发Windows服务实现文件系统过滤器
2. 集成SQLite作为存储后端
3. 实现复制写入机制
4. 提供与现有API兼容的接口

## 结论

虽然Windows平台缺乏Linux FUSE和命名空间的等效功能，但通过Windows Sandbox、容器技术或其他隔离机制，完全可以实现类似AgentFS的沙箱功能。SQLite作为存储后端，在各种方案中都能很好地发挥作用，提供统一的数据模型和强大的查询能力。