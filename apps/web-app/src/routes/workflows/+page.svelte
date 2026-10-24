<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import Dialog from '@wf-agent/ui/components/Dialog.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import Textarea from '@wf-agent/ui/components/Textarea.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import PageHeader from '#lib/components/layout/PageHeader.svelte';
	import SplitView from '#lib/components/layout/SplitView.svelte';
	import PageState from '#lib/components/layout/PageState.svelte';
	import WorkflowCard from '#lib/components/domain/WorkflowCard.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import KeyValueList from '#lib/components/domain/KeyValueList.svelte';
	import FilterBar from '@wf-agent/ui/components/FilterBar.svelte';
	import {
		createMinimalWorkflow,
		importWorkflow,
		listWorkflows,
		getWorkflowDetail,
	} from '#lib/services/workflows';
	import type { Workflow, WorkflowDetail } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import { t } from '#lib/i18n';
	import {
		formatNumber,
		formatPercent,
		formatRelativeTime,
	} from '#lib/utils/format';

	const STATUS_OPTIONS = [
		{ value: 'active', label: 'Active' },
		{ value: 'draft', label: 'Draft' },
		{ value: 'archived', label: 'Archived' },
	];

	let query = $state('');
	let status = $state('');
	let selectedId = $state<string | null>(null);
	let allWorkflows = $state<Workflow[]>([]);
	let selected = $state<WorkflowDetail | null>(null);
	let listError = $state<string | null>(null);
	let listLoading = $state(false);
	let detailError = $state<string | null>(null);

	let importOpen = $state(false);
	let importText = $state('');
	let importError = $state<string | null>(null);
	let importBusy = $state(false);
	let newOpen = $state(false);
	let newName = $state('');
	let newBusy = $state(false);

	onMount(() => {
		void reload();
	});

	async function reload(): Promise<void> {
		listError = null;
		listLoading = true;
		try {
			const page = await listWorkflows({ limit: 200 });
			allWorkflows = page.items;
			if (page.items.length > 0 && !selectedId) {
				selectedId = page.items[0].id;
			}
		} catch (e) {
			listError =
				e instanceof Error ? e.message : t('page.workflows.loadFailed');
			allWorkflows = [];
		} finally {
			listLoading = false;
		}
	}

	async function runImport(): Promise<void> {
		importError = null;
		try {
			JSON.parse(importText);
		} catch (e) {
			importError = e instanceof Error ? e.message : 'Invalid JSON';
			return;
		}
		importBusy = true;
		try {
			const id = await importWorkflow(importText);
			toasts.success('Workflow imported');
			importOpen = false;
			importText = '';
			await reload();
			await goto(resolve('/workflows/[id]', { id }));
		} catch (e) {
			importError =
				e instanceof Error ? e.message : t('page.workflows.importFailed');
		} finally {
			importBusy = false;
		}
	}

	async function runCreate(): Promise<void> {
		newBusy = true;
		try {
			const workflow = await createMinimalWorkflow(newName);
			toasts.success(t('page.workflows.created'));
			newOpen = false;
			newName = '';
			await goto(resolve('/workflows/[id]', { id: workflow.id }));
		} catch (e) {
			toasts.error(
				t('page.workflows.createFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			newBusy = false;
		}
	}

	$effect(() => {
		const id = selectedId;
		if (!id) {
			selected = null;
			return;
		}
		detailError = null;
		void getWorkflowDetail(id)
			.then((row) => {
				selected = row;
			})
			.catch((e) => {
				selected = null;
				detailError =
					e instanceof Error ? e.message : t('page.workflows.detailLoadFailed');
			});
	});

	const filtered = $derived(
		allWorkflows.filter((workflow) => {
			const matchesStatus = !status || workflow.status === status;
			const needle = query.trim().toLowerCase();
			const matchesQuery =
				!needle ||
				workflow.name.toLowerCase().includes(needle) ||
				workflow.tags.some((tag) => tag.toLowerCase().includes(needle));
			return matchesStatus && matchesQuery;
		}),
	);

	const filteredCopy = $derived(
		query.trim() || status
			? {
					title: t('page.workflows.noMatch'),
					description: t('page.workflows.noMatchDesc'),
				}
			: {
					title: t('page.workflows.noWorkflows'),
					description: t('page.workflows.noWorkflowsDesc'),
				},
	);
</script>

<SplitView
	inspectorTitle={t('page.workflowDetail.title')}
	inspectorOpen={selectedId !== null}
	class="h-full"
>
	<div class="flex h-full min-h-0 flex-col">
		<PageHeader
			title={t('page.workflows.title')}
			description={t('page.workflows.description')}
		>
			{#snippet actions()}
				<IconButton
					icon="refresh"
					label={t('common.refresh')}
					onclick={() => void reload()}
				/>
				<Button
					variant="outline"
					size="sm"
					onclick={() => {
						importText = '';
						importError = null;
						importOpen = true;
					}}
				>
					<Icon name="upload" size={13} />
					{t('page.workflows.import')}
				</Button>
				<Button
					size="sm"
					onclick={() => {
						newName = '';
						newOpen = true;
					}}
				>
					<Icon name="plus" size={13} />
					{t('page.workflows.new')}
				</Button>
			{/snippet}
		</PageHeader>

		<div class="page-container min-h-0 flex-1 overflow-y-auto px-4 pb-4">
			<FilterBar
				bind:query
				bind:status
				statusOptions={STATUS_OPTIONS}
				placeholder={t('page.workflows.filterPlaceholder')}
				class="mb-3"
			>
				{#snippet trailing()}
					<span class="text-caption text-muted-foreground"
						>{t('page.workflows.shown', { count: filtered.length })}</span
					>
				{/snippet}
			</FilterBar>

			<PageState
				loading={listLoading}
				error={listError}
				errorTitle={t('page.workflows.loadFailed')}
				empty={filtered.length === 0}
				emptyIcon="workflow"
				emptyTone="brand"
				emptyTitle={filteredCopy.title}
				emptyDescription={filteredCopy.description}
				onretry={() => void reload()}
				class="rounded-lg border border-border bg-card"
			>
				<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
					{#each filtered as workflow (workflow.id)}
						<WorkflowCard
							{workflow}
							selected={selectedId === workflow.id}
							onselect={(item) => (selectedId = item.id)}
							onopen={(item) =>
								void goto(resolve('/workflows/[id]', { id: item.id }))}
						/>
					{/each}
				</div>
			</PageState>
		</div>
	</div>

	{#snippet inspector()}
		{#if detailError && !selected}
			<ErrorState
				title={t('page.workflows.detailLoadFailed')}
				description={detailError}
				onretry={() => {
					if (selectedId) {
						detailError = null;
						void getWorkflowDetail(selectedId)
							.then((row) => {
								selected = row;
							})
							.catch((e) => {
								detailError =
									e instanceof Error
										? e.message
										: t('page.workflows.detailLoadFailed');
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
						{selected.description}
					</p>
					<div class="mt-2 flex flex-wrap items-center gap-1.5">
						{#each selected.tags as tag (tag)}
							<Badge variant="outline" class="text-[0.625rem]">{tag}</Badge>
						{/each}
					</div>
				</div>

				<div class="min-h-0 flex-1 space-y-3 overflow-y-auto px-3 py-3">
					<Card title={t('page.workflows.summary')}>
						<KeyValueList
							items={[
								{ key: 'version', value: `v${selected.version}` },
								{ key: 'category', value: selected.category },
								{ key: 'author', value: selected.author },
								{ key: 'nodes', value: formatNumber(selected.nodeCount) },
								{ key: 'edges', value: formatNumber(selected.edgeCount) },
								{ key: 'runs', value: formatNumber(selected.runs) },
								{
									key: 'success',
									value:
										selected.successRate === null
											? '—'
											: formatPercent(selected.successRate),
								},
								{
									key: 'updated',
									value: formatRelativeTime(selected.updatedAt),
								},
							]}
							dense
						/>
					</Card>

					<p class="text-caption text-muted-foreground">
						Open full detail to inspect the graph, neighborhood and editing
						tools.
					</p>

					<Button
						variant="outline"
						size="sm"
						href="/workflows/{selected.id}"
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

<Dialog
	bind:open={importOpen}
	title={t('page.workflows.importWorkflow')}
	description={t('page.workflows.importWorkflowDesc')}
>
	<Textarea
		bind:value={importText}
		placeholder={'{\n  "id": "my-workflow",\n  …\n}'}
		class="min-h-48 font-mono text-small"
	/>
	{#if importError}
		<p
			class="mt-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5 text-caption text-destructive"
		>
			{importError}
		</p>
	{/if}
	{#snippet footer()}
		<div class="flex items-center justify-end gap-2">
			<Button variant="ghost" size="sm" onclick={() => (importOpen = false)}>
				{t('common.cancel')}
			</Button>
			<Button size="sm" disabled={importBusy} onclick={() => void runImport()}>
				{importBusy
					? t('page.workflows.importing')
					: t('page.workflows.import')}
			</Button>
		</div>
	{/snippet}
</Dialog>

<Dialog
	bind:open={newOpen}
	title={t('page.workflows.newWorkflow')}
	description={t('page.workflows.newWorkflowDesc')}
>
	<Input bind:value={newName} placeholder={t('page.workflows.workflowName')} />
	{#snippet footer()}
		<div class="flex items-center justify-end gap-2">
			<Button variant="ghost" size="sm" onclick={() => (newOpen = false)}>
				{t('common.cancel')}
			</Button>
			<Button size="sm" disabled={newBusy} onclick={() => void runCreate()}>
				{newBusy ? t('page.workflows.creating') : t('common.create')}
			</Button>
		</div>
	{/snippet}
</Dialog>
