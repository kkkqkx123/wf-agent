<script lang="ts">
	import { onDestroy } from 'svelte';
	import type { ChatSessionController } from '#lib/chat/chat-session.svelte';
	import {
		cancelChatStep,
		fetchChatTree,
		stepChatSession,
		subscribeChatSession,
	} from '#lib/services/streaming';
	import type { ChatTreeSnapshot } from '#lib/services/streaming';

	interface Props {
		controller: ChatSessionController;
	}

	let { controller }: Props = $props();

	const selectedId = $derived(controller.selectedId);
	let snapshot = $state<ChatTreeSnapshot | null>(null);
	let error = $state('');
	let jobId = $state('');
	let streamController: AbortController | null = null;

	async function refresh(id: string): Promise<void> {
		try {
			snapshot = await fetchChatTree(id);
			error = '';
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		}
	}

	async function watch(id: string): Promise<void> {
		streamController?.abort();
		streamController = new AbortController();
		const signal = streamController.signal;
		await refresh(id);
		void subscribeChatSession(id, (event) => {
			if (
				event === 'tree.changed' ||
				event === 'dirty.changed' ||
				event === 'turn.completed' ||
				event === 'turn.failed' ||
				event === 'turn.cancelled' ||
				event === 'step.completed' ||
				event === 'step.stopped'
			) {
				void refresh(id);
			}
		}, signal);
	}

	$effect(() => {
		const id = selectedId;
		if (!id) {
			snapshot = null;
			streamController?.abort();
			streamController = null;
			return;
		}
		void watch(id);
		return () => {
			streamController?.abort();
			streamController = null;
		};
	});

	onDestroy(() => streamController?.abort());

	async function step(mode: 'single' | 'forward', nodeId?: string): Promise<void> {
		if (!selectedId) return;
		try {
			const result = await stepChatSession(selectedId, { mode, nodeId });
			if (result.job_id) {
				jobId = result.job_id;
				await refresh(selectedId);
			}
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		}
	}

	async function cancel(): Promise<void> {
		if (!selectedId || !jobId) return;
		await cancelChatStep(selectedId, jobId);
		jobId = '';
	}
</script>

<aside class="flex w-64 shrink-0 flex-col gap-2 border-r border-base-300 p-2 text-sm">
	<div class="flex items-center justify-between font-semibold">
		<span>Tree</span>
		<span class="opacity-60">v{snapshot?.version ?? '-'}</span>
	</div>

	{#if error}
		<div class="text-error text-xs">{error}</div>
	{/if}

	<div class="min-h-0 flex-1 overflow-y-auto">
		{#if !snapshot || snapshot.nodes.length === 0}
			<div class="opacity-60">No tree nodes yet.</div>
		{:else}
			<ul class="space-y-1">
				{#each snapshot.nodes as node (node.id)}
					<li
						class="flex items-center gap-2 rounded px-2 py-1"
						class:bg-base-300={node.id === snapshot.active_leaf}
						class:opacity-50={node.deleted}
					>
						<span>{node.kind === 'premise' ? '⌑' : '❯'}</span>
						<span class="min-w-0 flex-1 truncate">{node.id.slice(-8)}</span>
						{#if node.dirty}
							<span class="badge badge-warning badge-xs">stale</span>
						{/if}
						{#if node.reply?.model_drift}
							<span class="badge badge-ghost badge-xs">model</span>
						{/if}
						<button class="btn btn-ghost btn-xs" onclick={() => step('single', node.id)}>
							step
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</div>

	<div class="flex flex-wrap gap-1">
		<button class="btn btn-xs" onclick={() => step('single')}>Single</button>
		<button class="btn btn-xs" onclick={() => step('forward')}>Forward</button>
		{#if jobId}
			<button class="btn btn-error btn-xs" onclick={cancel}>Cancel</button>
		{/if}
	</div>
</aside>
