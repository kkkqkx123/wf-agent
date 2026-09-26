<script lang="ts">
	import type { Snippet } from 'svelte';
	import Sheet from '$lib/components/ui/Sheet.svelte';
	import Separator from '$lib/components/ui/Separator.svelte';
	import { ui } from '$lib/stores/ui.svelte';
	import { cn } from '$lib/utils/cn';

	interface Props {
		inspectorTitle?: string;
		inspectorWidth?: string;
		inspectorOpen?: boolean;
		class?: string;
		children: Snippet;
		inspector?: Snippet;
	}

	let {
		inspectorTitle = 'Details',
		inspectorWidth = '22.5rem',
		inspectorOpen = false,
		class: className = '',
		children,
		inspector,
	}: Props = $props();

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
			style:width={inspectorWidth}
			class="flex min-h-0 shrink-0 flex-col overflow-hidden bg-card"
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

{#if inspector && !ui.inspectorDocked}
	<Sheet
		open={ui.inspectorOpen}
		title={inspectorTitle}
		side="right"
		width={inspectorWidth}
		onclose={() => ui.closeInspector()}
	>
		{@render inspector()}
	</Sheet>
{/if}
