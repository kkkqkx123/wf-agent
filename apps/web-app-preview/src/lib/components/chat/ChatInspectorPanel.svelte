<script lang="ts">
	import SessionInspector from '$lib/components/domain/SessionInspector.svelte';
	import { chatStream } from '$lib/stores/stream-run.svelte';
	import type { ChatSessionController } from '$lib/chat/chat-session.svelte';

	interface Props {
		controller: ChatSessionController;
	}

	let { controller }: Props = $props();

	const selectedId = $derived(controller.selectedId);
</script>

<SessionInspector
	sessionId={selectedId ?? ''}
	bind:tab={controller.tab}
	revision={controller.revision}
	busy={chatStream.active}
	liveStatuses={{
		...Object.fromEntries(
			controller.projectedTools.map((tool) => [tool.name, tool.status]),
		),
		...Object.fromEntries(
			controller.projectedSubAgents.map((agent) => [
				agent.name,
				agent.success === null
					? 'running'
					: agent.success
						? 'completed'
						: 'failed',
			]),
		),
	}}
	liveNode={controller.miniCurrentNode}
	focusId={controller.miniSelected}
	onnodeselect={(id) => {
		if (id) {
			controller.focusToolFromNode(id);
		}
	}}
	class="h-full"
/>
