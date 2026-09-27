<script lang="ts" generics="T">
	import type { Column } from './table';
	import EmptyState from './EmptyState.svelte';
	import { VIRTUALIZE_THRESHOLD } from '$lib/config/virtualization';
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
		virtualize?: boolean;
		rowHeight?: number;
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
		virtualize,
		rowHeight,
		onrowclick,
	}: Props = $props();

	const ALIGN = {
		left: 'text-left',
		right: 'text-right',
		center: 'text-center',
	} as const;

	const OVERSCAN = 8;

	const active = $derived(virtualize ?? rows.length > VIRTUALIZE_THRESHOLD);
	const height = $derived(rowHeight ?? (dense ? 36 : 44));

	let viewport: HTMLDivElement | null = $state(null);
	let scrollTop = $state(0);
	let viewHeight = $state(0);

	const start = $derived(
		active ? Math.max(0, Math.floor(scrollTop / height) - OVERSCAN) : 0,
	);
	const end = $derived(
		active
			? Math.min(
					rows.length,
					start + Math.ceil(viewHeight / height) + OVERSCAN * 2,
				)
			: rows.length,
	);
	const window = $derived(active ? rows.slice(start, end) : rows);
</script>

<div
	bind:this={viewport}
	bind:clientHeight={viewHeight}
	onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
	class={cn(
		'w-full overflow-x-auto',
		active && 'max-h-[32rem] overflow-y-auto',
		className,
	)}
>
	<table class="w-full border-collapse text-body">
		<thead class={cn(active && 'sticky top-0 z-10 bg-card')}>
			<tr class="border-b border-border">
				{#each columns as column (column.key)}
					<th
						scope="col"
						style:width={column.width}
						class={cn(
							'px-3 py-2 text-micro font-medium uppercase tracking-wide text-muted-foreground',
							ALIGN[column.align ?? 'left'],
							active && 'bg-card',
						)}
					>
						{column.header}
					</th>
				{/each}
			</tr>
		</thead>
		<tbody>
			{#if active && start > 0}
				<tr aria-hidden="true">
					<td
						colspan={columns.length}
						style:height="{start * height}px"
						class="border-0 p-0"
					></td>
				</tr>
			{/if}
			{#each window as row (rowKey(row))}
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
			{#if active && end < rows.length}
				<tr aria-hidden="true">
					<td
						colspan={columns.length}
						style:height="{(rows.length - end) * height}px"
						class="border-0 p-0"
					></td>
				</tr>
			{/if}
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
