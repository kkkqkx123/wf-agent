<script lang="ts">
	import type { Snippet } from 'svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Select from '#lib/ui/components/Select.svelte';
	import { shortcutTitle } from '#lib/graph/canvas-shortcuts';
	import type { GraphLayoutKind } from '#lib/graph/display-model';
	import { t } from '#lib/i18n';
	import { preferences } from '#lib/stores/preferences.svelte';

	interface Props {
		layout: GraphLayoutKind;
		showFilters: boolean;
		editable: boolean;
		editMode: boolean;
		editDirty: boolean;
		canUndo: boolean;
		canRedo: boolean;
		editBusy: boolean;
		canZoomSelection: boolean;
		canFitHighlight: boolean;
		layoutId?: string;
		foldedCount: number;
		truncated: boolean;
		shownCount: number;
		totalCount: number;
		nodeCount: number;
		edgeCount: number;
		actions?: Snippet;
		onzoom?: () => void;
		onzoomout?: () => void;
		onfit?: () => void;
		onrelayout?: () => void;
		onresetlayout?: () => void;
		onexport?: () => void;
		ontogglefilters?: () => void;
		onzoomselection?: () => void;
		onfithighlight?: () => void;
		onenteredit?: () => void;
		onexitedit?: () => void;
		onundo?: () => void;
		onredo?: () => void;
		onaddnode?: () => void;
		ondeleteselected?: () => void;
		onsave?: () => void;
		onvalidate?: () => void;
		onpromote?: () => void;
	}

	let {
		layout = $bindable(),
		showFilters,
		editable,
		editMode,
		editDirty,
		canUndo,
		canRedo,
		editBusy,
		canZoomSelection,
		canFitHighlight,
		layoutId,
		foldedCount,
		truncated,
		shownCount,
		totalCount,
		nodeCount,
		edgeCount,
		actions,
		onzoom,
		onzoomout,
		onfit,
		onrelayout,
		onresetlayout,
		onexport,
		ontogglefilters,
		onzoomselection,
		onfithighlight,
		onenteredit,
		onexitedit,
		onundo,
		onredo,
		onaddnode,
		ondeleteselected,
		onsave,
		onvalidate,
		onpromote,
	}: Props = $props();

	const layoutOptions = $derived([
		{ value: 'layered', label: t('domain.graph.layoutLayered') },
		{ value: 'columns', label: t('domain.graph.layoutColumns') },
		{ value: 'rows', label: t('domain.graph.layoutRows') },
		{ value: 'compact', label: t('domain.graph.layoutCompact') },
	]);

	function cycleMinimap(): void {
		const current = preferences.minimapMode;
		preferences.setMinimapMode(current === 'auto' ? 'on' : current === 'on' ? 'off' : 'auto');
	}
</script>

<div class="flex flex-wrap items-center gap-1.5">
	<IconButton
		icon="plus"
		label={shortcutTitle('canvas.zoomIn', t('domain.graph.zoomIn'))}
		onclick={() => onzoom?.()}
	/>
	<IconButton
		icon="minus"
		label={shortcutTitle('canvas.zoomOut', t('domain.graph.zoomOut'))}
		onclick={() => onzoomout?.()}
	/>
	<IconButton
		icon="maximize"
		label={shortcutTitle('canvas.fit', t('domain.graph.fitView'))}
		onclick={() => onfit?.()}
	/>
	<IconButton
		icon="refresh"
		label={shortcutTitle('canvas.relayout', t('domain.graph.rerunLayout'))}
		onclick={() => onrelayout?.()}
	/>
	{#if layoutId}
		<IconButton
			icon="undo"
			label={t('domain.graph.resetLayout')}
			onclick={() => onresetlayout?.()}
		/>
	{/if}
	<Select bind:value={layout} options={layoutOptions} size="sm" placeholder="" class="w-28" />
	<IconButton icon="download" label={t('domain.graph.exportPng')} onclick={() => onexport?.()} />
	<IconButton
		icon="filter"
		label={showFilters ? t('domain.graph.hideFilters') : t('domain.graph.showFilters')}
		onclick={() => ontogglefilters?.()}
	/>
	<Button
		variant={preferences.minimapMode === 'off' ? 'outline' : 'default'}
		size="sm"
		onclick={cycleMinimap}
		title={t('domain.graph.cycleMinimap')}
	>
		{t('domain.graph.minimap')}
		{preferences.minimapMode}
	</Button>
	<Button
		variant="outline"
		size="sm"
		disabled={!canZoomSelection}
		onclick={() => onzoomselection?.()}
	>
		{t('domain.graph.zoomToSelection')}
	</Button>
	<Button
		variant="outline"
		size="sm"
		disabled={!canFitHighlight}
		onclick={() => onfithighlight?.()}
	>
		{t('domain.graph.fitHighlight')}
	</Button>
	{#if actions}
		<span class="mx-1 h-5 w-px bg-border"></span>
		{@render actions()}
	{/if}
	{#if editable}
		<span class="mx-1 h-5 w-px bg-border"></span>
		{#if !editMode}
			<Button variant="outline" size="sm" onclick={() => onenteredit?.()}>
				{t('domain.graph.enterEditMode')}
			</Button>
		{:else}
			<Badge variant="warning"
				>{editDirty ? t('domain.graph.editingUnsaved') : t('domain.graph.editing')}</Badge
			>
			<Button variant="outline" size="sm" disabled={!canUndo} onclick={() => onundo?.()}>
				{t('domain.graph.undo')}
			</Button>
			<Button variant="outline" size="sm" disabled={!canRedo} onclick={() => onredo?.()}>
				{t('domain.graph.redo')}
			</Button>
			<Button variant="outline" size="sm" onclick={() => onaddnode?.()}>
				{t('domain.graph.addNode')}
			</Button>
			<Button variant="outline" size="sm" onclick={() => ondeleteselected?.()}>
				{t('domain.graph.deleteSelected')}
			</Button>
			<Button size="sm" disabled={!editDirty || editBusy} onclick={() => onsave?.()}>
				{editBusy ? t('domain.graph.saving') : t('domain.graph.saveDraft')}
			</Button>
			<Button variant="ghost" size="sm" onclick={() => onvalidate?.()}>
				{t('domain.graph.validate')}
			</Button>
			<Button variant="ghost" size="sm" onclick={() => onpromote?.()}>
				{t('domain.graph.promote')}
			</Button>
			<Button variant="ghost" size="sm" onclick={() => onexitedit?.()}>
				{t('domain.graph.exit')}
			</Button>
		{/if}
	{/if}
	<span class="ml-auto text-micro text-muted-foreground">
		{#if foldedCount > 0}
			{t('domain.graph.foldedGroups', { count: foldedCount })} ·
		{/if}
		{#if truncated}
			{t('domain.graph.showingOf', {
				shown: shownCount,
				total: totalCount,
			})}
			· {t('domain.graph.retentionOrder')} ·
		{/if}
		{t('domain.canvas.summary', {
			nodes: nodeCount,
			edges: edgeCount,
		})}
	</span>
</div>

{#if editMode}
	<p class="text-micro text-muted-foreground">
		{t('domain.graph.editHint')}
	</p>
{/if}
