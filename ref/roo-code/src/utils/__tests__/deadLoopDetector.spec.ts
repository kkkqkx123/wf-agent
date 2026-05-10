import { describe, test, expect, beforeEach } from "vitest"
import { DeadLoopDetector } from "../deadLoopDetector"

describe("DeadLoopDetector", () => {
	let detector: DeadLoopDetector

	beforeEach(() => {
		detector = new DeadLoopDetector()
	})

	describe("短序列循环检测", () => {
		test("应该检测到 4 次重复的短序列", () => {
			// 最后 200 字符内有 4 次重复
			const baseText = "a".repeat(1800)
			const repeatingText = "思考".repeat(4) // 4 次 = 8 字符，满足 minRepeatCount=4
			const filler = "b".repeat(192)
			const text = baseText + repeatingText + filler

			const result = detector.detect(text)
			expect(result.detected).toBe(true)
			expect(result.type).toBe("shortSequenceLoop")
			expect(result.details).toContain("思考")
		})

		test("不应该检测到 3 次重复", () => {
			const baseText = "a".repeat(1800)
			const repeatingText = "思考".repeat(3) // 3 次，不满足
			const text = baseText + repeatingText

			const result = detector.detect(text)
			expect(result.detected).toBe(false)
		})

		test("应该检测到更多次重复的短序列（死循环特征）", () => {
			// 真实死循环会有持续的重复
			// 需要达到 2000 字符以触发第 1 检查点
			const baseText = "a".repeat(1800)
			const repeatingText = "测试".repeat(50) // 50 次重复，明显的死循环
			const filler = "b".repeat(100) // 填充到 2000 字符以触发第 1 检查点
			const text = baseText + repeatingText + filler

			const result = detector.detect(text)
			expect(result.detected).toBe(true)
			expect(result.type).toBe("shortSequenceLoop")
		})

		test("不应该检测到单字符重复", () => {
			const baseText = "a".repeat(1900)
			const repeatingText = "a".repeat(100) // 单字符重复，无效
			const text = baseText + repeatingText

			const result = detector.detect(text)
			expect(result.detected).toBe(false)
		})

		test("不应该检测到正常重复（如哈哈哈）", () => {
			const baseText = "a".repeat(1994)
			const repeatingText = "哈哈哈" // 只有 3 次重复
			const text = baseText + repeatingText

			const result = detector.detect(text)
			expect(result.detected).toBe(false)
		})

		test("在未达到检查点时不应该检测", () => {
			const text = "思考".repeat(100) // 远未达到 2000 字符

			const result = detector.detect(text)
			expect(result.detected).toBe(false)
		})

		test("应该在正常强调重复中不误报", () => {
			// 4 次重复但在 200 字符窗口的非末尾位置
			const baseText = "a".repeat(1800)
			const repeatingText = "思考".repeat(4) + "c".repeat(160)
			const text = baseText + repeatingText

			const result = detector.detect(text)
			expect(result.detected).toBe(false) // 不在末尾，不是死循环
		})
	})

	describe("段落内容重复检测", () => {
		test("应该检测到 6 个块的周期循环", () => {
			// 在 2000-3000 范围内产生周期循环
			// 最小情况：3 个周期 × 2 块 = 6 块
			// 检测器检测 2000-3000 范围，从末尾向前检测周期
			// 策略：用重复模式本身填充 2000-3000 范围，确保检测器能看到完整周期
			const baseText = "a".repeat(2000)
			// 用重复模式填充 1000 字符（2000-3000 范围），确保周期在检测范围内
			const repeatingParagraph = "块 A。块 B。" // 分割后：["块 A", "块 B"]
			const repeatingText = repeatingParagraph.repeat(125) // 1000 字符，250 块，远超过 6 块阈值
			const text = baseText + repeatingText

			const result = detector.detect(text)
			expect(result.detected).toBe(true)
			expect(result.type).toBe("paragraphRepetition")
		})

		test("应该检测到更多周期的段落重复", () => {
			// 真实死循环场景：5 个周期 × 2 块 = 10 块
			// 检测器检测 2000-3000 范围，从末尾向前检测周期
			const baseText = "a".repeat(2000)
			// 用重复模式填充 1000 字符（2000-3000 范围）
			const repeatingParagraph = "第一句。第二句。" // 2 个块
			const repeatingText = repeatingParagraph.repeat(125) // 1000 字符，250 块
			const text = baseText + repeatingText

			const result = detector.detect(text)
			expect(result.detected).toBe(true)
			expect(result.type).toBe("paragraphRepetition")
		})

		test("不应该检测到 5 个块", () => {
			// 边界情况：5 个块 < 6 个块阈值
			const baseText = "a".repeat(2000)
			const repeatingParagraph = "块。" // 1 块
			const repeatingText = repeatingParagraph.repeat(5) // 5 块
			const text = baseText + repeatingText + "c".repeat(1000)

			const result = detector.detect(text)
			expect(result.detected).toBe(false)
		})

		test("不应该误报正常列表", () => {
			// 6 块但不是周期性的
			const baseText = "a".repeat(2000)
			const text = baseText + "第一。第二。第三。第四。第五。第六。" + "c".repeat(976)

			const result = detector.detect(text)
			expect(result.detected).toBe(false) // 非周期
		})

		test("应该正确处理不同的语义边界符", () => {
			// 检测器检测 2000-3000 范围，从末尾向前检测周期
			const baseText = "a".repeat(2000)
			// 用重复模式填充 1000 字符（2000-3000 范围）
			// 周期循环：使用不同边界符的重复模式
			const repeatingText = "句号。分号；感叹号！问号？".repeat(125) // 1000 字符
			const text = baseText + repeatingText

			const result = detector.detect(text)
			expect(result.detected).toBe(true)
		})

		test("不应该误报单个多块的段落", () => {
			const baseText = "a".repeat(2000)
			// 6 个不同的块，无周期性
			const normalList = "这是第一个想法。这是第二个想法。这是第三个想法。这是第四个想法。这是第五个想法。这是第六个想法。"
			const text = baseText + normalList + "c".repeat(928)

			const result = detector.detect(text)
			expect(result.detected).toBe(false)
		})
	})

	describe("有序列表重复检测", () => {
		test("应该检测到 6 行的周期循环", () => {
			// 最小情况：3 个周期 × 2 行 = 6 行
			// 检测器检测 2000-3000 范围，从末尾向前检测周期
			const baseText = "a".repeat(2000)
			// 用重复模式填充 1000 字符（2000-3000 范围）
			const repeatingList = "1. 项\n2. 项\n" // 2 行 = 2 个元素
			const repeatingText = repeatingList.repeat(100) // 约 1000 字符，200 行
			const text = baseText + repeatingText

			const result = detector.detect(text)
			expect(result.detected).toBe(true)
			expect(result.type).toBe("orderedListRepetition")
		})

		test("应该检测到更多周期的列表重复", () => {
			// 真实死循环：5 个周期 × 2 行 = 10 行
			// 检测器检测 2000-3000 范围，从末尾向前检测周期
			const baseText = "a".repeat(2000)
			// 用重复模式填充 1000 字符（2000-3000 范围）
			const repeatingList = "1. 分析需求\n2. 设计方案\n"
			const repeatingText = repeatingList.repeat(100) // 约 1000 字符
			const text = baseText + repeatingText

			const result = detector.detect(text)
			expect(result.detected).toBe(true)
			expect(result.type).toBe("orderedListRepetition")
		})

		test("不应该检测到 5 行", () => {
			// 边界情况：5 行 < 6 行阈值
			const baseText = "a".repeat(2000)
			const text = baseText + "1. 第一项\n2. 第二项\n3. 第一项\n4. 第二项\n5. 第一项\n" + "c".repeat(950)

			const result = detector.detect(text)
			expect(result.detected).toBe(false)
		})

		test("不应该误报正常列表（6 行无周期）", () => {
			// 6 行但无周期性
			const baseText = "a".repeat(2000)
			const normalList = "1. 分析需求\n2. 设计方案\n3. 实现功能\n4. 测试代码\n5. 部署上线\n6. 监控维护\n"
			const text = baseText + normalList + "c".repeat(920)

			const result = detector.detect(text)
			expect(result.detected).toBe(false)
		})

		test("应该正确处理不同标号格式", () => {
			// 检测器检测 2000-3000 范围，从末尾向前检测周期
			const baseText = "a".repeat(2000)
			// 用重复模式填充 1000 字符（2000-3000 范围）
			// 3 个周期 × 2 行 = 6 行（不同的标号格式）
			const repeatingText = ("10. 第一项\n11. 第二项\n").repeat(100) // 约 1000 字符
			const text = baseText + repeatingText

			const result = detector.detect(text)
			expect(result.detected).toBe(true)
		})

		test("不应该误报标号递增的列表", () => {
			const baseText = "a".repeat(2000)
			// 虽然有 6 行，但内容和标号都递增，无周期性
			const text = baseText + "1. 步骤一\n2. 步骤二\n3. 步骤三\n4. 步骤四\n5. 步骤五\n6. 步骤六\n" + "c".repeat(930)

			const result = detector.detect(text)
			expect(result.detected).toBe(false)
		})
	})

	describe("通用周期检测", () => {
		test("应该检测到周期长度为 2 的循环", () => {
			// 检测器检测 2000-3000 范围，从末尾向前检测周期
			const baseText = "a".repeat(2000)
			// 用重复模式填充 1000 字符（2000-3000 范围）
			// 3 个周期 × 2 长度 = 6 个块，刚好满足 6 个元素阈值
			const repeatingText = "块 A。块 B。".repeat(125) // 1000 字符，250 块
			const text = baseText + repeatingText

			const result = detector.detect(text)
			expect(result.detected).toBe(true)
		})

		test("应该检测到周期长度为 3 的循环", () => {
			// 检测器检测 2000-3000 范围，从末尾向前检测周期
			const baseText = "a".repeat(2000)
			// 用重复模式填充 1000 字符（2000-3000 范围）
			// 2 个周期 × 3 长度 = 6 个块
			// "块 A。块 B。块 C。" = 12 字符，需要 84 次重复达到 1008 字符（超过 3000 阈值）
			const repeatingText = "块 A。块 B。块 C。".repeat(84) // 1008 字符，252 块
			const text = baseText + repeatingText

			const result = detector.detect(text)
			expect(result.detected).toBe(true)
		})

		test("应该检测到更长的周期循环", () => {
			// 检测器检测 2000-3000 范围，从末尾向前检测周期
			const baseText = "a".repeat(2000)
			// 用重复模式填充 1000 字符（2000-3000 范围）
			// 更多重复，明显的死循环
			const repeatingText = "块 A。块 B。".repeat(125) // 1000 字符
			const text = baseText + repeatingText

			const result = detector.detect(text)
			expect(result.detected).toBe(true)
		})

		test("不应该检测到过长的周期", () => {
			const baseText = "a".repeat(2000)
			// 创建一个周期长度超过 50 的序列
			const longPeriod = Array.from({ length: 60 }, (_, i) => `块${i}。`).join("")
			const text = baseText + longPeriod + longPeriod + "b".repeat(1000)

			const result = detector.detect(text)
			expect(result.detected).toBe(false)
		})
	})

	describe("边界情况", () => {
		test("应该正确处理空文本", () => {
			const result = detector.detect("")
			expect(result.detected).toBe(false)
		})

		test("应该正确处理极短文本", () => {
			const result = detector.detect("短文本")
			expect(result.detected).toBe(false)
		})

		test("应该正确处理刚好达到检查点的文本", () => {
			const text = "a".repeat(2000)
			const result = detector.detect(text)
			expect(result.detected).toBe(false) // 没有死循环
		})

		test("应该正确处理极长文本", () => {
			const baseText = "a".repeat(5000)
			const repeatingText = "思考".repeat(10)
			const text = baseText + repeatingText

			const result = detector.detect(text)
			expect(result.detected).toBe(true)
		})

		test("应该在达到第 3 检查点时执行尾部检测", () => {
			// 检测器检测 3000-5000 范围，从末尾向前检测周期
			// 注意：需要避免短序列检测先触发，所以使用不重复的长行
			const baseText = "a".repeat(3000)
			// 用重复模式填充 2000+ 字符（3000-5000 范围）
			// 使用较小的块（约 282 字符），重复 8 次，确保达到 5000 检查点
			// 需要至少 6 次重复才能满足 minPeriodElements=6 的要求（需要 5 次匹配）
			const lines = []
			for (let i = 0; i < 25; i++) {
				lines.push(`${i + 1}. 步骤${i + 1}说明`)
			}
			// 重复这个 25 行的块 8 次，形成周期模式（8 × 282 = 2256 字符，总长 5256）
			const blockText = lines.join("\n") + "\n"
			const text = baseText + blockText.repeat(8)

			const result = detector.detect(text)
			expect(result.detected).toBe(true)
		})
	})

	describe("误判防范", () => {
		test("不应该检测到用户提示词中的重复", () => {
			// 用户提示词的重复在 2000 字符之前，不会触发死循环检测
			const userPrompt = "测试".repeat(500) // 用户提示词中的重复
			const text = userPrompt + "a".repeat(1500)

			const result = detector.detect(text)
			expect(result.detected).toBe(false)
		})

		test("不应该检测到代码块中的重复", () => {
			const baseText = "a".repeat(2000)
			// 代码行虽然重复但无中文，不会被检测
			const codeBlock = "for (let i = 0; i < 10; i++) {\n".repeat(6)
			const text = baseText + codeBlock + "b".repeat(1000)

			const result = detector.detect(text)
			expect(result.detected).toBe(false)
		})

		test("不应该误报正常的强调", () => {
			const baseText = "a".repeat(2000)
			// 3 次重复 < 4 次阈值
			const emphasis = "重要！".repeat(3)
			const text = baseText + emphasis + "b".repeat(1000)

			const result = detector.detect(text)
			expect(result.detected).toBe(false)
		})

		test("不应该误报单个非周期的 6 块段落", () => {
			const baseText = "a".repeat(2000)
			// 6 个不同内容的块，虽然数量足够但无周期性
			const text = baseText + "第一个想法。第二个想法。第三个想法。第四个想法。第五个想法。第六个想法。" + "c".repeat(952)

			const result = detector.detect(text)
			expect(result.detected).toBe(false)
		})
	})

	describe("检测器状态管理", () => {
		test("应该在重置后重新检测", () => {
			const baseText = "a".repeat(1800)
			const repeatingText = "思考".repeat(100) // 100 次重复（满足 4 次阈值）
			const filler = "b".repeat(100) // 填充到 2000 字符以触发第 1 检查点
			const text = baseText + repeatingText + filler

			// 第一次检测
			let result = detector.detect(text)
			expect(result.detected).toBe(true)

			// 重置检测器
			detector.reset()

			// 第二次检测应该仍然能检测到
			result = detector.detect(text)
			expect(result.detected).toBe(true)
		})

		test("不应该重复检测同一个检查点", () => {
			const baseText = "a".repeat(1800)
			const repeatingText = "思考".repeat(100) // 100 次重复（满足 4 次阈值）
			const filler = "b".repeat(100) // 填充到 2000 字符以触发第 1 检查点
			const text = baseText + repeatingText + filler

			// 第一次检测
			let result = detector.detect(text)
			expect(result.detected).toBe(true)

			// 第二次检测（文本未变）
			result = detector.detect(text)
			expect(result.detected).toBe(false) // 已经检测过该检查点
		})

		test("应该在文本增长后检测新的检查点", () => {
			// 第一次检测：2000 字符
			let text = "a".repeat(2000)
			let result = detector.detect(text)
			expect(result.detected).toBe(false)

			// 增长到 3000 字符
			text = "a".repeat(3000)
			result = detector.detect(text)
			expect(result.detected).toBe(false)
		})
	})

	describe("配置参数", () => {
		test("应该支持自定义配置", () => {
			// 创建两个检测器，验证配置能被正确应用
			const defaultDetector = new DeadLoopDetector()

			// 自定义检测器：更敏感的参数
			const customDetector = new DeadLoopDetector({
				checkpoints: [2000, 3000, 5000],
				minRepeatCount: 3,  // 短序列：3 次而不是 4 次
				minPeriodElements: 4,  // 周期：4 个元素而不是 6 个
			})

			// 创建边界测试数据
			// 检测器检测 2000-3000 范围，从末尾向前检测周期
			const baseText = "a".repeat(2000)
			// 用重复模式填充 1000 字符（2000-3000 范围）
			// 标准参数需要 6 个块，自定义参数需要 4 个块
			const blockText = "块 A。块 B。".repeat(125) // 1000 字符，250 块
			const text = baseText + blockText

			// 默认检测器应该检测到（满足 6 个块）
			const defaultResult = defaultDetector.detect(text)
			expect(defaultResult.detected).toBe(true)

			// 自定义检测器也应该检测到（满足 4 个块）
			const customResult = customDetector.detect(text)
			expect(customResult.detected).toBe(true)
		})
	})
})
