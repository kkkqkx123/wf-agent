<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import { listTemplates } from '$lib/services/templates';
	import type { Template, TemplateKind } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { formatNumber } from '$lib/utils/format';

	const TABS = [
		{ id: 'all', label: 'All' },
		{ id: 'node', label: 'Node' },
		{ id: 'trigger', label: 'Trigger' },
		{ id: 'agent', label: 'Agent' },
		{ id: 'workflow', label: 'Workflow' },
	];

	let kind = $state<'all' | TemplateKind>('all');
	let featuredOnly = $state(false);

	let loading = $state(true);
	let error = $state<string | null>(null);
	let templates = $state<Template[]>([]);

	async function load() {
		loading = true;
		error = null;
		try {
			templates = await listTemplates({
				kind: kind === 'all' ? undefined : kind,
			});
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unknown error';
		} finally {
			loading = false;
		}
	}

	// Reload on kind change — debounced by user interaction
	$effect(() => {
		// Skip on first run (onMount handles initial load)
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

	onMount(load);
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
				onclick={load}
				disabled={loading}
			/>
			<Button
				variant="outline"
				size="sm"
				onclick={() => toasts.info('Import pending')}
				disabled
			>
				<Icon name="upload" size={13} />
				Import
			</Button>
			<Button
				size="sm"
				onclick={() => toasts.success('Template editor pending')}
				disabled
			>
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
		{#if loading}
			<div class="flex h-full items-center justify-center text-muted-foreground">
				Loading templates…
			</div>
		{:else if error}
			<div class="flex h-full flex-col items-center justify-center gap-3">
				<p class="text-destructive">{error}</p>
				<Button variant="outline" size="sm" onclick={load}>Retry</Button>
			</div>
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
										onclick={() => toasts.info('Clone pending')}
									>
										Clone
									</Button>
									<Button
										variant="ghost"
										size="sm"
										onclick={() => toasts.success('Template applied')}
									>
										Use
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
