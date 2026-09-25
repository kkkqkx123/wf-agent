<script lang="ts">
	import type { Snippet } from 'svelte';
	import Sheet from '$lib/components/ui/Sheet.svelte';
	import {
		INSPECTOR_WIDTH_MAX,
		INSPECTOR_WIDTH_MIN,
		preferences,
	} from '$lib/stores/preferences.svelte';
	import { cn } from '$lib/utils/cn';

	interface Props {
		inspectorTitle?: string;
		inspectorOpen?: boolean;
		class?: string;
		children: Snippet;
		inspector?: Snippet;
		/** The overlay is the only dismissible inspector, so closing it is the page's job. */
		oninspectorclose?: () => void;
	}

	let {
		inspectorTitle = 'Details',
		inspectorOpen = false,
		class: className = '',
		children,
		inspector,
		oninspectorclose,
	}: Props = $props();

	// Docking is decided in CSS: the pinned inspector sits inline from `lg`
	// upwards, while the sheet covers everything below that (or every viewport
	// while unpinned). No resize listener is involved.
	const pinned = $derived(preferences.inspectorPinned);

	let drag: { pointerId: number; startX: number; startWidth: number } | null =
		$state(null);
	let dragWidth = $state(0);
	const widthPx = $derived(drag ? dragWidth : preferences.inspectorWidth);
	const width = $derived(`${widthPx}px`);

	function onHandleDown(
		event: PointerEvent & { currentTarget: HTMLDivElement },
	): void {
		event.currentTarget.setPointerCapture(event.pointerId);
		dragWidth = preferences.inspectorWidth;
		drag = {
			pointerId: event.pointerId,
			startX: event.clientX,
			startWidth: preferences.inspectorWidth,
		};
	}

	function onHandleMove(event: PointerEvent): void {
		const active = drag;
		if (!active || event.pointerId !== active.pointerId) return;
		// The inspector is right-aligned, so dragging left widens it.
		dragWidth = Math.min(
			INSPECTOR_WIDTH_MAX,
			Math.max(
				INSPECTOR_WIDTH_MIN,
				active.startWidth - (event.clientX - active.startX),
			),
		);
	}

	function onHandleUp(event: PointerEvent): void {
		const active = drag;
		if (!active || event.pointerId !== active.pointerId) return;
		drag = null;
		preferences.setInspectorWidth(dragWidth);
	}
</script>

<div class={cn('flex h-full min-h-0 w-full', className)}>
	<div class="min-w-0 flex-1 overflow-hidden">
		{@render children()}
	</div>

	{#if inspector && pinned}
		<div
			aria-hidden="true"
			class="hidden w-1 shrink-0 cursor-col-resize transition-colors hover:bg-ring/60 lg:block"
			onpointerdown={onHandleDown}
			onpointermove={onHandleMove}
			onpointerup={onHandleUp}
			onpointercancel={onHandleUp}
		></div>
		<aside
			style:width
			class="hidden min-h-0 shrink-0 flex-col overflow-hidden border-l border-border bg-card lg:flex"
		>
			<div class="flex h-full min-h-0 flex-col">
				<div
					class="flex h-11 shrink-0 items-center justify-between border-b border-border px-3"
				>
					<h2 class="truncate text-title font-semibold">{inspectorTitle}</h2>
				</div>
				<div class="min-h-0 flex-1 overflow-y-auto">
					{@render inspector()}
				</div>
			</div>
		</aside>
	{/if}
</div>

{#if inspector}
	<div class={pinned ? 'lg:hidden' : ''}>
		<Sheet
			open={inspectorOpen}
			title={inspectorTitle}
			side="right"
			{width}
			onclose={oninspectorclose}
		>
			{@render inspector()}
		</Sheet>
	</div>
{/if}
