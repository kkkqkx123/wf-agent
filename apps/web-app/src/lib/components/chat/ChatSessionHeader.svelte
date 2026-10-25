<script lang="ts">
	import Icon from '#lib/ui/icons/Icon.svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import StatusBadge from '#lib/ui/components/StatusBadge.svelte';
	import { sessions } from '#lib/stores/sessions.svelte';
	import { appPath } from '#lib/utils/route';
	import type { SessionExecutorType } from '#lib/types/models';
	import { t, type MessageKey } from '#lib/i18n';
	import type { ChatSessionController } from '#lib/chat/chat-session.svelte';

	/** Message key for how each executor is named in the header. */
	const EXECUTION_KEYS: Record<SessionExecutorType, MessageKey> = {
		agent: 'chat.executionTypePicker.labelAgent',
		chat: 'chat.executionTypePicker.labelChat',
		workflow: 'chat.executionTypePicker.labelWorkflow',
	};

	/** Header label for an executor; unknown kinds fall back to the raw value. */
	function executionLabel(type: SessionExecutorType): string {
		return EXECUTION_KEYS[type] ? t(EXECUTION_KEYS[type]) : type;
	}

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
					{executionLabel(session.executionType)}
				</Badge>
				{#if session.profileId}
					<Badge variant="outline" class="font-mono">
						{session.profileId}
					</Badge>
				{/if}
			</div>
		{:else}
			<h1 class="truncate text-title font-semibold">
				{t('chat.chatSessionHeader.newSession')}
			</h1>
		{/if}
	</div>
	{#if selectedId}
		<IconButton
			icon="star"
			label={sessions.isStarred(selectedId)
				? t('chat.chatSessionHeader.unstar')
				: t('chat.chatSessionHeader.star')}
			onclick={() => void sessions.toggleStar(selectedId)}
		/>
	{/if}
	<IconButton
		icon="refresh"
		label={t('chat.chatSessionHeader.refresh')}
		onclick={() => controller.refresh()}
	/>
	{#if selectedId}
		<Button variant="outline" size="sm" href={appPath(`/executions/${selectedId}`)}>
			<Icon name="arrow-right" size={13} />
			{t('chat.chatSessionHeader.fullDetail')}
		</Button>
	{/if}
</div>
