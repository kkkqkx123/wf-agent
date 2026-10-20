<script lang="ts">
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import DataTable from '@wf-agent/ui/components/DataTable.svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Select from '@wf-agent/ui/components/Select.svelte';
		import type { Column } from '@wf-agent/ui/components/table';
	import {
		diffWorkflowVersions,
		executeWorkflow,
		type VersionDiff,
	} from '$lib/services/workflows';
	import { rollbackWorkflow } from '$lib/services/graph';
	import { buildVersionDiffView } from '$lib/graph/execution-projection';
	import type { DisplayEdge, DisplayNode } from '$lib/graph/display-model';
	import type { WorkflowVersion } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatDateTime } from '$lib/utils/format';
	import { t } from '$lib/i18n';

	interface Props {
		workflowId: string;
		versions: WorkflowVersion[];
		nodes: DisplayNode[];
		edges: DisplayEdge[];
		onchanged: () => void;
	}

	let { workflowId, versions, nodes, edges, onchanged }: Props = $props();

	let selectedId = $state<string | null>(null);
	let compareFrom = $state('');
	let compareTo = $state('');
	let diff = $state<VersionDiff | null>(null);
	let diffError = $state<string | null>(null);
	let diffLoading = $state(false);
	let rollbackTarget = $state('');
	let rollbackArmed = $state(false);
	let rollbackBusy = $state(false);
	let runTarget = $state('');
	let runBusy = $state(false);
	let diffExplorer = $state<{ focus: (id: string) => void } | null>(null);

	const versionColumns = $derived<Column<WorkflowVersion>[]>([
		{ key: 'version', header: t('domain.versions.version'), text: (row) => row.version },
		{ key: 'note', header: t('domain.versions.note'), text: (row) => row.note },
		{ key: 'author', header: t('domain.versions.author'), text: (row) => row.author },
		{
			key: 'created',
			header: t('domain.versions.created'),
			text: (row) => formatDateTime(row.createdAt),
		},
		{
			key: 'current',
			header: t('domain.versions.state'),
			text: (row) =>
				row.current ? t('domain.versions.current') : t('domain.versions.superseded'),
		},
	]);

	const versionOptions = $derived(
		versions.map((row) => ({ value: row.version, label: row.version })),
	);

	// Edge-level diff view: single derivation feeds both the text rows and
	// the graph, so counts and colors always agree.
	const diffView = $derived(buildVersionDiffView(nodes, edges, diff));
	const diffGraphNodes = $derived<DisplayNode[]>(diffView.nodes);
	const diffGraphEdges = $derived<DisplayEdge[]>(diffView.edges);
	const diffIsEmpty = $derived(diff !== null && diffView.empty);

	$effect(() => {
		if (versions.length > 0) {
			if (!compareFrom) compareFrom = versions[versions.length - 1].version;
			if (!compareTo) compareTo = versions[0].version;
			if (!rollbackTarget) rollbackTarget = versions[0].version;
			if (!runTarget) runTarget = versions[0].version;
		}
	});

	async function runCompare(): Promise<void> {
		if (!compareFrom || !compareTo) return;
		diffLoading = true;
		diffError = null;
		try {
			diff = await diffWorkflowVersions(workflowId, compareFrom, compareTo);
		} catch (e) {
			diffError = e instanceof Error ? e.message : t('domain.versions.compareFailed');
			diff = null;
		} finally {
			diffLoading = false;
		}
	}

	async function runRollback(): Promise<void> {
		if (!rollbackTarget) return;
		rollbackBusy = true;
		try {
			await rollbackWorkflow(workflowId, rollbackTarget);
			rollbackArmed = false;
			toasts.success(t('domain.versions.rolledBack', { version: rollbackTarget }));
			onchanged();
		} catch (e) {
			toasts.error(
				t('domain.versions.rollbackFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			rollbackBusy = false;
		}
	}

	async function runVersion(): Promise<void> {
		if (!runTarget || runBusy) return;
		runBusy = true;
		try {
			const executionId = await executeWorkflow(
				workflowId,
				null,
				undefined,
				runTarget,
			);
			toasts.success(
				t('domain.versions.versionStarted', { version: runTarget }),
				executionId,
			);
			await goto(resolve('/executions/[id]', { id: executionId }));
		} catch (e) {
			toasts.error(
				t('domain.versions.startFailed'),
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			runBusy = false;
		}
	}
</script>

<Card title={t('domain.versions.historyTitle')} bodyClass="p-0">
	<DataTable
		columns={versionColumns}
		rows={versions}
		rowKey={(row) => row.version}
	/>
</Card>
<div class="mt-3 grid gap-3 lg:grid-cols-2">
	<Card title={t('domain.versions.compareTitle')}>
		<div class="flex flex-wrap items-end gap-2">
			<Select
				bind:value={compareFrom}
				options={versionOptions}
				size="sm"
				placeholder={t('domain.versions.from')}
				class="w-32"
			/>
			<Select
				bind:value={compareTo}
				options={versionOptions}
				size="sm"
				placeholder={t('domain.versions.to')}
				class="w-32"
			/>
			<Button
				variant="outline"
				size="sm"
				disabled={!compareFrom || !compareTo || diffLoading}
				onclick={() => void runCompare()}
			>
				<Icon name="copy" size={13} />
				{t('domain.versions.compare')}
			</Button>
		</div>
		{#if diffError}
			<p class="mt-2 text-caption text-destructive">{diffError}</p>
		{:else if diff}
			{#if diffIsEmpty}
				<p class="mt-2 text-caption text-muted-foreground">
					{t('domain.versions.noDifferences', {
						from: compareFrom,
						to: compareTo,
					})}
				</p>
			{:else}
				<ul class="mt-2 space-y-1 text-caption">
					<li>
						{t('domain.versions.addedNodes', { count: diffView.addedNodes })}
						{#each diff.addedNodes as nodeId (nodeId)}
							<button
								type="button"
								aria-current={selectedId === nodeId}
								class="mr-1 font-mono text-success underline-offset-2 hover:underline aria-[current=true]:rounded aria-[current=true]:bg-success/15 aria-[current=true]:ring-1 aria-[current=true]:ring-success"
								onclick={() => diffExplorer?.focus(nodeId)}
							>
								{nodeId}
							</button>
						{:else}—{/each}
					</li>
					<li>
						{t('domain.versions.removedNodes', { count: diffView.removedNodes })}
						{#each diff.removedNodes as nodeId (nodeId)}
							<button
								type="button"
								aria-current={selectedId === nodeId}
								class="mr-1 font-mono text-destructive underline-offset-2 hover:underline aria-[current=true]:rounded aria-[current=true]:bg-success/15 aria-[current=true]:ring-1 aria-[current=true]:ring-destructive"
								onclick={() => diffExplorer?.focus(nodeId)}
							>
								{nodeId}
							</button>
						{:else}—{/each}
					</li>
					<li>
						{t('domain.versions.addedEdges', { count: diffView.addedEdges })}
						{#each diff.addedEdges as edge (`${edge.source}->${edge.target}`)}
							<button
								type="button"
								aria-current={selectedId === edge.source ||
									selectedId === edge.target}
								class="mr-1 font-mono text-success underline-offset-2 hover:underline aria-[current=true]:rounded aria-[current=true]:bg-success/15 aria-[current=true]:ring-1 aria-[current=true]:ring-success"
								onclick={() => diffExplorer?.focus(edge.source)}
							>
								{edge.source} → {edge.target}
							</button>
						{:else}—{/each}
					</li>
					<li>
						{t('domain.versions.removedEdges', { count: diffView.removedEdges })}
						{#each diff.removedEdges as edge (`${edge.source}->${edge.target}`)}
							<button
								type="button"
								aria-current={selectedId === edge.source ||
									selectedId === edge.target}
								class="mr-1 font-mono text-destructive underline-offset-2 hover:underline aria-[current=true]:rounded aria-[current=true]:bg-success/15 aria-[current=true]:ring-1 aria-[current=true]:ring-destructive"
								onclick={() => diffExplorer?.focus(edge.source)}
							>
								{edge.source} → {edge.target}
							</button>
						{:else}—{/each}
					</li>
				</ul>
			{/if}
		{/if}
	</Card>
	<Card title={t('domain.versions.rollbackTitle')}>
		<div class="flex flex-wrap items-end gap-2">
			<Select
				bind:value={rollbackTarget}
				options={versionOptions}
				size="sm"
				placeholder={t('domain.versions.version')}
				class="w-32"
			/>
			{#if rollbackArmed}
				<Button
					size="sm"
					disabled={rollbackBusy || !rollbackTarget}
					onclick={() => void runRollback()}
				>
					{t('domain.versions.confirmRollback', { version: rollbackTarget })}
				</Button>
				<Button
					variant="ghost"
					size="sm"
					onclick={() => (rollbackArmed = false)}
				>
					{t('common.cancel')}
				</Button>
			{:else}
				<Button
					variant="outline"
					size="sm"
					disabled={!rollbackTarget}
					onclick={() => (rollbackArmed = true)}
				>
					<Icon name="history" size={13} />
					{t('domain.versions.rollback')}
				</Button>
			{/if}
		</div>
	</Card>
	<Card title={t('domain.versions.runTitle')}>
		<div class="flex flex-wrap items-end gap-2">
			<Select
				bind:value={runTarget}
				options={versionOptions}
				size="sm"
				placeholder={t('domain.versions.version')}
				class="w-32"
			/>
			<Button
				variant="outline"
				size="sm"
				disabled={!runTarget || runBusy}
				title={t('domain.versions.runTooltip')}
				onclick={() => void runVersion()}
			>
				<Icon name="refresh" size={13} />
				{runBusy ? t('domain.versions.starting') : t('domain.versions.run', { version: runTarget })}
			</Button>
		</div>
	</Card>
</div>
{#if diff && !diffLoading}
	<div class="mt-3">
		{#if diffIsEmpty}
			<p class="mb-2 text-caption text-muted-foreground">
				{t('domain.versions.graphMatches')}
			</p>
		{/if}
		{#await import('$lib/components/domain/GraphExplorer.svelte') then module}
<module.default
			bind:this={diffExplorer}
			nodes={diffGraphNodes}
			edges={diffGraphEdges}
			preset="workflow"
			{selectedId}
			onselect={(id) => (selectedId = id)}
			overlays={[
				{
					id: 'added',
					label: t('domain.versions.added'),
					ids: diff.addedNodes,
				},
				{
					id: 'removed',
					label: t('domain.versions.removed'),
					ids: diff.removedNodes,
				},
			]}
		/> {/await}
	</div>
{/if}
