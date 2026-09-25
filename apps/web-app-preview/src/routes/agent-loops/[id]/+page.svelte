<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import SessionInspector from '$lib/components/domain/SessionInspector.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import {
		cancelAgentLoop,
		createAgentLoopCheckpoint,
		getAgentLoop,
		pauseAgentLoop,
		resumeAgentLoop,
		restoreAgentLoopCheckpoint,
	} from '$lib/services/agent-loops';
	import type { Checkpoint } from '$lib/types/models';
	import { createResource } from '$lib/stores/collection.svelte';
	import { sessions } from '$lib/stores/sessions.svelte';
	import {
		DETAIL_TABS,
		isSessionTab,
		type SessionTab,
	} from '$lib/config/session-tabs';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatNumber } from '$lib/utils/format';
	import { appPath, gotoWithParams, parseListParams } from '$lib/utils/route';

	const TERMINAL = ['completed', 'failed', 'cancelled'];

	const id = $derived(page.params.id as string);

	const initial = parseListParams(page.url);
	let tab = $state<SessionTab>(isSessionTab(initial.tab) ?? 'messages');
	let revision = $state(0);
	let busy = $state(false);

	const loop = createResource(() => getAgentLoop(id));
	const status = $derived(loop.data?.status ?? '');

	function reload(): void {
		revision += 1;
		void loop.reload();
	}

	onMount(() => {
		void loop.reload();
	});

	async function step(action: 'pause' | 'resume'): Promise<void> {
		busy = true;
		try {
			if (action === 'pause') await pauseAgentLoop(id);
			else await resumeAgentLoop(id);
			reload();
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Action failed');
		} finally {
			busy = false;
		}
	}

	async function recordCheckpoint(): Promise<void> {
		busy = true;
		try {
			await createAgentLoopCheckpoint(id);
			toasts.success('Checkpoint recorded');
			reload();
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Checkpoint failed');
		} finally {
			busy = false;
		}
	}

	// Both remaining mutations discard in-flight work, so they are confirm-gated.
	let target = $state<
		{ kind: 'cancel' } | { kind: 'restore'; checkpoint: Checkpoint } | null
	>(null);
	let confirming = $state(false);

	function ask(
		action: { kind: 'cancel' } | { kind: 'restore'; checkpoint: Checkpoint },
	): void {
		target = action;
		confirming = true;
	}

	const confirmCopy = $derived.by(() => {
		if (!target) return { title: '', detail: '' };
		return target.kind === 'cancel'
			? {
					title: 'Cancel agent loop',
					detail: `Loop ${id} stops at its next safe point and stays cancelled.`,
				}
			: {
					title: 'Restore checkpoint',
					detail: `Loop ${id} is rewound to ${target.checkpoint.kind}; later state is discarded.`,
				};
	});

	async function runConfirmed(): Promise<void> {
		const action = target;
		busy = true;
		try {
			if (action?.kind === 'cancel') {
				await cancelAgentLoop(id);
				toasts.success(`Cancelled loop ${id}`);
			} else if (action?.kind === 'restore') {
				await restoreAgentLoopCheckpoint(id, action.checkpoint.id);
				toasts.success('Checkpoint restored');
			}
			confirming = false;
			target = null;
			reload();
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Action failed');
		} finally {
			busy = false;
		}
	}

	$effect(() => {
		gotoWithParams(page.url, {
			tab: tab === 'messages' ? '' : tab,
		});
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader title={sessions.label(id)} description="Agent loop session">
		{#snippet meta()}
			{#if loop.data}
				<StatusBadge status={loop.data.status} />
				<Badge variant="outline"
					>iteration {formatNumber(loop.data.iteration)}</Badge
				>
				<Badge variant="outline"
					>{formatNumber(loop.data.toolCalls)} tool calls</Badge
				>
				<span class="font-mono text-caption text-muted-foreground">{id}</span>
			{/if}
		{/snippet}
		{#snippet actions()}
			<IconButton icon="refresh" label="Refresh loop" onclick={reload} />
			<IconButton
				icon="pause"
				label="Pause loop"
				disabled={busy || status !== 'running'}
				onclick={() => void step('pause')}
			/>
			<IconButton
				icon="play"
				label="Resume loop"
				disabled={busy || !['paused', 'queued'].includes(status)}
				onclick={() => void step('resume')}
			/>
			<Button
				variant="outline"
				size="sm"
				disabled={busy}
				onclick={recordCheckpoint}
			>
				<Icon name="archive" size={13} />
				Checkpoint
			</Button>
			<Button
				variant="outline"
				size="sm"
				disabled={busy || TERMINAL.includes(status)}
				onclick={() => ask({ kind: 'cancel' })}
			>
				<Icon name="square" size={13} />
				Cancel
			</Button>
			<Button variant="outline" size="sm" href={appPath(`/chat?id=${id}`)}>
				<Icon name="sparkles" size={13} />
				Open in chat
			</Button>
		{/snippet}
	</PageHeader>

	{#if loop.error}
		<ErrorState
			title="Failed to load agent loop"
			description={loop.error}
			onretry={reload}
			class="min-h-0 flex-1 rounded-lg border border-border bg-card"
		>
			{#snippet actions()}
				<Button variant="link" size="sm" href="/agent-loops"
					>Back to agent loops</Button
				>
			{/snippet}
		</ErrorState>
	{:else}
		<SessionInspector
			sessionId={id}
			bind:tab
			tabs={DETAIL_TABS}
			{revision}
			{busy}
			onrestore={(checkpoint) => ask({ kind: 'restore', checkpoint })}
			class="min-h-0 flex-1"
		/>
	{/if}
</div>

<Dialog
	bind:open={confirming}
	title={confirmCopy.title}
	description={confirmCopy.detail}
	onclose={() => (target = null)}
>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (confirming = false)}
			>Keep running</Button
		>
		<Button
			variant="destructive"
			size="sm"
			disabled={!target || busy}
			onclick={() => void runConfirmed()}
		>
			{target?.kind === 'restore' ? 'Restore' : 'Cancel loop'}
		</Button>
	{/snippet}
</Dialog>
