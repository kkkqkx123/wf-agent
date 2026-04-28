import { EventEmitter } from "events"

import { v4 as uuidv4 } from "uuid"

import { QueuedMessage } from "@coder/types"

export interface MessageQueueState {
	messages: QueuedMessage[]
	isProcessing: boolean
	isPaused: boolean
}

export interface QueueEvents {
	stateChanged: [messages: QueuedMessage[]]
}

/**
 * Handler for processing dequeued messages
 * The handler is called when a message is dequeued and should handle submission
 */
export type QueuedMessageHandler = (message: QueuedMessage) => Promise<void>

export class MessageQueueService extends EventEmitter<QueueEvents> {
	private _messages: QueuedMessage[]
	private _messageHandler?: QueuedMessageHandler

	constructor() {
		super()

		this._messages = []
	}

	/**
	 * Register a handler to process dequeued messages.
	 * The handler will be called automatically when dequeueMessage() is called
	 * and a message is available.
	 *
	 * @param handler - The function to handle dequeued messages
	 */
	public setMessageHandler(handler: QueuedMessageHandler): void {
		this._messageHandler = handler
	}

	/**
	 * Clear the registered message handler
	 */
	public clearMessageHandler(): void {
		this._messageHandler = undefined
	}

	private findMessage(id: string) {
		const index = this._messages.findIndex((msg) => msg.id === id)

		if (index === -1) {
			return { index, message: undefined }
		}

		return { index, message: this._messages[index] }
	}

	public addMessage(text: string, images?: string[]): QueuedMessage | undefined {
		if (!text && !images?.length) {
			return undefined
		}

		const message: QueuedMessage = {
			timestamp: Date.now(),
			id: uuidv4(),
			text,
			images,
		}

		this._messages.push(message)
		this.emit("stateChanged", this._messages)

		return message
	}

	public removeMessage(id: string): boolean {
		const { index, message } = this.findMessage(id)

		if (!message) {
			return false
		}

		this._messages.splice(index, 1)
		this.emit("stateChanged", this._messages)
		return true
	}

	public updateMessage(id: string, text: string, images?: string[]): boolean {
		const { message } = this.findMessage(id)

		if (!message) {
			return false
		}

		message.timestamp = Date.now()
		message.text = text
		message.images = images
		this.emit("stateChanged", this._messages)
		return true
	}

	/**
	 * Dequeue a message and optionally process it with the registered handler.
	 *
	 * If a message handler is registered and a message is available:
	 * 1. The message is removed from the queue
	 * 2. The handler is called to process the message (asynchronously)
	 * 3. Errors from the handler are logged but don't throw
	 *
	 * @param shouldProcess - Whether to automatically invoke the handler if registered (default: true)
	 * @returns The dequeued message (if not auto-processing) or undefined
	 */
	public dequeueMessage(shouldProcess = true): QueuedMessage | undefined {
		const message = this._messages.shift()
		this.emit("stateChanged", this._messages)

		// If a handler is registered and auto-processing is enabled, invoke it asynchronously
		if (message && shouldProcess && this._messageHandler) {
			// Use setTimeout to ensure async processing doesn't block the caller
			setTimeout(() => {
				this._messageHandler?.(message).catch((err) => {
					console.error("[MessageQueueService] Handler error:", err)
				})
			}, 0)
		}

		return message
	}

	public get messages(): QueuedMessage[] {
		return this._messages
	}

	public isEmpty(): boolean {
		return this._messages.length === 0
	}

	public dispose(): void {
		this._messages = []
		this.removeAllListeners()
	}
}
