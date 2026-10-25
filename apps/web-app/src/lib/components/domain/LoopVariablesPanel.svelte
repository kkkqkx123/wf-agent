<script lang="ts">
	import { getAgentLoopVariables } from '#lib/services/agent-loops';
	import type { LoopVariable } from '#lib/types/models';
	import Card from '#lib/ui/components/Card.svelte';
	import DataTable from '#lib/ui/components/DataTable.svelte';
	import type { Column } from '#lib/ui/components/table';
	import ErrorState from '#lib/ui/components/ErrorState.svelte';
	import { formatDateTime } from '#lib/utils/format';
	import { t } from '#lib/i18n';

	/** Loop variable snapshot table; owns its own loading keyed by id. */
	let { id }: { id: string } = $props();

	let variables = $state<LoopVariable[]>([]);
	let error = $state<string | null>(null);
	let seen = $state('');

	$effect(() => {
		const loopId = id;
		if (seen === loopId) return;
		seen = loopId;
		void load(loopId);
	});

	async function load(loopId: string): Promise<void> {
		error = null;
		try {
			variables = await getAgentLoopVariables(loopId);
		} catch (e) {
			seen = '';
			variables = [];
			error = e instanceof Error ? e.message : t('domain.loopVariables.failed');
		}
	}

	const variableColumns = $derived<Column<LoopVariable>[]>([
		{
			key: 'key',
			header: t('domain.loopVariables.key'),
			text: (row) => row.key,
		},
		{
			key: 'type',
			header: t('domain.loopVariables.type'),
			text: (row) => row.type ?? '',
		},
		{
			key: 'value',
			header: t('domain.loopVariables.value'),
			text: (row) => row.value,
		},
		{
			key: 'scope',
			header: t('domain.loopVariables.scope'),
			text: (row) => row.scope ?? '',
		},
		{
			key: 'updated',
			header: t('domain.loopVariables.updated'),
			text: (row) => formatDateTime(row.updatedAt ?? null),
		},
	]);
</script>

{#if error}
	<ErrorState
		title={t('domain.loopVariables.failedTitle')}
		description={error}
		onretry={() => void load(id)}
		class="rounded-lg border border-border bg-card"
	/>
{:else}
	<Card title={t('domain.history.variables')} bodyClass="p-0">
		<DataTable columns={variableColumns} rows={variables} rowKey={(row) => row.key} />
	</Card>
{/if}
