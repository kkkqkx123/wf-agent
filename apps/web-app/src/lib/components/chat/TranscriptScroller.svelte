<script lang="ts" generics="T">
	import type { Snippet } from 'svelte';
	import { tick } from 'svelte';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import {
		nextAnchorIndex,
		prevAnchorIndex,
	} from '#lib/chat/transcript-anchors';
	import {
		FOLLOW_TAIL_SLACK_PX,
		TRANSCRIPT_OVERSCAN_ROWS,
		TRANSCRIPT_ROW_ESTIMATE_PX,
		TRANSCRIPT_ROW_SIZE,
		VIRTUALIZE_THRESHOLD,
	} from '#lib/config/virtualization';
	import { cn } from '#lib/ui/cn';
	import { t } from '#lib/i18n';
	import { createHotkey } from '@tanstack/svelte-hotkeys';
	import { effectiveHotkey, isShortcutDisabled } from '#lib/shortcuts/registry';

	interface Props {
		items: T[];
		itemKey: (item: T) => string;
		renderItem: Snippet<[T]>;
		/** Live content that follows the transcript, kept outside the window. */
		tail?: Snippet;
		/** Monotonic content counter; while detached it feeds the increment badge. */
		activity?: number;
		/** Bumping this re-pins the viewport to the tail (send path). */
		pin?: number;
		/** Session identity; following and its counter are per-session state. */
		resetKey?: string;
		/** Anchor predicate for next/previous jumps; defaults to every item. */
		isAnchor?: (item: T) => boolean;
		contentClass?: string;
		class?: string;
	}

	let {
		items,
		itemKey,
		renderItem,
		tail,
		activity = 0,
		pin = 0,
		resetKey = '',
		isAnchor,
		contentClass = '',
		class: className = '',
	}: Props = $props();

	let viewport: HTMLDivElement | null = $state(null);
	let scrollTop = $state(0);
	let viewHeight = $state(0);
	let contentHeight = $state(0);
	let following = $state(true);
	let baseline = $state(0);
	/** Timestamp until which scroll events from a programmatic sink keep following. */
	let programmaticUntil = 0;
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
	const awayFromTop = $derived(scrollTop > FOLLOW_TAIL_SLACK_PX);
	const canScroll = $derived(contentHeight - viewHeight > FOLLOW_TAIL_SLACK_PX);
	const showTop = $derived(canScroll && awayFromTop);

	/** Short window where scroll events from a programmatic sink keep following. */
	const PROGRAMMATIC_FOLLOW_MS = 350;

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

	function toTail(smooth = false): void {
		if (!viewport) return;
		programmaticUntil = Date.now() + PROGRAMMATIC_FOLLOW_MS;
		viewport.scrollTo({
			top: viewport.scrollHeight,
			behavior: smooth ? 'smooth' : 'auto',
		});
	}

	function toTop(): void {
		if (!viewport) return;
		following = false;
		scrollTop = 0;
		viewport.scrollTo({ top: 0 });
	}

	export function resume(): void {
		following = true;
		toTail(true);
	}

	export function scrollToTop(): void {
		toTop();
	}

	export function scrollToTail(): void {
		following = true;
		toTail();
	}

	function anchorIdFor(key: string): string {
		return `message-${CSS.escape(key)}`;
	}

	function revealKey(key: string): boolean {
		const index = items.findIndex((item) => itemKey(item) === key);
		if (index < 0) return false;
		const rowIndex = Math.floor(index / TRANSCRIPT_ROW_SIZE);
		const top = offsets[rowIndex] ?? 0;
		following = false;
		if (viewport) {
			programmaticUntil = Date.now() + PROGRAMMATIC_FOLLOW_MS;
			viewport.scrollTo({ top: Math.max(0, top - 8) });
		}
		return true;
	}

	/** Jump to one message anchor; retries until the virtual row mounts. */
	export async function scrollToMessage(
		key: string,
		behavior: ScrollBehavior = 'smooth',
	): Promise<boolean> {
		if (!viewport) return false;
		following = false;
		if (!revealKey(key)) return false;
		for (let attempt = 0; attempt < 4; attempt += 1) {
			await tick();
			const node = document.getElementById(anchorIdFor(key));
			if (node) {
				node.scrollIntoView({ block: 'start', behavior });
				try {
					history.replaceState(null, '', `#message-${key}`);
				} catch {
					// Hash sync is best-effort; jumps never fail on it.
				}
				return true;
			}
		}
		return false;
	}

	/** Viewport cursor lead below the scroll top when resolving the current item. */
	const VIEWPORT_LEAD_PX = 100;

	/**
	 * Item positions counting as anchors, ascending. Derived from the items
	 * so jumps never rescan the transcript.
	 */
	const anchorIndexes = $derived.by(() => {
		const out: number[] = [];
		for (let index = 0; index < items.length; index += 1) {
			if (isAnchor ? isAnchor(items[index]) : true) out.push(index);
		}
		return out;
	});

	/** Viewport cursor as an item index; rows measure as a whole, so the row head stands in. */
	function currentIndex(): number {
		if (!viewport || items.length === 0) return 0;
		const rowIndex = rowAt(scrollTop + VIEWPORT_LEAD_PX);
		return Math.min(rowIndex * TRANSCRIPT_ROW_SIZE, items.length - 1);
	}

	/** Jump to the next anchor toward the tail; at the end, resume follow. */
	export function scrollToNextAnchor(): void {
		if (anchorIndexes.length === 0) return;
		const target = nextAnchorIndex(anchorIndexes, currentIndex());
		if (target === null) {
			resume();
			try {
				history.replaceState(
					null,
					'',
					window.location.pathname + window.location.search,
				);
			} catch {
				// Hash cleanup is best-effort.
			}
			return;
		}
		void scrollToMessage(itemKey(items[target]));
	}

	/** Jump to the previous anchor toward the head. */
	export function scrollToPrevAnchor(): void {
		if (anchorIndexes.length === 0) return;
		const target =
			prevAnchorIndex(anchorIndexes, currentIndex()) ?? anchorIndexes[0];
		if (target === undefined) return;
		void scrollToMessage(itemKey(items[target]));
	}

	function onScroll(event: Event): void {
		const node = event.currentTarget as HTMLDivElement;
		scrollTop = node.scrollTop;
		viewHeight = node.clientHeight;
		contentHeight = node.scrollHeight;
		const shortBy = node.scrollHeight - node.scrollTop - node.clientHeight;
		if (Date.now() < programmaticUntil) {
			if (shortBy <= FOLLOW_TAIL_SLACK_PX) following = true;
			return;
		}
		following = shortBy <= FOLLOW_TAIL_SLACK_PX;
	}

	let lastResetKey = $state('');
	let lastPin = $state(0);

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
		if (pin === lastPin) return;
		lastPin = pin;
		following = true;
		toTail();
	});

	$effect(() => {
		const current = activity;
		if (following) {
			baseline = current;
			toTail();
		}
	});

	// Detached growth never fires a viewport scroll event, so the top-button
	// predicate re-reads the content height once the rows settle.
	$effect(() => {
		void items.length;
		void activity;
		const node = viewport;
		if (!node) return;
		void tick().then(() => {
			contentHeight = node.scrollHeight;
			viewHeight = node.clientHeight;
		});
	});

	// Global jump keys work from any input state; bare keys keep native
	// semantics. Bindings resolve through the shortcut catalog so user
	// overrides apply.
	createHotkey(
		effectiveHotkey('transcript.toBottom'),
		(event) => {
			event.preventDefault();
			following = true;
			toTail();
		},
		{
			enabled: !isShortcutDisabled('transcript.toBottom'),
			ignoreInputs: false,
		},
	);
	createHotkey(
		effectiveHotkey('transcript.toTop'),
		(event) => {
			event.preventDefault();
			toTop();
		},
		{ enabled: !isShortcutDisabled('transcript.toTop'), ignoreInputs: false },
	);
	createHotkey(
		effectiveHotkey('transcript.anchorNext'),
		(event) => {
			event.preventDefault();
			scrollToNextAnchor();
		},
		{
			enabled: !isShortcutDisabled('transcript.anchorNext'),
			ignoreInputs: false,
		},
	);
	createHotkey(
		effectiveHotkey('transcript.anchorPrev'),
		(event) => {
			event.preventDefault();
			scrollToPrevAnchor();
		},
		{
			enabled: !isShortcutDisabled('transcript.anchorPrev'),
			ignoreInputs: false,
		},
	);

	// Deep links: an empty hash follows the tail, a message hash reveals
	// and scrolls to that anchor, unknown hashes fall back to the tail.
	$effect(() => {
		void items.length;
		const hash = window.location.hash;
		if (!hash) return;
		const match = /^#message-(.+)$/.exec(hash);
		if (!match) return;
		const key = decodeURIComponent(match[1]);
		void tick().then(() => {
			void scrollToMessage(key, 'auto');
		});
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

	{#if !following || showTop}
		<div
			class="absolute bottom-3 left-1/2 flex -translate-x-1/2 items-center gap-2"
		>
			{#if showTop}
				<button
					type="button"
					onclick={toTop}
					aria-label={t('chat.transcriptScroller.backToTop')}
					class="flex items-center gap-1.5 rounded-full border border-border bg-card px-3 py-1 text-caption text-card-foreground shadow-sm transition-colors hover:bg-accent"
				>
					<Icon name="chevron-up" size={13} />
					<span class="hidden sm:inline"
						>{t('chat.transcriptScroller.backToTop')}</span
					>
				</button>
			{/if}
			{#if !following}
				<button
					type="button"
					onclick={resume}
					aria-label={increments > 0
						? t('chat.transcriptScroller.backToTailNew', { count: increments })
						: t('chat.transcriptScroller.backToTail')}
					class="flex items-center gap-1.5 rounded-full border border-border bg-card px-3 py-1 text-caption text-card-foreground shadow-sm transition-colors hover:bg-accent"
				>
					<Icon name="chevron-down" size={13} />
					<span class="hidden sm:inline"
						>{t('chat.transcriptScroller.backToTail')}</span
					>
					{#if increments > 0}
						<span class="font-mono text-micro text-muted-foreground">
							+{increments}
						</span>
					{/if}
				</button>
			{/if}
		</div>
	{/if}
</div>
