<script lang="ts" generics="T">
	import type { Snippet } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import {
		FOLLOW_TAIL_SLACK_PX,
		TRANSCRIPT_OVERSCAN_ROWS,
		TRANSCRIPT_ROW_ESTIMATE_PX,
		TRANSCRIPT_ROW_SIZE,
		VIRTUALIZE_THRESHOLD,
	} from '$lib/config/virtualization';
	import { cn } from '$lib/utils/cn';

	interface Props {
		items: T[];
		itemKey: (item: T) => string;
		renderItem: Snippet<[T]>;
		/** Live content that follows the transcript, kept outside the window. */
		tail?: Snippet;
		/** Monotonic content counter; while detached it feeds the increment badge. */
		activity?: number;
		/** Session identity; following and its counter are per-session state. */
		resetKey?: string;
		contentClass?: string;
		class?: string;
	}

	let {
		items,
		itemKey,
		renderItem,
		tail,
		activity = 0,
		resetKey = '',
		contentClass = '',
		class: className = '',
	}: Props = $props();

	let viewport: HTMLDivElement | null = $state(null);
	let scrollTop = $state(0);
	let viewHeight = $state(0);
	let following = $state(true);
	let baseline = $state(0);
	/** Row heights measured so far, keyed by the row's first item. */
	let heights = $state<Record<string, number>>({});

	const rows = $derived.by(() => {
		const out: Array<{ key: string; items: T[] }> = [];
		for (let start = 0; start < items.length; start += TRANSCRIPT_ROW_SIZE) {
			const group = items.slice(start, start + TRANSCRIPT_ROW_SIZE);
			out.push({ key: itemKey(group[0]), items: group });
		}
		return out;
	});

	const windowed = $derived(items.length > VIRTUALIZE_THRESHOLD);

	const offsets = $derived.by(() => {
		const out: number[] = [0];
		for (const row of rows) {
			out.push(
				out[out.length - 1] + (heights[row.key] ?? TRANSCRIPT_ROW_ESTIMATE_PX),
			);
		}
		return out;
	});

	function rowAt(offset: number): number {
		let low = 0;
		let high = offsets.length - 1;
		while (low < high) {
			const middle = (low + high) >> 1;
			if (offsets[middle + 1] <= offset) low = middle + 1;
			else high = middle;
		}
		return low;
	}

	const first = $derived(
		windowed ? Math.max(0, rowAt(scrollTop) - TRANSCRIPT_OVERSCAN_ROWS) : 0,
	);
	const last = $derived(
		windowed
			? Math.min(
					rows.length,
					rowAt(scrollTop + viewHeight) + 1 + TRANSCRIPT_OVERSCAN_ROWS,
				)
			: rows.length,
	);
	const visible = $derived(rows.slice(first, last));
	const padTop = $derived(windowed ? offsets[first] : 0);
	const padBottom = $derived(
		windowed ? offsets[rows.length] - offsets[last] : 0,
	);
	const increments = $derived(Math.max(0, activity - baseline));

	/** Rows are keyed by their own identity, so the measured key never changes. */
	function measure(node: HTMLElement, key: string) {
		const report = () => {
			const height = node.offsetHeight;
			if (heights[key] === height) return;
			heights[key] = height;
			// A corrected row height moves the window offsets, so an anchored
			// transcript reads its own bottom again.
			if (following) toTail();
		};
		const observer = new ResizeObserver(report);
		observer.observe(node);
		report();
		return { destroy: () => observer.disconnect() };
	}

	function toTail(): void {
		if (!viewport) return;
		viewport.scrollTo({ top: viewport.scrollHeight });
	}

	function resume(): void {
		following = true;
		toTail();
	}

	function onScroll(event: Event): void {
		const node = event.currentTarget as HTMLDivElement;
		scrollTop = node.scrollTop;
		const shortBy = node.scrollHeight - node.scrollTop - node.clientHeight;
		following = shortBy <= FOLLOW_TAIL_SLACK_PX;
	}

	let lastResetKey = $state('');

	// Selecting text can scroll the column on its own, so an anchored selection
	// takes the tail over from the transcript.
	$effect(() => {
		if (!following) return;
		const onSelectionChange = () => {
			if ((window.getSelection()?.toString() ?? '') !== '') following = false;
		};
		document.addEventListener('selectionchange', onSelectionChange);
		return () =>
			document.removeEventListener('selectionchange', onSelectionChange);
	});

	$effect(() => {
		if (resetKey === lastResetKey) return;
		lastResetKey = resetKey;
		following = true;
	});

	$effect(() => {
		const current = activity;
		if (following) {
			baseline = current;
			toTail();
		}
	});
</script>

<div class={cn('relative flex min-h-0 flex-col', className)}>
	<div
		bind:this={viewport}
		bind:clientHeight={viewHeight}
		onscroll={onScroll}
		class="min-h-0 flex-1 overflow-y-auto"
	>
		<div class={cn('flex flex-col gap-3', contentClass)}>
			{#if padTop > 0}
				<div aria-hidden="true" style:height="{padTop}px"></div>
			{/if}
			{#each visible as row (row.key)}
				<div use:measure={row.key} class="flex flex-col gap-3">
					{#each row.items as item (itemKey(item))}
						{@render renderItem(item)}
					{/each}
				</div>
			{/each}
			{#if padBottom > 0}
				<div aria-hidden="true" style:height="{padBottom}px"></div>
			{/if}
			{@render tail?.()}
		</div>
	</div>

	{#if !following}
		<button
			type="button"
			onclick={resume}
			class="absolute bottom-3 left-1/2 flex -translate-x-1/2 items-center gap-1.5 rounded-full border border-border bg-card px-3 py-1 text-caption text-card-foreground shadow-sm transition-colors hover:bg-accent"
		>
			<Icon name="chevron-down" size={13} />
			Back to tail
			{#if increments > 0}
				<span class="font-mono text-micro text-muted-foreground">
					+{increments}
				</span>
			{/if}
		</button>
	{/if}
</div>
