# 性能基准测试

本目录包含 Layertwine 项目的性能基准测试，使用 Criterion 库进行测试。

## 基准测试概览

### 1. Diff 引擎 (`diff.rs`)
测试差异计算引擎的性能：
- `diff_to_line_diff`: 行级差异计算
- `format_unified_diff`: 统一diff格式化
- 不同大小的文件（10行、100行、1000行、10000行）
- 不同的变更率（10%、30%、50%）

### 2. Merge 引擎 (`merge.rs`)
测试合并引擎的性能：
- `apply_deltas`: 应用Delta链重建内容
- `merge_texts`: 三路文本合并
- 不同数量的Delta（1、5、10个）
- 不同大小的文件

### 3. Inverse 引擎 (`inverse.rs`)
测试反向Delta生成引擎的性能：
- `inverse_delta`: 生成反向Delta
- 单独测试插入、删除、替换操作
- 测试有/无old_content的情况

### 4. 核心类型计算 (`core_types.rs`)
测试核心类型和计算的性能：
- `content_id_from_content`: Blake3内容哈希计算
- `delta_compute_id`: Delta ID计算
- `snapshot_compute_id`: Snapshot ID计算
- 不同大小的数据（16字节到16KB）

### 5. 存储操作 (`storage.rs`)
测试存储层的性能：
- `store_snapshot`: Snapshot存储
- `store_delta`: Delta存储
- `get_snapshot`: Snapshot查询
- `get_delta`: Delta查询
- 批量操作性能

### 6. 序列化/反序列化 (`serialization.rs`)
测试序列化和反序列化的性能：
- Delta序列化/反序列化
- Snapshot序列化/反序列化
- LineDiff序列化
- FileNode序列化

## 运行基准测试

### 运行所有基准测试
```bash
cargo bench
```

### 运行特定基准测试文件
```bash
# 运行 Diff 引擎基准测试
cargo bench --bench diff

# 运行 Merge 引擎基准测试
cargo bench --bench merge

# 运行 Inverse 引擎基准测试
cargo bench --bench inverse

# 运行核心类型基准测试
cargo bench --bench core_types

# 运行存储操作基准测试
cargo bench --bench storage

# 运行序列化基准测试
cargo bench --bench serialization
```

### 运行特定基准测试组
```bash
# 运行小型文件基准测试
cargo bench --bench diff diff_small

# 运行中型文件基准测试
cargo bench --bench diff diff_medium

# 运行大型文件基准测试
cargo bench --bench diff diff_large
```

### 运行特定基准测试函数
```bash
cargo bench --bench diff diff_to_line_diff_small_10_lines_10_percent
```

## 基准测试结果

基准测试结果会保存在 `target/criterion/` 目录中。

### 查看结果
```bash
# 使用标准浏览器查看HTML报告
open target/criterion/report/index.html

# 或使用特定浏览器
firefox target/criterion/report/index.html
```

### 比较历史结果
```bash
cargo bench -- --save-baseline new_name
cargo bench -- --baseline new_name
```

## 依赖

基准测试依赖于以下 crate：
- `criterion`: Rust的性能基准测试库
- `tempfile`: 临时文件创建（用于存储测试）

## 基准测试最佳实践

1. **运行环境**：在相同硬件环境下运行基准测试以确保结果可比较
2. **多次运行**：基准测试会自动运行多次以获得稳定的结果
3. **避免干扰**：运行基准测试时避免运行其他CPU密集型任务
4. **保存结果**：使用 `--save-baseline` 选项保存基准测试结果以供比较
5. **关注趋势**：关注性能趋势而非单次运行的绝对值

## 性能目标

以下是预期的性能目标（基于设计和实现）：

### Diff 引擎
- 小型文件（<100行）：<1μs
- 中型文件（100-1000行）：<10μs
- 大型文件（1000-10000行）：<100μs

### Merge 引擎
- 应用单个Delta：<1μs
- 三路合并（小型文件）：<5μs
- 三路合并（大型文件）：<50μs

### 内容哈希
- Blake3哈希计算：~1GB/s

### 存储操作
- Snapshot存储：<1ms（100行文件）
- Delta存储：<1ms（100行文件）
- 查询操作：<100μs

## 添加新的基准测试

要添加新的基准测试：

1. 在 `benches/` 目录中创建新的 `.rs` 文件
2. 在 `Cargo.toml` 中添加对应的 `[[bench]]` 条目
3. 使用 Criterion 的宏定义基准测试组和基准测试函数
4. 运行 `cargo bench --bench <benchmark_name>` 测试

## 故障排除

### Gnuplot not found 警告
如果看到 "Gnuplot not found, using plotters backend" 警告，可以安全忽略。基准测试仍会正常运行，只是图表生成会使用替代方案。

### 性能波动
如果基准测试结果有较大波动，可以：
- 增加迭代次数（在代码中调整）
- 确保系统负载稳定
- 关闭不必要的后台进程
- 使用 `--sample-size` 参数增加样本数

### 编译错误
如果遇到编译错误：
- 确保所有依赖已正确安装
- 检查 `Cargo.toml` 中的依赖版本
- 运行 `cargo clean && cargo build --benches` 重新构建