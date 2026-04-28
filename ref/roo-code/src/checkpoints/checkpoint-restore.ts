import type { ClineApiReqInfo } from "@coder/types"

import { Task } from "../task/Task"

import { getCheckpointService } from "./checkpoint-service"

import { consolidateTokenUsage as getApiMetrics } from "@coder/core/browser"

/**
 * Options for checkpoint restore operation
 */
export type CheckpointRestoreOptions = {
	ts: number
	commitHash: string
	mode: "preview" | "restore"
	operation?: "delete" | "edit"
}

/**
 * Restore a checkpoint for the current task
 *
 * This function:
 * 1. Restores the workspace files to the checkpoint state
 * 2. Truncates API conversation history to remove deleted messages from prompt context
 * 3. Truncates clineMessages using MessageManager (handles context-management cleanup)
 * 4. Reports deleted API request metrics
 * 5. Cancels the current task to trigger reinitialization
 *
 * @param task - The task to restore checkpoint for
 * @param options - Restore options including timestamp, commit hash, mode, and operation type
 */
export async function checkpointRestore(
	task: Task,
	{ ts, commitHash, mode, operation = "delete" }: CheckpointRestoreOptions,
) {
	const service = await getCheckpointService(task)

	if (!service) {
		return
	}

	const index = task.clineMessages.findIndex((m) => m.ts === ts)

	if (index === -1) {
		return
	}

	const provider = task.providerRef.deref()

	try {
		await service.restoreCheckpoint(commitHash)
		await provider?.postMessageToWebview({ type: "currentCheckpointUpdated", text: commitHash })

		if (mode === "restore") {
			// Calculate metrics from messages that will be deleted (must be done before rewind)
			const deletedMessages = task.clineMessages.slice(index + 1)

			const { totalTokensIn, totalTokensOut, totalCacheWrites, totalCacheReads, totalCost } = getApiMetrics(
				deletedMessages,
			)

			// Explicitly truncate API conversation history to ensure prompt context is properly rewound.
			// This is critical for checkpoint restore to correctly remove deleted messages from the
			// API context, not just from clineMessages.
			//
			// For both "delete" and "edit" operations: keep messages [0, apiIndex) - exclude the target message
			// The edit operation will later re-add the edited message via pending edit processing
			const apiIndex = task.apiConversationHistory.findIndex((m) => m.ts === ts)
			if (apiIndex !== -1) {
				await task.overwriteApiConversationHistory(task.apiConversationHistory.slice(0, apiIndex))
			}

			// Use MessageManager to properly handle context-management events
			// This ensures orphaned Summary messages and truncation markers are cleaned up
			await task.messageManager.rewindToTimestamp(ts, {
				includeTargetMessage: operation === "edit",
			})

			// Report the deleted API request metrics
			await task.say(
				"api_req_deleted",
				JSON.stringify({
					tokensIn: totalTokensIn,
					tokensOut: totalTokensOut,
					cacheWrites: totalCacheWrites,
					cacheReads: totalCacheReads,
					cost: totalCost,
				} satisfies ClineApiReqInfo),
			)
		}

		// The task is already cancelled by the provider beforehand, but we
		// need to re-init to get the updated messages.
		//
		// This was taken from Cline's implementation of the checkpoints
		// feature. The task instance will hang if we don't cancel twice,
		// so this is currently necessary, but it seems like a complicated
		// and hacky solution to a problem that I don't fully understand.
		// I'd like to revisit this in the future and try to improve the
		// task flow and the communication between the webview and the
		// `Task` instance.
		provider?.cancelTask()
	} catch (err) {
		provider?.log("[checkpointRestore] disabling checkpoints for this task")
		task.enableCheckpoints = false
	}
}
