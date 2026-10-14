<script lang="ts">
	import { onMount } from 'svelte';
	import { beforeNavigate } from '$app/navigation';
	import { page } from '$app/state';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Segmented from '@wf-agent/ui/components/Segmented.svelte';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import UnsavedChangesDialog from '@wf-agent/ui/components/UnsavedChangesDialog.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import TemplateDetailPreview from '$lib/components/domain/templates/TemplateDetailPreview.svelte';
	import TemplateCreatePanel from '$lib/components/domain/templates/TemplateCreatePanel.svelte';
	import TemplateEditSection from '$lib/components/domain/templates/TemplateEditSection.svelte';
	import {
		getTemplateDetail,
		type TemplateDetail,
	} from '$lib/services/templates';
	import type { TemplateKind } from '$lib/types/models';
	import {
		TemplateEditController,
		VIEWS,
		type DetailView,
	} from '$lib/templates/template-edit-session.svelte';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';

	const KINDS: readonly TemplateKind[] = [
		'node',
		'trigger',
		'agent',
		'workflow',
	];

	function isKind(raw: string): raw is TemplateKind {
		return (KINDS as readonly string[]).includes(raw);
	}

	function isView(raw: string | undefined): raw is DetailView {
		return raw !== undefined && (VIEWS as readonly string[]).includes(raw);
	}

	const routeKindParam = $derived(page.params.kind ?? '');
	const routeIdParam = $derived(page.params.id ?? '');
	const kind = $derived<TemplateKind | null>(
		isKind(routeKindParam) ? routeKindParam : null,
	);
	const isNew = $derived(routeIdParam === 'new');

	let detail = $state<TemplateDetail | null>(null);
	let detailLoading = $state(true);
	let detailError = $state<string | null>(null);
	let loadedKey = $state('');

	const controller = new TemplateEditController({
		kind: () => kind,
		isNew: () => isNew,
		detail: () => detail,
		routeIdParam: () => routeIdParam,
		loadDetail: (activeKind, id, view) => loadDetail(activeKind, id, view),
	});

	// Mirror the editing state back to the address bar so the detail view is
	// refresh-safe and shareable. Gated on the settled detail so the entry
	// URL (?tab=edit) survives until the first load applies it.
	$effect(() => {
		if (detailLoading || kind === null || (detail === null && !isNew)) return;
		const view: DetailView = !controller.editMode
			? 'preview'
			: controller.editTab === 'graph'
				? 'graph'
				: 'edit';
		void gotoWithParams(page.url, { tab: view === 'preview' ? '' : view });
	});

	// Keep the session document fresh while typing; failed parses retain
	// the last valid document so the form and canvas never clear.
	$effect(() => {
		controller.watchSessionText();
	});

	// Explicit check results only describe the validated text; any later
	// edit returns the badge and server issues to idle.
	$effect(() => {
		controller.watchValidationReset();
	});

	// Reload when the route identity changes (list navigation, clone jumps).
	// The view intent always comes from the live URL so clone and save jumps
	// land on the view their address carries.
	$effect(() => {
		const activeKind = kind;
		if (activeKind === null) {
			detailLoading = false;
			return;
		}
		const viewParam = parseListParams(page.url).tab;
		const view: DetailView = isView(viewParam)
			? viewParam
			: isNew
				? 'edit'
				: 'preview';
		if (isNew) {
			controller.startNewTemplate(activeKind);
			detail = null;
			detailError = null;
			detailLoading = false;
			loadedKey = `${activeKind}/new`;
			return;
		}
		const key = `${activeKind}/${routeIdParam}`;
		if (key === loadedKey) return;
		loadedKey = key;
		controller.resetEditState();
		controller.templateEditStore.load([], []);
		void loadDetail(activeKind, routeIdParam, view);
	});

	async function loadDetail(
		activeKind: TemplateKind,
		id: string,
		view: DetailView,
	): Promise<void> {
		detailLoading = true;
		detailError = null;
		try {
			detail = await getTemplateDetail(id, activeKind);
			if (view !== 'preview') {
				controller.enterEdit(view === 'graph' ? 'graph' : 'json');
			} else {
				controller.editMode = false;
			}
		} catch (e) {
			detailError = e instanceof Error ? e.message : 'Template detail failed.';
			detail = null;
		} finally {
			detailLoading = false;
		}
	}

	onMount(() => {
		const onBeforeUnload = (event: BeforeUnloadEvent): void => {
			if (controller.editDirty()) event.preventDefault();
		};
		window.addEventListener('beforeunload', onBeforeUnload);
		return () => window.removeEventListener('beforeunload', onBeforeUnload);
	});

	beforeNavigate((navigation) => {
		if (navigation.willUnload) return;
		if (!controller.editDirty()) return;
		navigation.cancel();
		controller.pendingNavUrl = navigation.to?.url.toString() ?? null;
		controller.unsavedOpen = true;
	});
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title={isNew
			? 'New template'
			: (detail?.template.name ?? 'Template detail')}
		description={isNew
			? 'Create a template in the library.'
			: kind
				? `${kind} template · ${routeIdParam}`
				: 'Template detail'}
	>
		{#snippet actions()}
			{#if controller.insertNodeType !== null}
				<Button
					variant="outline"
					size="sm"
					onclick={() => controller.insertIntoCanvas()}
				>
					<Icon name="blocks" size={13} />
					Insert as {controller.insertNodeType}
				</Button>
			{/if}
			<Button variant="ghost" size="sm" href="/templates">
				<Icon name="arrow-left" size={13} />
				Library
			</Button>
		{/snippet}
	</PageHeader>

	{#if kind === null}
		<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
			<ErrorState
				title="Unknown template kind"
				description={`Expected one of node, trigger, agent, workflow; got "${routeKindParam}".`}
			/>
		</div>
	{:else if detailLoading}
		<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
			<Skeleton lines={6} />
		</div>
	{:else if !isNew && detailError}
		<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
			<ErrorState
				title="Template detail failed to load"
				description={detailError ?? 'Template detail failed.'}
				onretry={() => {
					if (kind) void loadDetail(kind, routeIdParam, 'preview');
				}}
				class="rounded-lg border border-border bg-card"
			/>
		</div>
	{:else}
		{#if !isNew}
			<Segmented
				items={controller.viewItems}
				value={!controller.editMode
					? 'preview'
					: controller.editTab === 'graph'
						? 'graph'
						: 'edit'}
				class="px-4"
				onchange={(id) => controller.requestView(id as DetailView)}
			/>
		{/if}
		<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
			<div class="space-y-3">
				{#if isNew}
					<TemplateCreatePanel {controller} />
				{:else if detail && !controller.editMode && kind}
					<TemplateDetailPreview {kind} {detail} {controller} />
				{:else if detail && kind}
					<TemplateEditSection {kind} {detail} {controller} />
				{:else}
					<EmptyState
						icon="template"
						title="Template not found"
						description="It may have been deleted."
						class="rounded-lg border border-border bg-card"
					/>
				{/if}
			</div>
		</div>
	{/if}
</div>

<UnsavedChangesDialog
	bind:open={controller.unsavedOpen}
	description="This template has unsaved edits. Leaving discards the JSON, form and canvas changes."
	ondiscard={() => controller.discardEditsAndLeave()}
/>
