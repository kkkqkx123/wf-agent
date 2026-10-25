<script lang="ts">
	import { goto } from '$app/navigation';
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import DataTable from '#lib/ui/components/DataTable.svelte';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import Select from '#lib/ui/components/Select.svelte';
	import type { Column } from '#lib/ui/components/table';
	import {
		diffDefinitionVersions,
		rollbackDefinition,
		type DefinitionDiff,
	} from '#lib/services/definitions';
	import { buildVersionDiffView } from '#lib/graph/execution-projection';
	import type { DisplayEdge, DisplayNode } from '#lib/graph/display-model';
	import type { DefinitionKind, DefinitionVersion } from '#lib/types/models';
	import { toasts } from '#lib/stores/toast.svelte';
	import { formatDateTime } from '#lib/utils/format';
	import { t } from '#lib/i18n';

	interface Props {
		/** The kind of definition the versions belong to. */
		kind: DefinitionKind;
		/** The definition the versions are of. */
		definitionId: string;
		versions: DefinitionVersion[];
		nodes: DisplayNode[];
		edges: DisplayEdge[];
		onchanged: () => void;
	}

	let { kind, definitionId, versions, nodes, edges, onchanged }: Props = $props();

	let selectedId = $state<string | null>(null);
	let compareFrom = $state('');
	let compareTo = $state('');
	let diff = $state<DefinitionDiff | null>(null);
	let diffError = $state<string | null>(null);
	let diffLoading = $state(false);
	let rollbackTarget = $state('');
	let rollbackArmed = $state(false);
	let rollbackBusy = $state(false);
	let runTarget = $state('');
	let diffExplorer = $state<{ focus: (id: string) => void } | null>(null);

	// Only a workflow has a graph to diff; a definition without one is
	// compared by the fields that changed.
	const hasGraph = $derived(kind === 'workflow');

	const versionColumns = $derived<Column<DefinitionVersion>[]>([
		{
			key: 'version',
			header: t('domain.versions.version'),
			text: (row) => row.version,
		},
		{ key: 'note', header: t('domain.versions.note'), text: (row) => row.note },
		{
			key: 'author',
			header: t('domain.versions.author'),
			text: (row) => row.author,
		},
		{
			key: 'created',
			header: t('domain.versions.created'),
			text: (row) => formatDateTime(row.createdAt),
		},
		{
			key: 'current',
			header: t('domain.versions.state'),
			text: (row) => (row.current ? t('domain.versions.current') : t('domain.versions.superseded')),
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
	const diffIsEmpty = $derived(
		diff !== null && diffView.empty && (diff.changedFields?.length ?? 0) === 0,
	);

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
			diff = await diffDefinitionVersions(kind, definitionId, compareFrom, compareTo);
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
			await rollbackDefinition(kind, definitionId, rollbackTarget);
			rollbackArmed = false;
			toasts.success(t('domain.versions.rolledBack', { rollbackTarget }));
			onchanged();
		} catch (e) {
			toasts.error(t('domain.versions.rollbackFailed'), e instanceof Error ? e.message : undefined);
		} finally {
			rollbackBusy = false;
		}
	}

	// Version runs are created in the execution workbench: this panel
	// only deep-links there with the definition and version prefilled.
	async function goToVersionRun(): Promise<void> {
		if (!runTarget) return;
		const target =
			`/executions?definition=${encodeURIComponent(`workflow:${definitionId}`)}` +
			`&version=${encodeURIComponent(runTarget)}`;
		await goto(target);
	}
</script>

<Card title={t('domain.versions.historyTitle')} bodyClass="p-0">
	<DataTable columns={versionColumns} rows={versions} rowKey={(row) => row.version} />
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
					{t('domain.versions.noDifferences')}
				</p>
			{:else}
				<ul class="mt-2 space-y-1 text-caption">
					{#if hasGraph}
						<li>
							{t('domain.versions.addedNodes')}
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
							{t('domain.versions.removedNodes')}
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
							{t('domain.versions.addedEdges')}
							{#each diff.addedEdges as edge (`${edge.source}->${edge.target}`)}
								<button
									type="button"
									aria-current={selectedId === edge.source || selectedId === edge.target}
									class="mr-1 font-mono text-success underline-offset-2 hover:underline aria-[current=true]:rounded aria-[current=true]:bg-success/15 aria-[current=true]:ring-1 aria-[current=true]:ring-success"
									onclick={() => diffExplorer?.focus(edge.source)}
								>
									{edge.source} → {edge.target}
								</button>
							{:else}—{/each}
						</li>
						<li>
							{t('domain.versions.removedEdges')}
							{#each diff.removedEdges as edge (`${edge.source}->${edge.target}`)}
								<button
									type="button"
									aria-current={selectedId === edge.source || selectedId === edge.target}
									class="mr-1 font-mono text-destructive underline-offset-2 hover:underline aria-[current=true]:rounded aria-[current=true]:bg-success/15 aria-[current=true]:ring-1 aria-[current=true]:ring-destructive"
									onclick={() => diffExplorer?.focus(edge.source)}
								>
									{edge.source} → {edge.target}
								</button>
							{:else}—{/each}
						</li>
					{/if}
					{#if diff.changedFields.length > 0}
						<li>
							{t('domain.versions.changedFields', {
								count: diff.changedFields.length,
							})}
							{#each diff.changedFields as field (field)}
								<span class="mr-1 font-mono">{field}</span>
							{/each}
						</li>
					{/if}
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
				<Button variant="ghost" size="sm" onclick={() => (rollbackArmed = false)}>
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
		{#if kind === 'workflow'}
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
					disabled={!runTarget}
					title={t('domain.versions.runTooltip')}
					onclick={() => void goToVersionRun()}
				>
					<Icon name="play" size={13} />
					{t('domain.versions.run', { runTarget })}
				</Button>
			</div>
		{:else}
			<p class="text-caption text-muted-foreground">
				{t('domain.versions.runUnavailable')}
			</p>
		{/if}
	</Card>
</div>
{#if diff && !diffLoading && hasGraph}
	<div class="mt-3">
		{#if diffIsEmpty}
			<p class="mb-2 text-caption text-muted-foreground">
				{t('domain.versions.graphMatches')}
			</p>
		{/if}
		{#await import('#lib/components/domain/GraphExplorer.svelte') then module}
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
			/>
		{/await}
	</div>
{/if}
