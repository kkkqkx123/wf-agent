<script lang="ts">
	import type { Snippet } from 'svelte';
	import Sheet from '@wf-agent/ui/components/Sheet.svelte';
	import Separator from '@wf-agent/ui/components/Separator.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import { ui } from '$lib/stores/ui.svelte';
	import { preferences } from '$lib/stores/preferences.svelte';
	import { cn } from '@wf-agent/ui/cn';

	interface Props {
		inspectorTitle?: string;
		/** Pixel override; falls back to the width remembered in preferences. */
		inspectorWidth?: number;
		inspectorOpen?: boolean;
		oninspectorclose?: () => void;
		/** Dual compare: a second inspector pane beside the first.
		 * Wide viewports dock both; narrower ones stack both in one sheet. */
		dual?: boolean;
		secondaryTitle?: string;
		secondary?: Snippet;
		secondaryOpen?: boolean;
		class?: string;
		children: Snippet;
		inspector?: Snippet;
	}

	let {
		inspectorTitle = 'Details',
		inspectorWidth,
		inspectorOpen = false,
		oninspectorclose,
		dual = false,
		secondaryTitle = 'Compare',
		secondary,
		secondaryOpen,
		class: className = '',
		children,
		inspector,
	}: Props = $props();

	const widthCss = $derived(
		`${inspectorWidth ?? preferences.inspectorWidth}px`,
	);

	// Wide viewports dock the inspector; narrower ones overlay it as a sheet.
	const docked = $derived(ui.inspectorDocked && inspectorOpen);
	const secondaryVisible = $derived(
		dual && secondary !== undefined && (secondaryOpen ?? true),
	);
	const dockedSecondary = $derived(ui.inspectorDocked && secondaryVisible);
	const sheetTitle = $derived(
		secondaryVisible ? `${inspectorTitle} · ${secondaryTitle}` : inspectorTitle,
	);

	$effect(() => {
		const anyOpen = inspectorOpen || secondaryVisible;
		if (!anyOpen) {
			ui.closeInspector();
		} else if (!ui.inspectorDocked) {
			ui.openInspector(inspectorTitle);
		}
	});
</script>

<div class={cn('flex h-full min-h-0 w-full', className)}>
	<div class="min-w-0 flex-1 overflow-hidden">
		{@render children()}
	</div>

	{#if inspector && docked}
		<Separator orientation="vertical" />
		<aside
			style:width={widthCss}
			class="flex min-h-0 shrink-0 flex-col overflow-hidden bg-card"
		>
			<div class="flex h-full min-h-0 flex-col">
				<div
					class="flex h-11 shrink-0 items-center justify-between gap-2 border-b border-border px-3"
				>
					<h2 class="truncate text-title font-semibold">{inspectorTitle}</h2>
					<IconButton
						icon="sliders"
						label="Inspector width: {preferences.inspectorWidthLabel}"
						onclick={() => preferences.cycleInspectorWidth()}
					/>
				</div>
				<div class="min-h-0 flex-1 overflow-y-auto">
					{@render inspector()}
				</div>
			</div>
		</aside>
	{/if}
	{#if secondary && dockedSecondary}
		<Separator orientation="vertical" />
		<aside
			style:width={widthCss}
			class="flex min-h-0 shrink-0 flex-col overflow-hidden bg-card"
		>
			<div class="flex h-full min-h-0 flex-col">
				<div
					class="flex h-11 shrink-0 items-center justify-between gap-2 border-b border-border px-3"
				>
					<h2 class="truncate text-title font-semibold">{secondaryTitle}</h2>
				</div>
				<div class="min-h-0 flex-1 overflow-y-auto">
					{@render secondary()}
				</div>
			</div>
		</aside>
	{/if}
</div>

{#if (inspector || secondaryVisible) && !ui.inspectorDocked}
	<Sheet
		open={ui.inspectorOpen}
		title={sheetTitle}
		side="right"
		width={widthCss}
		onclose={() => {
			ui.closeInspector();
			oninspectorclose?.();
		}}
	>
		{#if inspector}
			{@render inspector()}
		{/if}
		{#if secondary && secondaryVisible}
			<div class="mt-4 border-t border-border pt-3">
				<h3 class="mb-2 truncate text-title font-semibold">{secondaryTitle}</h3>
				{@render secondary()}
			</div>
		{/if}
	</Sheet>
{/if}
