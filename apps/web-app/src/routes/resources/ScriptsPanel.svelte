<script lang="ts">
	import Card from '@wf-agent/ui/components/Card.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import DataTable from '@wf-agent/ui/components/DataTable.svelte';
	import { t } from '#lib/i18n';
	import type { Column } from '@wf-agent/ui/components/table';
	import type { Script } from '#lib/types/models';
	import { formatNumber, formatRelativeTime } from '#lib/utils/format';

	interface Props {
		scripts: Script[];
	}

	let { scripts }: Props = $props();

	const scriptColumns: Column<Script>[] = [
		{
			key: 'name',
			header: t('page.resources.scripts.columnScript'),
			text: (script) => script.name,
			cellClass: 'font-mono text-caption',
		},
		{
			key: 'runtime',
			header: t('page.resources.scripts.columnRuntime'),
			text: (script) => script.runtime,
			cellClass: 'text-caption text-muted-foreground',
		},
		{
			key: 'state',
			header: t('page.resources.scripts.columnState'),
			cell: scriptStateCell,
		},
		{
			key: 'runs',
			header: t('page.resources.scripts.columnRuns'),
			align: 'right',
			text: (script) => formatNumber(script.runs),
			cellClass: 'tabular-nums text-caption',
		},
		{
			key: 'updated',
			header: t('page.resources.scripts.columnUpdated'),
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
		emptyTitle={t('page.resources.scripts.noScripts')}
		emptyDescription={t('page.resources.scripts.noScriptsDesc')}
		virtualize={false}
	/>
</Card>

{#snippet scriptStateCell(script: Script)}
	<StatusBadge status={script.enabled ? 'enabled' : 'disabled'} size="sm" />
{/snippet}
