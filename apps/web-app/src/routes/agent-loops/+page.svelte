<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import SplitView from '$lib/components/layout/SplitView.svelte';
	import PageState from '$lib/components/layout/PageState.svelte';
	import StatusBadge from '$lib/components/ui/StatusBadge.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import type { Column } from '$lib/components/ui/table';
	import KeyValueList from '$lib/components/domain/KeyValueList.svelte';
	import FilterBar from '$lib/components/ui/FilterBar.svelte';
	import Progress from '$lib/components/ui/Progress.svelte';
	import {
		listAgentLoops,
		getAgentLoopDetail,
	} from '$lib/services/agent-loops';
	import type { AgentLoop, AgentLoopDetail } from '$lib/types/models';
	import { formatNumber, formatRelativeTime } from '$lib/utils/format';

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
	let allLoops = $state<AgentLoop[]>([]);
	let selected = $state<AgentLoopDetail | null>(null);
	let listError = $state<string | null>(null);
	let listLoading = $state(false);
	let detailError = $state<string | null>(null);

	onMount(() => {
		void reload();
	});

	async function reload(): Promise<void> {
		listError = null;
		listLoading = true;
		try {
			const page = await listAgentLoops();
			allLoops = page.items;
			if (page.items.length > 0 && !selectedId) {
				selectedId = page.items[0].id;
			}
		} catch (e) {
			listError = e instanceof Error ? e.message : 'Agent loops failed.';
			allLoops = [];
		} finally {
			listLoading = false;
		}
	}

	$effect(() => {
		const id = selectedId;
		if (!id) {
			selected = null;
			return;
		}
		detailError = null;
		void getAgentLoopDetail(id)
			.then((row) => {
				selected = row;
			})
			.catch((e) => {
				selected = null;
				detailError = e instanceof Error ? e.message : 'Detail failed.';
			});
	});

	const filtered = $derived(
		allLoops.filter((loop) => {
			const matchesStatus = !status || loop.status === status;
			const needle = query.trim().toLowerCase();
			const matchesQuery =
				!needle ||
				loop.name.toLowerCase().includes(needle) ||
				loop.tags.some((tag) => tag.toLowerCase().includes(needle));
			return matchesStatus && matchesQuery;
		}),
	);

	const loopColumns: Column<AgentLoop>[] = [
		{ key: 'name', header: 'Loop', cell: loopNameCell },
		{ key: 'status', header: 'Status', cell: loopStatusCell },
		{
			key: 'iteration',
			header: 'Iteration',
			text: (loop) => `${loop.iteration}/${loop.maxIterations}`,
			cellClass: 'tabular-nums text-caption text-muted-foreground',
		},
		{
			key: 'model',
			header: 'Model',
			text: (loop) => loop.model,
			cellClass: 'font-mono text-caption',
		},
		{
			key: 'tokens',
			header: 'Tokens',
			align: 'right',
			text: (loop) => formatNumber(loop.tokens),
			cellClass: 'tabular-nums text-caption text-muted-foreground',
		},
		{
			key: 'updated',
			header: 'Updated',
			align: 'right',
			text: (loop) => formatRelativeTime(loop.updatedAt),
			cellClass: 'text-caption text-muted-foreground',
		},
	];

	const filteredCopy = $derived(
		query.trim() || status
			? {
					title: 'No loops match',
					description: 'Clear the filters to browse every loop.',
				}
			: {
					title: 'No agent loops yet',
					description: 'Start a run from the chat page to see loops here.',
				},
	);
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
					onclick={() => void reload()}
				/>
				<Button size="sm" onclick={() => void goto(resolve('/chat'))}>
					<Icon name="play" size={13} />
					New run
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

			<PageState
				loading={listLoading}
				error={listError}
				errorTitle="Agent loops failed to load"
				empty={filtered.length === 0}
				emptyIcon="loop"
				emptyTitle={filteredCopy.title}
				emptyDescription={filteredCopy.description}
				onretry={() => void reload()}
				class="rounded-lg border border-border bg-card"
			>
				<Card bodyClass="p-0">
					<DataTable
						columns={loopColumns}
						rows={filtered}
						rowKey={(loop) => loop.id}
						selectedKey={selectedId}
						onrowclick={(loop) => (selectedId = loop.id)}
						virtualize={false}
					/>
				</Card>
			</PageState>
		</div>
	</div>

	{#snippet inspector()}
		{#if detailError && !selected}
			<ErrorState
				title="Loop detail failed to load"
				description={detailError}
				onretry={() => {
					if (selectedId) {
						detailError = null;
						void getAgentLoopDetail(selectedId)
							.then((row) => {
								selected = row;
							})
							.catch((e) => {
								detailError = e instanceof Error ? e.message : 'Detail failed.';
							});
					}
				}}
				class="m-3 rounded-lg border border-border bg-card"
			/>
		{:else if selected}
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

{#snippet loopNameCell(loop: AgentLoop)}
	<span class="flex items-center gap-1.5">
		{#if loop.starred}
			<Icon name="star" size={12} class="shrink-0 text-warning" />
		{/if}
		<span class="truncate">{loop.name}</span>
	</span>
{/snippet}

{#snippet loopStatusCell(loop: AgentLoop)}
	<StatusBadge status={loop.status} size="sm" />
{/snippet}
