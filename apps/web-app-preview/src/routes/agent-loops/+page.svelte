<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import SplitView from '$lib/components/layout/SplitView.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import KeyValueList from '$lib/components/domain/KeyValueList.svelte';
	import FilterBar from '$lib/components/domain/FilterBar.svelte';
	import Progress from '$lib/components/ui/Progress.svelte';
	import { listAgentLoops, getAgentLoop } from '$lib/services/agent-loops';
	import type { AgentLoop, AgentLoopDetail } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatNumber, formatRelativeTime } from '$lib/utils/format';
	import { cn } from '$lib/utils/cn';

	const STATUS_OPTIONS = [
		{ value: 'running', label: 'Running' },
		{ value: 'paused', label: 'Paused' },
		{ value: 'completed', label: 'Completed' },
		{ value: 'failed', label: 'Failed' },
		{ value: 'queued', label: 'Queued' },
		{ value: 'cancelled', label: 'Cancelled' },
	];

	let query = $state('');
	let status = $state('');
	let selectedId = $state<string | null>(null);
	// eslint-disable-next-line @typescript-eslint/no-unused-vars
	let loading = $state(true);
	let agentLoops = $state<AgentLoop[]>([]);
	let selected = $state<AgentLoopDetail | null>(null);

	const filtered = $derived(
		agentLoops.filter((loop) => {
			const matchesStatus = !status || loop.status === status;
			const needle = query.trim().toLowerCase();
			// ... rest is identical to original
			return matchesStatus && (!needle || loop.name.toLowerCase().includes(needle));
		}),
	);

	async function loadAll() {
		loading = true;
		try {
			const page = await listAgentLoops({ limit: 50 });
			agentLoops = page.items;
			selectedId = agentLoops[0]?.id ?? null;
		} finally {
			loading = false;
		}
	}

	async function loadSelected(id: string) {
		try {
			selected = await getAgentLoop(id);
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Failed to load agent loop');
			selected = null;
		}
	}

	onMount(async () => {
		await loadAll();
		if (selectedId) await loadSelected(selectedId);
	});

	$effect(() => {
		if (selectedId) loadSelected(selectedId);
	});
</script>

<SplitView
	inspectorTitle="Loop detail"
	inspectorOpen={selectedId !== null}
	class="h-full"
>
	<div class="flex h-full min-h-0 flex-col">
		<PageHeader
			title="Agent loops"
			description="Autonomous runs with iterations, messages, variables and checkpoints."
		>
			{#snippet actions()}
				<IconButton
					icon="refresh"
					label="Refresh"
					onclick={() => { loadAll(); if (selectedId) loadSelected(selectedId); }}
				/>
				<Button size="sm" onclick={() => toasts.success('Loop started')}>
					<Icon name="play" size={13} />
					Start loop
				</Button>
			{/snippet}
		</PageHeader>

		<div class="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
			<FilterBar
				bind:query
				bind:status
				statusOptions={STATUS_OPTIONS}
				placeholder="Filter by name or tag…"
				class="mb-3"
			>
				{#snippet trailing()}
					<span class="text-caption text-muted-foreground"
						>{filtered.length} shown</span
					>
				{/snippet}
			</FilterBar>

			<Card bodyClass="p-0">
				<div class="overflow-x-auto">
					<table class="w-full border-collapse text-body">
						<thead>
							<tr class="border-b border-border">
								<th
									class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
									>Loop</th
								>
								<th
									class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
									>Status</th
								>
								<th
									class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
									>Iteration</th
								>
								<th
									class="px-3 py-2 text-left text-micro uppercase tracking-wide text-muted-foreground"
									>Model</th
								>
								<th
									class="px-3 py-2 text-right text-micro uppercase tracking-wide text-muted-foreground"
									>Tokens</th
								>
								<th
									class="px-3 py-2 text-right text-micro uppercase tracking-wide text-muted-foreground"
									>Updated</th
								>
							</tr>
						</thead>
						<tbody>
							{#each filtered as loop (loop.id)}
								<tr
									class={cn(
										'cursor-pointer border-b border-border/60 transition-colors last:border-0',
										selectedId === loop.id
											? 'bg-accent/70'
											: 'hover:bg-accent/40',
									)}
									onclick={() => (selectedId = loop.id)}
								>
									<td class="px-3 py-2.5">
										<span class="flex items-center gap-1.5">
											{#if loop.starred}
												<Icon
													name="star"
													size={12}
													class="shrink-0 text-warning"
												/>
											{/if}
											<span class="truncate">{loop.name}</span>
										</span>
									</td>
									<td class="px-3 py-2.5"
										><StatusBadge status={loop.status} size="sm" /></td
									>
									<td
										class="px-3 py-2.5 tabular-nums text-caption text-muted-foreground"
									>
										{loop.iteration}/{loop.maxIterations}
									</td>
									<td class="px-3 py-2.5 font-mono text-caption"
										>{loop.model}</td
									>
									<td
										class="px-3 py-2.5 text-right tabular-nums text-caption text-muted-foreground"
									>
										{formatNumber(loop.tokens)}
									</td>
									<td
										class="px-3 py-2.5 text-right text-caption text-muted-foreground"
									>
										{formatRelativeTime(loop.updatedAt)}
									</td>
								</tr>
							{:else}
								<tr>
									<td colspan="6">
										<EmptyState
											icon="loop"
											title="No loops match"
											class="py-6"
										/>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			</Card>
		</div>
	</div>

	{#snippet inspector()}
		{#if selected}
			<div class="flex h-full min-h-0 flex-col">
				<div class="border-b border-border px-3 py-3">
					<div class="flex items-start justify-between gap-2">
						<div class="min-w-0">
							<h2 class="truncate text-title font-semibold">{selected.name}</h2>
							<p class="mt-0.5 font-mono text-micro text-muted-foreground">
								{selected.id}
							</p>
						</div>
						<StatusBadge status={selected.status} />
					</div>
					<p class="mt-2 text-caption text-muted-foreground">
						{selected.summary}
					</p>
					<div class="mt-3">
						<div
							class="flex items-center justify-between text-micro text-muted-foreground"
						>
							<span>Iterations</span>
							<span class="tabular-nums"
								>{selected.iteration}/{selected.maxIterations}</span
							>
						</div>
						<Progress
							value={selected.iteration / Math.max(1, selected.maxIterations)}
							tone="running"
							class="mt-1"
						/>
					</div>
				</div>

				<div class="min-h-0 flex-1 space-y-3 overflow-y-auto px-3 py-3">
					<Card title="Run facts">
						<KeyValueList
							items={[
								{ key: 'model', value: selected.model },
								{ key: 'tokens', value: formatNumber(selected.tokens) },
								{
									key: 'checkpoints',
									value: formatNumber(selected.checkpoints),
								},
								{ key: 'errors', value: formatNumber(selected.errors) },
								{
									key: 'started',
									value: formatRelativeTime(selected.startedAt),
								},
								{
									key: 'updated',
									value: formatRelativeTime(selected.updatedAt),
								},
							]}
							dense
						/>
					</Card>

					<Card title="Tags">
						<div class="flex flex-wrap gap-1.5">
							{#each selected.tags as tag (tag)}
								<Badge variant="outline" class="text-[0.625rem]">{tag}</Badge>
							{:else}
								<span class="text-caption text-muted-foreground">No tags</span>
							{/each}
						</div>
					</Card>

					<Button
						variant="outline"
						size="sm"
						href="/agent-loops/{selected.id}"
						class="w-full"
					>
						<Icon name="arrow-right" size={13} />
						Open full detail
					</Button>
				</div>
			</div>
		{/if}
	{/snippet}
</SplitView>
