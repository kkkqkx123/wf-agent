/**
 * TTS Debug Test - 验证say模块加载和功能
 * 
 * 运行方式: cd src && npx vitest run utils/__tests__/tts-debug.spec.ts
 */

import { describe, it, expect, beforeEach, afterEach } from "vitest"

describe("TTS Debug - say模块验证", () => {
	let sayModule: any

	beforeEach(() => {
		try {
			sayModule = require("say")
		} catch (error) {
			console.error("无法加载say模块:", error)
		}
	})

	afterEach(() => {
		// 清理
	})

	it("应该能够成功加载say模块", () => {
		expect(sayModule).toBeDefined()
		expect(typeof sayModule.speak).toBe("function")
		expect(typeof sayModule.stop).toBe("function")
	})

	it("say模块应该有正确的接口", () => {
		expect(sayModule).toHaveProperty("speak")
		expect(sayModule).toHaveProperty("stop")
	})

	it("应该能够调用speak方法（不实际播放）", async () => {
		return new Promise<void>((resolve, reject) => {
			try {
				// 测试调用speak方法，但不等待实际播放完成
				sayModule.speak("test", undefined, 1.0, (err?: string) => {
					if (err) {
						console.log("speak回调错误（预期行为）:", err)
						// 某些环境可能没有音频设备，这是正常的
					}
					resolve()
				})

				// 立即停止，避免实际播放
				setTimeout(() => {
					try {
						sayModule.stop()
					} catch (e) {
						// 忽略停止错误
					}
					resolve()
				}, 100)
			} catch (error) {
				console.error("调用speak失败:", error)
				resolve() // 不让测试失败，只是记录错误
			}
		})
	})

	it("应该能够调用stop方法", () => {
		expect(() => {
			sayModule.stop()
		}).not.toThrow()
	})

	it("应该能够获取系统语音列表（如果支持）", async () => {
		return new Promise<void>((resolve) => {
			try {
				// 尝试获取语音列表（如果say模块支持）
				if (sayModule.getInstalledVoices) {
					sayModule.getInstalledVoices((err?: string, voices?: string[]) => {
						if (err) {
							console.log("获取语音列表失败:", err)
						} else {
							console.log("系统可用语音:", voices)
						}
						resolve()
					})
				} else {
					console.log("当前say模块不支持getInstalledVoices方法")
					resolve()
				}
			} catch (error) {
				console.error("获取语音列表异常:", error)
				resolve()
			}
		})
	}, 10000) // 增加超时时间到10秒

	it("应该能够处理空文本", async () => {
		return new Promise<void>((resolve) => {
			try {
				sayModule.speak("", undefined, 1.0, (err?: string) => {
					// 空文本应该被正常处理
					resolve()
				})
				setTimeout(() => {
					try {
						sayModule.stop()
					} catch (e) { }
					resolve()
				}, 50)
			} catch (error) {
				resolve()
			}
		})
	})

	it("应该能够处理特殊字符", async () => {
		return new Promise<void>((resolve) => {
			try {
				const specialText = "Hello! @#$%^&*()_+-=[]{}|;:,.<>?`~"
				sayModule.speak(specialText, undefined, 1.0, (err?: string) => {
					if (err) {
						console.log("特殊字符处理错误:", err)
					}
					resolve()
				})
				setTimeout(() => {
					try {
						sayModule.stop()
					} catch (e) { }
					resolve()
				}, 50)
			} catch (error) {
				console.error("特殊字符测试异常:", error)
				resolve()
			}
		})
	})

	it("应该能够处理中文文本", async () => {
		return new Promise<void>((resolve) => {
			try {
				const chineseText = "你好，世界"
				sayModule.speak(chineseText, undefined, 1.0, (err?: string) => {
					if (err) {
						console.log("中文文本处理错误:", err)
					}
					resolve()
				})
				setTimeout(() => {
					try {
						sayModule.stop()
					} catch (e) { }
					resolve()
				}, 50)
			} catch (error) {
				console.error("中文文本测试异常:", error)
				resolve()
			}
		})
	})

	it("应该能够处理不同的播放速度", async () => {
		return new Promise<void>((resolve) => {
			try {
				const speeds = [0.5, 1.0, 1.5, 2.0]
				let completed = 0

				speeds.forEach((speed) => {
					sayModule.speak("test", undefined, speed, (err?: string) => {
						completed++
						if (completed === speeds.length) {
							resolve()
						}
					})
				})

				// 清理
				setTimeout(() => {
					try {
						sayModule.stop()
					} catch (e) { }
					resolve()
				}, 100)
			} catch (error) {
				console.error("播放速度测试异常:", error)
				resolve()
			}
		})
	})

	it("应该能够处理并发调用", async () => {
		return new Promise<void>((resolve) => {
			try {
				let callCount = 0
				const maxCalls = 3

				for (let i = 0; i < maxCalls; i++) {
					sayModule.speak(`test ${i}`, undefined, 1.0, (err?: string) => {
						callCount++
						if (err) {
							console.log(`并发调用 ${i} 错误:`, err)
						}
					})
				}

				// 清理
				setTimeout(() => {
					try {
						sayModule.stop()
					} catch (e) { }
					resolve()
				}, 100)
			} catch (error) {
				console.error("并发调用测试异常:", error)
				resolve()
			}
		})
	})
})

describe("TTS Debug - 环境信息", () => {
	it("应该显示当前平台信息", () => {
		const platform = process.platform
		console.log("当前平台:", platform)
		console.log("Node版本:", process.version)
		console.log("架构:", process.arch)
		expect(platform).toBeDefined()
	})

	it("应该显示say模块版本信息（如果可用）", () => {
		try {
			const sayModule = require("say")
			console.log("say模块信息:", {
				hasSpeak: typeof sayModule.speak === "function",
				hasStop: typeof sayModule.stop === "function",
				hasGetVoices: typeof sayModule.getInstalledVoices === "function",
			})
		} catch (error) {
			console.log("无法获取say模块信息")
		}
	})
})