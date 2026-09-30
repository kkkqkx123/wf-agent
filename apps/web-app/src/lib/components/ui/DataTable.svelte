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

	// Narrow-screen card mapping: the first column titles each card and the
	// remaining columns become key-value rows unless a column opts out.
	// Action columns stay in the card footer so row operations remain usable.
	const titleColumns = $derived(
		columns.some((column) => column.card === 'title')
			? columns.filter((column) => column.card === 'title')
			: columns.slice(0, 1),
	);
	const actionColumns = $derived(
		columns.filter((column) => column.card === 'actions'),
	);
	const detailColumns = $derived(
		columns.filter(
			(column) =>
				!titleColumns.includes(column) && !actionColumns.includes(column),
		),
	);

	function selectRow(row: T): void {
		onrowclick?.(row);
	}

	function rowKeyDown(event: KeyboardEvent, row: T): void {
		if (!onrowclick) return;
		if (event.key !== 'Enter' && event.key !== ' ') return;
		event.preventDefault();
		onrowclick(row);
	}
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
	<div class="hidden sm:block">
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
							selectedKey === rowKey(row)
								? 'bg-accent/70'
								: 'hover:bg-accent/40',
						)}
						tabindex={onrowclick ? 0 : undefined}
						aria-selected={onrowclick && selectedKey !== null
							? selectedKey === rowKey(row)
							: undefined}
						onclick={() => selectRow(row)}
						onkeydown={(event) => rowKeyDown(event, row)}
					>
						{#each columns as column (column.key)}
							<td
								class={cn(
									dense ? 'px-3 py-1.5' : 'px-3 py-2.5',
									ALIGN[column.align ?? 'left'],
									column.cellClass,
								)}
							>
								{@render cellContent(column, row)}
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
	</div>
	<div class="space-y-2 sm:hidden">
		{#if active && start > 0}
			<div aria-hidden="true" style:height="{start * height}px"></div>
		{/if}
		{#each window as row (rowKey(row))}
			{@const isSelected = selectedKey === rowKey(row)}
			{#if onrowclick}
				<div
					role="button"
					tabindex={0}
					aria-pressed={selectedKey !== null ? isSelected : undefined}
					class={cn(
						'rounded-lg border border-border bg-card',
						dense ? 'px-3 py-1.5' : 'px-3 py-2.5',
						'cursor-pointer',
						isSelected ? 'border-ring/50 bg-accent/70' : 'hover:bg-accent/40',
					)}
					onclick={() => selectRow(row)}
					onkeydown={(event) => rowKeyDown(event, row)}
				>
					{@render cardBody(row)}
				</div>
			{:else}
				<div
					class={cn(
						'rounded-lg border border-border bg-card',
						dense ? 'px-3 py-1.5' : 'px-3 py-2.5',
						isSelected ? 'border-ring/50 bg-accent/70' : 'hover:bg-accent/40',
					)}
				>
					{@render cardBody(row)}
				</div>
			{/if}
		{/each}
		{#if active && end < rows.length}
			<div
				aria-hidden="true"
				style:height="{(rows.length - end) * height}px"
			></div>
		{/if}
	</div>
	{#if rows.length === 0}
		<EmptyState
			title={emptyTitle}
			description={emptyDescription}
			icon="file"
			class="py-8"
		/>
	{/if}
</div>

{#snippet cellContent(column: Column<T>, row: T)}
	{#if column.cell}
		{@render column.cell(row)}
	{:else if column.text}
		{column.text(row)}
	{/if}
{/snippet}

{#snippet cardBody(row: T)}
	<div class="text-body font-medium">
		{#each titleColumns as column (column.key)}
			<span class={cn(column.cellClass)}>
				{@render cellContent(column, row)}
			</span>
		{/each}
	</div>
	{#if detailColumns.length > 0}
		<dl class="mt-1.5 space-y-1">
			{#each detailColumns as column (column.key)}
				<div class="flex items-baseline justify-between gap-3">
					<dt class="shrink-0 text-micro text-muted-foreground">
						{column.header}
					</dt>
					<dd class={cn('min-w-0 text-right text-caption', column.cellClass)}>
						{@render cellContent(column, row)}
					</dd>
				</div>
			{/each}
		</dl>
	{/if}
	{#if actionColumns.length > 0}
		<div
			class="mt-2 flex flex-wrap items-center gap-2 border-t border-border pt-2"
		>
			{#each actionColumns as column (column.key)}
				{@render cellContent(column, row)}
			{/each}
		</div>
	{/if}
{/snippet}
