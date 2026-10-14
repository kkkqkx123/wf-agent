<script lang="ts">
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import { sessions } from '$lib/stores/sessions.svelte';
	import { formatNumber } from '$lib/utils/format';
	import { appPath } from '$lib/utils/route';
	import type { ChatSessionController } from '$lib/chat/chat-session.svelte';

	interface Props {
		controller: ChatSessionController;
	}

	let { controller }: Props = $props();

	const session = $derived(controller.session);
	const selectedId = $derived(controller.selectedId);
</script>

<div class="flex h-12 shrink-0 items-center gap-2 border-b border-border px-3">
	<div class="min-w-0 flex-1">
		{#if session}
			<div class="flex min-w-0 items-center gap-2">
				<h1 class="truncate text-title font-semibold">
					{sessions.label(session.id)}
				</h1>
				<StatusBadge status={session.status} size="sm" />
				<Badge variant="outline">
					iteration {formatNumber(session.iteration)}
				</Badge>
			</div>
		{:else}
			<h1 class="truncate text-title font-semibold">New session</h1>
		{/if}
	</div>
	{#if selectedId}
		<IconButton
			icon="star"
			label={sessions.isStarred(selectedId) ? 'Unstar session' : 'Star session'}
			onclick={() => void sessions.toggleStar(selectedId)}
		/>
	{/if}
	<IconButton
		icon="refresh"
		label="Refresh session"
		onclick={() => controller.refresh()}
	/>
	{#if selectedId}
		<Button
			variant="outline"
			size="sm"
			href={appPath(`/executions/${selectedId}`)}
		>
			<Icon name="arrow-right" size={13} />
			Full detail
		</Button>
	{/if}
</div>
