#!/bin/bash

# 基准测试运行脚本
# 用于运行所有 Layertwine 项目的性能基准测试

set -e

echo "================================"
echo "Layertwine 性能基准测试套件"
echo "================================"
echo ""

# 检查是否安装了必要的工具
command -v cargo >/dev/null 2>&1 || { echo >&2 "需要安装 cargo"; exit 1; }

# 清理之前的构建
echo "清理之前的构建..."
cargo clean

# 运行所有基准测试
echo "================================"
echo "运行所有基准测试..."
echo "================================"
cargo bench

# 检查是否安装了浏览器来查看结果
if command -v xdg-open >/dev/null 2>&1; then
    echo ""
    echo "基准测试完成！"
    echo "正在打开基准测试结果..."
    xdg-open target/criterion/report/index.html
elif command -v open >/dev/null 2>&1; then
    echo ""
    echo "基准测试完成！"
    echo "正在打开基准测试结果..."
    open target/criterion/report/index.html
else
    echo ""
    echo "基准测试完成！"
    echo "结果保存在: target/criterion/report/index.html"
    echo "请使用浏览器打开该文件查看详细结果"
fi

echo ""
echo "================================"
echo "基准测试汇总"
echo "================================"
echo "测试结果已保存在 target/criterion/ 目录中"
echo ""
echo "要运行特定的基准测试，请使用以下命令："
echo "  cargo bench --bench diff      # Diff 引擎"
echo "  cargo bench --bench merge     # Merge 引擎"
echo "  cargo bench --bench inverse   # Inverse 引擎"
echo "  cargo bench --bench core_types    # 核心类型"
echo "  cargo bench --bench storage   # 存储操作"
echo "  cargo bench --bench serialization  # 序列化"
echo ""