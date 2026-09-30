<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { page } from '$app/state';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import IconButton from '@wf-agent/ui/components/IconButton.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import Segmented from '@wf-agent/ui/components/Segmented.svelte';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import ErrorState from '@wf-agent/ui/components/ErrorState.svelte';
	import Skeleton from '@wf-agent/ui/components/Skeleton.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import TemplateImportDialog from '$lib/components/domain/TemplateImportDialog.svelte';
	import {
		cloneTemplate,
		importTemplate,
		listFeaturedTemplates,
		listTemplates,
		recordTemplateUsage,
	} from '$lib/services/templates';
	import type { Template, TemplateKind } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatNumber } from '$lib/utils/format';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';

	const TABS = [
		{ id: 'all', label: 'All' },
		{ id: 'node', label: 'Node' },
		{ id: 'trigger', label: 'Trigger' },
		{ id: 'agent', label: 'Agent' },
		{ id: 'workflow', label: 'Workflow' },
	];

	const requestedTab = parseListParams(page.url).tab;
	let kind = $state(
		requestedTab && TABS.some((item) => item.id === requestedTab)
			? requestedTab
			: 'all',
	);

	$effect(() => {
		gotoWithParams(page.url, { tab: kind === 'all' ? '' : kind });
	});

	let featuredOnly = $state(false);
	let templates = $state<Template[]>([]);
	let featuredIds = $state<Set<string>>(new Set());
	let listLoading = $state(true);
	let listError = $state<string | null>(null);

	let importOpen = $state(false);
	let importKind = $state<TemplateKind>('node');
	let importText = $state('');
	let importError = $state<string | null>(null);
	let importBusy = $state(false);

	async function reload(): Promise<void> {
		listLoading = true;
		listError = null;
		try {
			const [all, featured] = await Promise.all([
				listTemplates({ kind: 'all' }),
				listFeaturedTemplates(),
			]);
			featuredIds = new Set(featured.map((t) => t.id));
			templates = all.map((t) => ({
				...t,
				featured: t.featured || featuredIds.has(t.id),
			}));
		} catch (e) {
			listError = e instanceof Error ? e.message : 'Templates failed.';
			templates = [];
		} finally {
			listLoading = false;
		}
	}

	/** Browse-only list: preview and editing live on the detail route. */
	async function openDetail(row: {
		id: string;
		kind: TemplateKind;
	}): Promise<void> {
		await goto(
			resolve('/templates/[kind]/[id]', { kind: row.kind, id: row.id }),
		);
	}

	function startCreate(): void {
		/* eslint-disable svelte/no-navigation-without-resolve -- resolve() cannot append the query string */
		void goto(
			resolve('/templates/[kind]/[id]', { kind: 'node', id: 'new' }) +
				'?tab=edit',
		);
		/* eslint-enable svelte/no-navigation-without-resolve */
	}

	async function runClone(row: Template): Promise<void> {
		try {
			const newId = await cloneTemplate(row.id, row.kind, `${row.name} (copy)`);
			void recordTemplateUsage(row.id).catch(() => undefined);
			toasts.success('Template cloned');
			await reload();
			await goto(
				resolve('/templates/[kind]/[id]', { kind: row.kind, id: newId }),
			);
		} catch (e) {
			toasts.error('Clone failed', e instanceof Error ? e.message : undefined);
		}
	}

	async function runImport(): Promise<void> {
		importError = null;
		let parsed: unknown;
		try {
			parsed = JSON.parse(importText);
		} catch (e) {
			importError = e instanceof Error ? e.message : 'Invalid JSON';
			return;
		}
		if (!parsed || typeof parsed !== 'object') {
			importError = 'Import body must be a JSON object';
			return;
		}
		importBusy = true;
		try {
			const newId = await importTemplate(importKind, importText);
			toasts.success('Template imported');
			importOpen = false;
			importText = '';
			await reload();
			await goto(
				resolve('/templates/[kind]/[id]', { kind: importKind, id: newId }),
			);
		} catch (e) {
			importError = e instanceof Error ? e.message : 'Import failed.';
		} finally {
			importBusy = false;
		}
	}

	onMount(() => {
		void reload();
	});

	const filtered = $derived(
		templates.filter((template) => {
			const matchesKind = kind === 'all' || template.kind === kind;
			const matchesFeatured = !featuredOnly || template.featured;
			return matchesKind && matchesFeatured;
		}),
	);

	const KIND_ICON: Record<
		TemplateKind,
		'blocks' | 'zap' | 'sparkles' | 'workflow'
	> = {
		node: 'blocks',
		trigger: 'zap',
		agent: 'sparkles',
		workflow: 'workflow',
	};
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Template library"
		description="Reusable node, trigger, agent and workflow templates from the registry."
	>
		{#snippet actions()}
			<IconButton
				icon="refresh"
				label="Refresh"
				onclick={() => void reload()}
			/>
			<Button variant="outline" size="sm" onclick={() => (importOpen = true)}>
				<Icon name="upload" size={13} />
				Import
			</Button>
			<Button size="sm" onclick={startCreate}>
				<Icon name="plus" size={13} />
				New template
			</Button>
		{/snippet}
	</PageHeader>

	<Segmented items={TABS} bind:value={kind} class="px-4">
		{#snippet trailing()}
			<Button
				variant={featuredOnly ? 'secondary' : 'ghost'}
				size="sm"
				onclick={() => (featuredOnly = !featuredOnly)}
			>
				<Icon name="star" size={13} />
				Featured
			</Button>
		{/snippet}
	</Segmented>

	<div class="min-h-0 flex-1 overflow-y-auto px-4 py-3">
		{#if listLoading}
			<Skeleton lines={5} class="rounded-lg border border-border bg-card p-4" />
		{:else if listError}
			<ErrorState
				title="Templates failed to load"
				description={listError}
				onretry={() => void reload()}
				class="rounded-lg border border-border bg-card"
			/>
		{:else if filtered.length === 0}
			<EmptyState
				icon="template"
				title="No templates match"
				description="Switch the kind filter or disable the featured toggle."
				class="rounded-lg border border-border bg-card"
			/>
		{:else}
			<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
				{#each filtered as template (template.id)}
					<Card title={template.name}>
						{#snippet actions()}
							{#if template.featured}
								<Badge variant="warning" class="text-[0.625rem]">
									<Icon name="star" size={10} />
									featured
								</Badge>
							{/if}
						{/snippet}
						<p class="text-caption text-muted-foreground">
							{template.description}
						</p>
						<div class="mt-2 flex flex-wrap items-center gap-1.5">
							<span
								class="inline-flex items-center gap-1 rounded border border-border px-1.5 py-0.5 text-micro text-muted-foreground"
							>
								<Icon name={KIND_ICON[template.kind]} size={11} />
								{template.kind}
							</span>
							<span class="text-micro text-muted-foreground"
								>{template.category}</span
							>
						</div>
						<div class="mt-2 flex flex-wrap gap-1">
							{#each template.tags as tag (tag)}
								<Badge variant="outline" class="text-[0.625rem]">{tag}</Badge>
							{/each}
						</div>
						{#snippet footer()}
							<div class="flex items-center justify-between">
								<span class="tabular-nums"
									>{formatNumber(template.usage)} uses</span
								>
								<div class="flex items-center gap-1">
									<Button
										variant="ghost"
										size="sm"
										onclick={() => void runClone(template)}
									>
										Clone
									</Button>
									<Button
										variant="ghost"
										size="sm"
										onclick={() => void openDetail(template)}
									>
										Details
									</Button>
								</div>
							</div>
						{/snippet}
					</Card>
				{/each}
			</div>
		{/if}
	</div>
</div>

<TemplateImportDialog
	bind:open={importOpen}
	bind:kind={importKind}
	bind:text={importText}
	error={importError}
	busy={importBusy}
	onimport={() => void runImport()}
	ondiscard={() => (importError = null)}
/>
