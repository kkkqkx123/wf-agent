<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import ExecutionInspector from '$lib/components/domain/ExecutionInspector.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import {
		getExecution,
		listToolCalls,
		listTimeline,
		pauseExecution,
		resumeExecution,
		cancelExecution,
	} from '$lib/services/executions';
	import { createResource } from '$lib/stores/collection.svelte';
	import { live } from '$lib/stores/live.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDuration } from '$lib/utils/format';

	interface ExecutionBundle {
		detail: Awaited<ReturnType<typeof getExecution>>;
		toolCalls: Awaited<ReturnType<typeof listToolCalls>>;
		timeline: Awaited<ReturnType<typeof listTimeline>>;
	}

	const id = $derived(page.params.id as string);

	const bundle = createResource<ExecutionBundle>(async () => {
		const [detail, toolCalls, timeline] = await Promise.all([
			getExecution(id),
			listToolCalls(id),
			listTimeline(id),
		]);
		return { detail, toolCalls, timeline };
	});

	let cancelOpen = $state(false);
	let cancelling = $state(false);
	let acting = $state(false);

	async function control(action: 'pause' | 'resume'): Promise<void> {
		acting = true;
		try {
			if (action === 'pause') await pauseExecution(id);
			else await resumeExecution(id);
			toasts.success(
				action === 'pause' ? 'Pause requested' : 'Resume requested',
			);
			await bundle.reload();
		} catch (e) {
			toasts.error(
				e instanceof Error ? e.message : 'Failed to change execution state',
			);
		} finally {
			acting = false;
		}
	}

	async function confirmCancel(): Promise<void> {
		cancelling = true;
		try {
			await cancelExecution(id);
			toasts.success('Cancel requested');
			cancelOpen = false;
			await bundle.reload();
		} catch (e) {
			toasts.error(
				e instanceof Error ? e.message : 'Failed to cancel execution',
			);
		} finally {
			cancelling = false;
		}
	}

	onMount(() => {
		void bundle.reload();
		let pending: ReturnType<typeof setTimeout> | null = null;
		const unsubscribe = live.subscribe((event) => {
			if (event.executionId !== id) return;
			if (!event.type.startsWith('WORKFLOW_EXECUTION_')) return;
			if (pending) return;
			pending = setTimeout(() => {
				pending = null;
				void bundle.reload();
			}, 1000);
		});
		return () => {
			if (pending) clearTimeout(pending);
			unsubscribe();
		};
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={bundle.data?.detail.workflowName ?? 'Execution detail'}
		description="Full execution detail with state, timeline and analysis."
	>
		{#snippet meta()}
			{#if bundle.data}
				<StatusBadge status={bundle.data.detail.status} />
				<Badge variant="outline"
					>{formatDuration(bundle.data.detail.durationMs)}</Badge
				>
				<span class="font-mono text-caption text-muted-foreground"
					>{bundle.data.detail.id}</span
				>
				{#if bundle.data.detail.trigger}
					<span class="text-caption text-muted-foreground"
						>{bundle.data.detail.trigger}</span
					>
				{/if}
			{/if}
		{/snippet}
		{#snippet actions()}
			<IconButton
				icon="pause"
				label="Pause execution"
				disabled={acting || !bundle.data}
				onclick={() => control('pause')}
			/>
			<IconButton
				icon="play"
				label="Resume execution"
				disabled={acting || !bundle.data}
				onclick={() => control('resume')}
			/>
			<Button
				variant="outline"
				size="sm"
				disabled={!bundle.data}
				onclick={() => (cancelOpen = true)}
			>
				<Icon name="square" size={13} />
				Cancel
			</Button>
			<Button variant="outline" size="sm" href="/executions">
				<Icon name="arrow-left" size={13} />
				Back to workbench
			</Button>
		{/snippet}
	</PageHeader>

	<div class="min-h-0 flex-1 overflow-hidden px-4 pb-4">
		{#if bundle.loading && !bundle.data}
			<div
				class="flex h-full flex-col gap-3 overflow-hidden rounded-lg border border-border bg-card p-4"
			>
				<Skeleton shape="block" height="28px" class="w-64 rounded-md" />
				<Skeleton shape="block" height="240px" class="flex-1 rounded-md" />
			</div>
		{:else if bundle.error}
			<EmptyState
				icon="alert-triangle"
				title="Failed to load execution"
				description={bundle.error}
				class="h-full rounded-lg border border-border bg-card"
			>
				{#snippet actions()}
					<Button variant="link" size="sm" onclick={() => bundle.reload()}
						>Retry</Button
					>
				{/snippet}
			</EmptyState>
		{:else if bundle.data}
			<div
				class="h-full overflow-hidden rounded-lg border border-border bg-card"
			>
				<ExecutionInspector
					execution={bundle.data.detail}
					toolCalls={bundle.data.toolCalls}
					timeline={bundle.data.timeline}
				/>
			</div>
		{/if}
	</div>
</div>

<Dialog
	bind:open={cancelOpen}
	title="Cancel this execution"
	description="The execution is cancelled at the next safe point and cannot be resumed afterwards."
>
	<p class="text-caption text-muted-foreground">
		Execution <span class="font-mono text-foreground">{id}</span> will be marked as
		cancelled.
	</p>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (cancelOpen = false)}>
			Keep running
		</Button>
		<Button
			variant="destructive"
			size="sm"
			disabled={cancelling}
			onclick={confirmCancel}
		>
			{cancelling ? 'Cancelling…' : 'Cancel execution'}
		</Button>
	{/snippet}
</Dialog>
