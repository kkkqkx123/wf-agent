<script lang="ts">
	import Card from '$lib/components/ui/Card.svelte';
	import StatusBadge from '$lib/components/ui/StatusBadge.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import type { Column } from '$lib/components/ui/table';
	import type { Script } from '$lib/types/models';
	import { formatNumber, formatRelativeTime } from '$lib/utils/format';

	interface Props {
		scripts: Script[];
	}

	let { scripts }: Props = $props();

	const scriptColumns: Column<Script>[] = [
		{
			key: 'name',
			header: 'Script',
			text: (script) => script.name,
			cellClass: 'font-mono text-caption',
		},
		{
			key: 'runtime',
			header: 'Runtime',
			text: (script) => script.runtime,
			cellClass: 'text-caption text-muted-foreground',
		},
		{ key: 'state', header: 'State', cell: scriptStateCell },
		{
			key: 'runs',
			header: 'Runs',
			align: 'right',
			text: (script) => formatNumber(script.runs),
			cellClass: 'tabular-nums text-caption',
		},
		{
			key: 'updated',
			header: 'Updated',
			align: 'right',
			text: (script) => formatRelativeTime(script.updatedAt),
			cellClass: 'text-caption text-muted-foreground',
		},
	];
</script>

<Card bodyClass="p-0">
	<DataTable
		columns={scriptColumns}
		rows={scripts}
		rowKey={(script) => script.id}
		emptyTitle="No scripts registered"
		emptyDescription="Scripts appear here once the registry has entries."
		virtualize={false}
	/>
</Card>

{#snippet scriptStateCell(script: Script)}
	<StatusBadge status={script.enabled ? 'enabled' : 'disabled'} size="sm" />
{/snippet}
