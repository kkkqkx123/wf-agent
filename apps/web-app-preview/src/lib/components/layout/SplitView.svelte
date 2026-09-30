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
		class?: string;
		children: Snippet;
		inspector?: Snippet;
	}

	let {
		inspectorTitle = 'Details',
		inspectorWidth,
		inspectorOpen = false,
		oninspectorclose,
		class: className = '',
		children,
		inspector,
	}: Props = $props();

	const widthCss = $derived(
		`${inspectorWidth ?? preferences.inspectorWidth}px`,
	);

	// Wide viewports dock the inspector; narrower ones overlay it as a sheet.
	const docked = $derived(ui.inspectorDocked && inspectorOpen);

	$effect(() => {
		if (!inspectorOpen) {
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
</div>

{#if inspector && !ui.inspectorDocked}
	<Sheet
		open={ui.inspectorOpen}
		title={inspectorTitle}
		side="right"
		width={widthCss}
		onclose={() => {
			ui.closeInspector();
			oninspectorclose?.();
		}}
	>
		{@render inspector()}
	</Sheet>
{/if}
