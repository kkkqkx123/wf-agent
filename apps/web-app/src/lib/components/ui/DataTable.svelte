<script lang="ts" generics="T">
	import type { Column } from './table';
	import EmptyState from './EmptyState.svelte';
	import { cn } from '$lib/utils/cn';

	interface Props {
		columns: Column<T>[];
		rows: T[];
		rowKey: (row: T) => string;
		selectedKey?: string | null;
		dense?: boolean;
		emptyTitle?: string;
		emptyDescription?: string;
		class?: string;
		onrowclick?: (row: T) => void;
	}

	let {
		columns,
		rows,
		rowKey,
		selectedKey = null,
		dense = false,
		emptyTitle = 'Nothing to show',
		emptyDescription,
		class: className = '',
		onrowclick,
	}: Props = $props();

	const ALIGN = {
		left: 'text-left',
		right: 'text-right',
		center: 'text-center',
	} as const;
</script>

<div class={cn('w-full overflow-x-auto', className)}>
	<table class="w-full border-collapse text-body">
		<thead>
			<tr class="border-b border-border">
				{#each columns as column (column.key)}
					<th
						scope="col"
						style:width={column.width}
						class={cn(
							'px-3 py-2 text-micro font-medium uppercase tracking-wide text-muted-foreground',
							ALIGN[column.align ?? 'left'],
						)}
					>
						{column.header}
					</th>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#each rows as row (rowKey(row))}
				<tr
					class={cn(
						'border-b border-border/60 transition-colors last:border-0',
						onrowclick && 'cursor-pointer',
						selectedKey === rowKey(row) ? 'bg-accent/70' : 'hover:bg-accent/40',
					)}
					onclick={() => onrowclick?.(row)}
				>
					{#each columns as column (column.key)}
						<td
							class={cn(
								dense ? 'px-3 py-1.5' : 'px-3 py-2.5',
								ALIGN[column.align ?? 'left'],
							)}
						>
							{#if column.cell}
								{@render column.cell(row)}
							{:else if column.text}
								{column.text(row)}
							{/if}
						</td>
					{/each}
				</tr>
			{/each}
		</tbody>
	</table>
	{#if rows.length === 0}
		<EmptyState
			title={emptyTitle}
			description={emptyDescription}
			icon="file"
			class="py-8"
		/>
	{/if}
</div>
