<script lang="ts">
	import { getAgentLoopVariables } from '$lib/services/agent-loops';
	import type { LoopVariable } from '$lib/types/models';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import DataTable from '@wf-agent/ui/components/DataTable.svelte';
	import type { Column } from '@wf-agent/ui/components/table';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import { formatDateTime } from '$lib/utils/format';

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
			error = e instanceof Error ? e.message : 'Variables failed.';
		}
	}

	const variableColumns: Column<LoopVariable>[] = [
		{ key: 'key', header: 'Key', text: (row) => row.key },
		{ key: 'type', header: 'Type', text: (row) => row.type ?? '' },
		{ key: 'value', header: 'Value', text: (row) => row.value },
		{ key: 'scope', header: 'Scope', text: (row) => row.scope ?? '' },
		{
			key: 'updated',
			header: 'Updated',
			text: (row) => formatDateTime(row.updatedAt ?? null),
		},
	];
</script>

{#if error}
	<ErrorState
		title="Variables failed to load"
		description={error}
		onretry={() => void load(id)}
		class="rounded-lg border border-border bg-card"
	/>
{:else}
	<Card title="Variables" bodyClass="p-0">
		<DataTable
			columns={variableColumns}
			rows={variables}
			rowKey={(row) => row.key}
		/>
	</Card>
{/if}
