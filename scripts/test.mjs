#!/usr/bin/env node

/**
 * 测试脚本：支持从根目录直接运行特定测试文件
 *
 * 用法：
 *   pnpm test                              # 全部测试 (Turbo)
 *   pnpm test -- file.ts              # 特定文件 (直接 vitest)
 *   pnpm test -- file.ts -t "pattern" # 特定测试模式 (直接 vitest)
 *   pnpm test -- apps/web-app/src/...    # web-app 包的测试
 *   pnpm test -- packages/common-utils/src/...  # common-utils 包的测试
 *   pnpm test -- sdk/src/...             # sdk 包的测试
 */

import { execSync } from 'child_process'
import process from 'process'
import path from 'path'

// 获取 -- 之后的所有参数
const args = process.argv.slice(2)

// 检查是否有参数
if (args.length === 0) {
	// 没有参数，运行全部测试
	try {
		execSync('turbo test --log-order grouped --output-logs new-only', {
			stdio: 'inherit',
			shell: true
		})
	} catch (error) {
		process.exit(error.status || 1)
	}
} else {
	// 有参数，直接调用 vitest 而不通过 Turbo
	// 这跳过了 Turbo 的冗余信息和全量包测试
	try {
		// 检测目标包：apps/*, packages/*, 或 sdk
		// 规范化路径：将反斜杠转换为正斜杠（处理 Windows 路径）
		const firstArg = (args[0] || '').replace(/\\/g, '/')

		let targetDir = process.cwd()
		let prefixToRemove = ''

		if (firstArg.startsWith('apps/')) {
			// 提取 apps/xxx 部分
			const match = firstArg.match(/^apps\/([^\/]+)/)
			if (match) {
				targetDir = path.join(process.cwd(), 'apps', match[1])
				prefixToRemove = `apps/${match[1]}/`
			}
		} else if (firstArg.startsWith('packages/')) {
			// 提取 packages/xxx 部分
			const match = firstArg.match(/^packages\/([^\/]+)/)
			if (match) {
				targetDir = path.join(process.cwd(), 'packages', match[1])
				prefixToRemove = `packages/${match[1]}/`
			}
		} else if (firstArg.startsWith('sdk/')) {
			targetDir = path.join(process.cwd(), 'sdk')
			prefixToRemove = 'sdk/'
		}

		// 使用根目录的 vitest，因为所有包共享同一个 vitest 配置和版本
		const vitestBin = path.join(process.cwd(), 'node_modules/.bin/vitest')

		// 处理参数：将路径转换为相对路径（因为 vitest 在目标目录下执行）
		const processedArgs = args.map(arg => {
			// 规范化路径：将反斜杠转换为正斜杠（处理 Windows 路径）
			let normalizedArg = arg.replace(/\\/g, '/')

			// 消除多个连续的斜杠
			normalizedArg = normalizedArg.replace(/\/+/g, '/')

			// 如果是文件路径参数（不是以 - 开头的选项）
			if (!normalizedArg.startsWith('-') && normalizedArg.includes('/') && prefixToRemove) {
				// 移除前缀（如 apps/web-app/, packages/common-utils/, sdk/）
				normalizedArg = normalizedArg.replace(new RegExp(`^${prefixToRemove.replace('/', '\\/')}`), '')
			}

			return normalizedArg
		})

		// 为参数添加引号以保护空格和特殊字符
		const quotedArgs = processedArgs.map(arg => {
			if (arg.includes(' ')) {
				return `"${arg}"`
			}
			return arg
		})
		const testArgs = quotedArgs.join(' ')
		// 在目标目录中执行
		execSync(`${vitestBin} run ${testArgs}`, {
			stdio: 'inherit',
			shell: true,
			cwd: targetDir
		})
	} catch (error) {
		process.exit(error.status || 1)
	}
}
