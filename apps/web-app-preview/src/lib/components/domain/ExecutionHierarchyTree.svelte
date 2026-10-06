<script lang="ts">
	import type {
		ExecutionSubtree,
		ExecutionSubtreeNode,
	} from '$lib/types/models';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import { SvelteSet } from 'svelte/reactivity';
	import { cn } from '@wf-agent/ui/cn';
	import { resolve } from '$app/paths';
	import { formatNumber } from '$lib/utils/format';

	interface Props {
		subtree: ExecutionSubtree;
		/** Execution currently being inspected, highlighted in the tree. */
		currentId?: string | null;
		class?: string;
	}

	let { subtree, currentId = null, class: className = '' }: Props = $props();

	// Rows arrive breadth-first from the backend, so a node's whole subtree is
	// the contiguous run that follows it.
	const rows = $derived(
		subtree.nodes.map((node, index) => ({
			node,
			hasChildren: subtree.nodes.some(
				(candidate) => candidate.parentExecutionId === node.executionId,
			),
			lastAtDepth: isLastAtDepth(subtree.nodes, index),
		})),
	);

	let collapsed = new SvelteSet<string>();

	const visible = $derived(
		rows.filter(
			(row) =>
				!ancestorCollapsed(row.node) && !collapsed.has(row.node.executionId),
		),
	);

	/** A node is hidden when any displayed ancestor is collapsed. */
	function ancestorCollapsed(node: ExecutionSubtreeNode): boolean {
		let cursor = node.parentExecutionId;
		while (cursor) {
			if (collapsed.has(cursor)) return true;
			cursor =
				subtree.nodes.find((candidate) => candidate.executionId === cursor)
					?.parentExecutionId ?? null;
		}
		return false;
	}

	/** Whether a node is the last sibling at its own depth. */
	function isLastAtDepth(
		nodes: ExecutionSubtreeNode[],
		index: number,
	): boolean {
		const depth = nodes[index].depth;
		return index === nodes.length - 1 || nodes[index + 1].depth <= depth;
	}

	function toggle(executionId: string): void {
		if (collapsed.has(executionId)) collapsed.delete(executionId);
		else collapsed.add(executionId);
	}

	function branchGlyphs(row: (typeof rows)[number]): string {
		let prefix = '';
		for (let level = 1; level < row.node.depth; level += 1) {
			prefix += row.lastAtDepth && level === row.node.depth - 1 ? '  ' : '│ ';
		}
		return `${prefix}${row.node.depth === 0 ? '' : row.lastAtDepth ? '└ ' : '├ '}`;
	}
</script>

<div class={cn('space-y-2', className)}>
	{#if subtree.nodes.length === 0}
		<EmptyState
			icon="git-commit"
			title="No nested executions"
			description="Child runs appear here once this execution spawns one."
			class="rounded-lg border border-border bg-card"
		/>
	{:else}
		<div class="flex flex-wrap items-center gap-2">
			<p class="text-micro text-muted-foreground">
				{formatNumber(subtree.nodes.length)} execution(s) rooted at
				{subtree.rootExecutionId}
			</p>
			{#if subtree.truncated}
				<span
					class="rounded border border-warning/40 bg-warning/10 px-1.5 text-micro text-warning"
				>
					list truncated · {formatNumber(subtree.omitted)} omitted
				</span>
			{/if}
		</div>

		<ul class="divide-y divide-border rounded-lg border border-border bg-card">
			{#each visible as row (row.node.executionId)}
				{@const selected = row.node.executionId === currentId}
				<li
					class={cn(
						'flex items-center gap-2 px-3 py-2',
						selected && 'bg-accent/40',
					)}
					style={`padding-left: ${0.75 + row.node.depth * 1.1}rem`}
				>
					{#if row.hasChildren}
						<IconButton
							label={collapsed.has(row.node.executionId)
								? 'Expand child executions'
								: 'Collapse child executions'}
							icon={collapsed.has(row.node.executionId)
								? 'chevron-right'
								: 'chevron-down'}
							iconSize={14}
							compact
							onclick={() => toggle(row.node.executionId)}
						/>
					{:else}
						<span class="inline-block w-6" aria-hidden="true"></span>
					{/if}

					<span
						aria-hidden="true"
						class="font-mono text-micro text-muted-foreground/70"
					>
						{branchGlyphs(row)}
					</span>

					<a
						href={resolve('/executions/[id]', { id: row.node.executionId })}
						class="truncate font-mono text-caption text-foreground underline-offset-2 hover:underline"
					>
						{row.node.executionId}
					</a>

					<span
						class="rounded border border-border px-1.5 text-micro text-muted-foreground"
					>
						{row.node.executionType}
					</span>

					{#if row.node.status}
						<StatusBadge status={row.node.status} size="sm" />
					{:else}
						<span class="text-micro text-muted-foreground/70">
							record unavailable
						</span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</div>
