// 通过 require("say") 动态导入

interface Say {
	speak: (text: string, voice?: string, speed?: number, callback?: (err?: string) => void) => void
	stop: () => void
}

type PlayTtsOptions = {
	onStart?: () => void
	onStop?: () => void
}

type QueueItem = {
	message: string
	options: PlayTtsOptions
}

let isTtsEnabled = false

export const setTtsEnabled = (enabled: boolean) => {
	isTtsEnabled = enabled
	console.log("[TTS] isTtsEnabled set to: " + enabled)
	return isTtsEnabled
}

let speed = 1.0

export const setTtsSpeed = (newSpeed: number) => {
	speed = newSpeed
	console.log("[TTS] speed set to: " + newSpeed)
	return speed
}

let sayInstance: Say | undefined = undefined
let queue: QueueItem[] = []

export const playTts = async (message: string, options: PlayTtsOptions = {}) => {
	const messagePreview = message.length > 50 ? message.substring(0, 50) + "..." : message
	console.log("[TTS] playTts called with message: " + messagePreview)
	console.log("[TTS] isTtsEnabled: " + isTtsEnabled)

	if (!isTtsEnabled) {
		console.warn("[TTS] TTS is disabled, skipping playback. Enable TTS in settings to use this feature.")
		return
	}

	if (!message || message.trim() === "") {
		console.warn("[TTS] playTts called with empty or whitespace-only message")
		return
	}

	try {
		queue.push({ message, options })
		console.log("[TTS] Message added to queue, queue length: " + queue.length)
		await processQueue()
	} catch (error: any) {
		console.error("[TTS] Error in playTts: " + error.message)
		console.error("[TTS] Stack trace: " + error.stack)
		console.error("[TTS] Failed message preview: " + messagePreview)
	}
}

export const stopTts = () => {
	console.log("[TTS] stopTts called")
	try {
		if (sayInstance) {
			sayInstance.stop()
			console.log("[TTS] sayInstance.stop() called successfully")
		} else {
			console.log("[TTS] No active sayInstance to stop")
		}
	} catch (error: any) {
		console.error("[TTS] Error stopping TTS: " + error.message)
	} finally {
		sayInstance = undefined
		queue = []
		console.log("[TTS] Queue cleared, sayInstance reset")
	}
}

const processQueue = async (): Promise<void> => {
	console.log("[TTS] processQueue called, isTtsEnabled: " + isTtsEnabled + ", sayInstance: " + !!sayInstance + ", queue length: " + queue.length)

	if (!isTtsEnabled) {
		console.warn("[TTS] processQueue: TTS is disabled, aborting queue processing")
		return
	}

	if (sayInstance) {
		console.log("[TTS] Skipping - sayInstance already exists (audio is currently playing)")
		return
	}

	const item = queue.shift()

	if (!item) {
		console.log("[TTS] Queue is empty, nothing to process")
		return
	}

	const messagePreview = item.message.length > 50 ? item.message.substring(0, 50) + "..." : item.message
	console.log("[TTS] Processing message: " + messagePreview)
	console.log("[TTS] Message length: " + item.message.length + ", speed: " + speed)

	try {
		const { message: nextUtterance, options } = item

		await new Promise<void>((resolve, reject) => {
			let say: Say | undefined

			try {
				say = require("say")
				console.log("[TTS] say module loaded successfully")
			} catch (requireError: any) {
				console.error("[TTS] CRITICAL: Failed to load say module: " + requireError.message)
				console.error("[TTS] require.stack: " + requireError.stack)
				console.error("[TTS] This may indicate the 'say' npm package is not installed correctly")
				reject(requireError)
				return
			}

			sayInstance = say
			console.log("[TTS] sayInstance assigned, calling onStart callback")

			try {
				if (options.onStart) {
					options.onStart()
					console.log("[TTS] onStart callback executed successfully")
				}
			} catch (onStartError: any) {
				console.error("[TTS] Warning: onStart callback threw error: " + onStartError.message)
			}

			console.log("[TTS] Calling say.speak with text length: " + nextUtterance.length + ", speed: " + speed)

			try {
				say!.speak(nextUtterance, undefined, speed, (err) => {
					console.log("[TTS] say.speak callback called, err: " + (err || "none"))

					try {
						if (options.onStop) {
							options.onStop()
							console.log("[TTS] onStop callback executed successfully")
						}
					} catch (onStopError: any) {
						console.error("[TTS] Warning: onStop callback threw error: " + onStopError.message)
					}

					if (err) {
						console.error("[TTS] Playback error reported by say.speak: " + err)
						console.error("[TTS] Text that failed: " + messagePreview)
						reject(new Error(err))
					} else {
						console.log("[TTS] Playback completed successfully for message: " + messagePreview)
						resolve()
					}

					sayInstance = undefined
					console.log("[TTS] sayInstance reset after playback")
				})
			} catch (speakError: any) {
				console.error("[TTS] say.speak threw synchronous error: " + speakError.message)
				console.error("[TTS] Stack: " + speakError.stack)
				sayInstance = undefined
				reject(speakError)
			}
		})

		console.log("[TTS] Continuing with next item in queue, remaining items: " + queue.length)

		if (queue.length > 0) {
			await processQueue()
		} else {
			console.log("[TTS] Queue fully processed")
		}
	} catch (error: any) {
		console.error("[TTS] Error in processQueue: " + error.message)
		console.error("[TTS] Stack trace: " + error.stack)
		console.error("[TTS] Failed message: " + messagePreview)
		sayInstance = undefined

		if (queue.length > 0) {
			console.warn("[TTS] Attempting to process next item after error, remaining queue: " + queue.length)
			try {
				await processQueue()
			} catch (nextError: any) {
				console.error("[TTS] Failed to process next item: " + nextError.message)
			}
		} else {
			console.log("[TTS] No more items to process after error")
		}
	}
}
