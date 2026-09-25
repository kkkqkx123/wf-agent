<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import Input from '$lib/components/ui/Input.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import {
		cloneTemplate,
		listFeaturedTemplates,
		listTemplates,
	} from '$lib/services/templates';
	import type { Template, TemplateKind } from '$lib/types/models';
	import { createResource } from '$lib/stores/collection.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatNumber } from '$lib/utils/format';
	import { gotoWithParams, parseListParams } from '$lib/utils/route';
	import { page } from '$app/state';

	const TABS = [
		{ id: 'all', label: 'All' },
		{ id: 'node', label: 'Node' },
		{ id: 'trigger', label: 'Trigger' },
		{ id: 'agent', label: 'Agent' },
		{ id: 'workflow', label: 'Workflow' },
	];

	const initial = parseListParams(page.url);
	let kind = $state<'all' | TemplateKind>(
		(initial.tab as TemplateKind | undefined) ?? 'all',
	);
	let featuredOnly = $state(false);

	const registry = createResource<Template[]>(async () => {
		if (!featuredOnly) return listTemplates({ kind });
		const featured = await listFeaturedTemplates();
		return kind === 'all' ? featured : featured.filter((t) => t.kind === kind);
	});

	$effect(() => {
		gotoWithParams(page.url, { tab: kind === 'all' ? '' : kind });
	});

	// Both filters are applied by the endpoint or right after it, so every
	// change refetches.
	$effect(() => {
		void kind;
		void featuredOnly;
		void registry.reload();
	});

	const templates = $derived(registry.data ?? []);

	const KIND_ICON: Record<
		TemplateKind,
		'blocks' | 'zap' | 'sparkles' | 'workflow'
	> = {
		node: 'blocks',
		trigger: 'zap',
		agent: 'sparkles',
		workflow: 'workflow',
	};

	let cloneTarget = $state<Template | null>(null);
	let cloneOpen = $state(false);
	let cloneName = $state('');
	let cloning = $state(false);

	function openClone(template: Template): void {
		cloneTarget = template;
		cloneName = `${template.name} copy`;
		cloneOpen = true;
	}

	async function submitClone(): Promise<void> {
		const target = cloneTarget;
		if (!target) return;
		cloning = true;
		try {
			await cloneTemplate(target.id, target.kind, cloneName.trim());
			await registry.reload();
			cloneOpen = false;
			toasts.success(`Cloned ${target.name}`);
		} catch (e) {
			toasts.error(e instanceof Error ? e.message : 'Clone failed');
		} finally {
			cloning = false;
		}
	}
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
				onclick={() => void registry.reload()}
				disabled={registry.loading}
			/>
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
		{#if registry.loading && !registry.data}
			<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
				{#each Array.from({ length: 6 }, (_, position) => position) as index (index)}
					<Skeleton class="h-[168px] rounded-lg" />
				{/each}
			</div>
		{:else if registry.error}
			<ErrorState
				title="Failed to load templates"
				description={registry.error}
				onretry={() => void registry.reload()}
				class="rounded-lg border border-border bg-card"
			/>
		{:else if templates.length === 0}
			<EmptyState
				icon="template"
				title="No templates match"
				description="Switch the kind filter or disable the featured toggle."
				class="rounded-lg border border-border bg-card"
			/>
		{:else}
			<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
				{#each templates as template (template.id)}
					<Card title={template.name}>
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
								<Badge variant="outline" size="sm">{tag}</Badge>
							{/each}
						</div>
						{#snippet footer()}
							<div class="flex items-center justify-between">
								<span class="tabular-nums"
									>{formatNumber(template.usage)} uses</span
								>
								{#if template.kind === 'workflow' || template.kind === 'agent'}
									<Button
										variant="ghost"
										size="sm"
										onclick={() => openClone(template)}
									>
										Clone
									</Button>
								{/if}
							</div>
						{/snippet}
					</Card>
				{/each}
			</div>
		{/if}
	</div>
</div>

{#if cloneTarget}
	<Dialog
		bind:open={cloneOpen}
		title="Clone {cloneTarget.name}"
		description="The registry stores the copy as a new editable template."
	>
		<label class="text-caption text-muted-foreground" for="clone-name">
			New name
		</label>
		<Input id="clone-name" bind:value={cloneName} class="mt-1" />
		{#snippet footer()}
			<Button variant="ghost" size="sm" onclick={() => (cloneOpen = false)}
				>Cancel</Button
			>
			<Button
				size="sm"
				disabled={cloning || cloneName.trim() === ''}
				onclick={() => void submitClone()}
			>
				Clone
			</Button>
		{/snippet}
	</Dialog>
{/if}
