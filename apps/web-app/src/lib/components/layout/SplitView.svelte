<script lang="ts">
	import type { Snippet } from 'svelte';
	import Sheet from '#lib/ui/components/Sheet.svelte';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import Separator from '#lib/ui/components/Separator.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import { ui } from '#lib/stores/ui.svelte';
	import { preferences } from '#lib/stores/preferences.svelte';
	import { cn } from '#lib/ui/cn';
	import { t } from '#lib/i18n';

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
		inspectorTitle = '',
		inspectorWidth,
		inspectorOpen = false,
		oninspectorclose,
		dual = false,
		secondaryTitle = '',
		secondary,
		secondaryOpen,
		class: className = '',
		children,
		inspector,
	}: Props = $props();

	// Defaults resolve at render time so they follow the active locale.
	const resolvedInspectorTitle = $derived(
		inspectorTitle || t('layout.splitView.details'),
	);
	const resolvedSecondaryTitle = $derived(
		secondaryTitle || t('layout.splitView.compare'),
	);

	const widthCss = $derived(
		`${inspectorWidth ?? preferences.inspectorWidth}px`,
	);

	let collapsed = $state(false);

	$effect(() => {
		if (!inspectorOpen) collapsed = false;
	});

	function startResize(event: PointerEvent): void {
		const handle = event.currentTarget as HTMLElement | null;
		if (!handle) return;
		event.preventDefault();
		handle.setPointerCapture(event.pointerId);
		const onMove = (moveEvent: PointerEvent): void => {
			const width = window.innerWidth - moveEvent.clientX;
			preferences.setInspectorWidth(width);
		};
		const onUp = (upEvent: PointerEvent): void => {
			handle.releasePointerCapture(upEvent.pointerId);
			handle.removeEventListener('pointermove', onMove);
			handle.removeEventListener('pointerup', onUp);
			handle.removeEventListener('pointercancel', onUp);
		};
		handle.addEventListener('pointermove', onMove);
		handle.addEventListener('pointerup', onUp);
		handle.addEventListener('pointercancel', onUp);
	}

	// Wide viewports dock the inspector; narrower ones overlay it as a sheet.
	const docked = $derived(ui.inspectorDocked && inspectorOpen && !collapsed);
	const secondaryVisible = $derived(
		!collapsed && dual && secondary !== undefined && (secondaryOpen ?? true),
	);
	const dockedSecondary = $derived(ui.inspectorDocked && secondaryVisible);
	const sheetTitle = $derived(
		secondaryVisible
			? `${resolvedInspectorTitle} · ${resolvedSecondaryTitle}`
			: resolvedInspectorTitle,
	);

	$effect(() => {
		const anyOpen = (inspectorOpen && !collapsed) || secondaryVisible;
		if (!anyOpen) {
			ui.closeInspector();
		} else if (!ui.inspectorDocked) {
			ui.openInspector(resolvedInspectorTitle);
		}
	});
</script>

<div class={cn('flex h-full min-h-0 w-full', className)}>
	<div class="min-w-0 flex-1 overflow-hidden">
		{@render children()}
	</div>

	{#if inspectorOpen && collapsed}
		<button
			type="button"
			class="flex w-8 shrink-0 items-center justify-center border-l border-border bg-card text-muted-foreground hover:text-foreground"
			onclick={() => (collapsed = false)}
			aria-label={t('layout.splitView.showDetails')}
			title={t('layout.splitView.showDetails')}
		>
			<Icon name="panel-right" size={14} />
		</button>
	{/if}

	{#if inspector && docked}
		<Separator orientation="vertical" />
		<aside
			style:width={widthCss}
			class="relative flex min-h-0 shrink-0 flex-col overflow-hidden bg-card"
		>
			<button
				type="button"
				aria-label={t('layout.splitView.resizeDetails')}
				class="absolute left-0 top-0 z-10 h-full w-1.5 cursor-col-resize bg-transparent hover:bg-primary/20"
				onpointerdown={startResize}
			></button>
			<div class="flex h-full min-h-0 flex-col">
				<div
					class="flex h-11 shrink-0 items-center justify-between gap-2 border-b border-border px-3"
				>
					<h2 class="truncate text-title font-semibold">
						{resolvedInspectorTitle}
					</h2>
					<IconButton
						icon="chevron-right"
						label={t('layout.splitView.hideDetails')}
						onclick={() => (collapsed = true)}
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
			class="relative flex min-h-0 shrink-0 flex-col overflow-hidden bg-card"
		>
			<button
				type="button"
				aria-label={t('layout.splitView.resizeDetails')}
				class="absolute left-0 top-0 z-10 h-full w-1.5 cursor-col-resize bg-transparent hover:bg-primary/20"
				onpointerdown={startResize}
			></button>
			<div class="flex h-full min-h-0 flex-col">
				<div
					class="flex h-11 shrink-0 items-center justify-between gap-2 border-b border-border px-3"
				>
					<h2 class="truncate text-title font-semibold">
						{resolvedSecondaryTitle}
					</h2>
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
				<h3 class="mb-2 truncate text-title font-semibold">
					{resolvedSecondaryTitle}
				</h3>
				{@render secondary()}
			</div>
		{/if}
	</Sheet>
{/if}
