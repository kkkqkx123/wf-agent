<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import IconButton from '$lib/components/ui/IconButton.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Segmented from '$lib/components/ui/Segmented.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ErrorState from '$lib/components/ui/ErrorState.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import Dialog from '$lib/components/ui/Dialog.svelte';
	import Sheet from '$lib/components/ui/Sheet.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import Textarea from '$lib/components/ui/Textarea.svelte';
	import StatusBadge from '$lib/components/domain/StatusBadge.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import {
		cloneTemplate,
		deleteTemplate,
		exportTemplate,
		getTemplateDetail,
		importTemplate,
		listFeaturedTemplates,
		listTemplates,
		saveTemplate,
		validateTemplateDefinition,
		type TemplateDetail,
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

	let drawerOpen = $state(false);
	let drawerKind = $state<TemplateKind>('node');
	let drawerId = $state<string | null>(null);
	let detail = $state<TemplateDetail | null>(null);
	let detailLoading = $state(false);
	let detailError = $state<string | null>(null);

	let editMode = $state(false);
	let editText = $state('');
	let syntaxError = $state<string | null>(null);
	let validationState = $state<'idle' | 'valid' | 'invalid'>('idle');
	let validationIssues = $state<string[]>([]);
	let saveBusy = $state(false);
	let deleteArmed = $state(false);

	let createName = $state('');
	let createId = $state('');

	let importOpen = $state(false);
	let importKind = $state<'node' | 'trigger'>('node');
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

	async function openDetail(row: { id: string; kind: TemplateKind }): Promise<void> {
		drawerKind = row.kind;
		drawerId = row.id;
		editMode = false;
		detail = null;
		detailError = null;
		deleteArmed = false;
		validationState = 'idle';
		validationIssues = [];
		syntaxError = null;
		drawerOpen = true;
		await loadDetail(row.kind, row.id);
	}

	async function loadDetail(kind: TemplateKind, id: string): Promise<void> {
		detailLoading = true;
		detailError = null;
		try {
			detail = await getTemplateDetail(id, kind);
		} catch (e) {
			detailError =
				e instanceof Error ? e.message : 'Template detail failed.';
			detail = null;
		} finally {
			detailLoading = false;
		}
	}

	function startCreate(): void {
		drawerKind = 'node';
		drawerId = null;
		detail = null;
		editMode = true;
		createName = '';
		createId = '';
		editText = '';
		syntaxError = null;
		validationState = 'idle';
		validationIssues = [];
		deleteArmed = false;
		drawerOpen = true;
	}

	function parseEditText(): { value: unknown; error: string | null } {
		try {
			return { value: JSON.parse(editText), error: null };
		} catch (e) {
			return {
				value: null,
				error: e instanceof Error ? e.message : 'Invalid JSON',
			};
		}
	}

	function editedPayload(parsed: unknown): unknown {
		if (drawerKind === 'node' || drawerKind === 'trigger') return parsed;
		const base =
			detail?.raw && typeof detail.raw === 'object'
				? (detail.raw as Record<string, unknown>)
				: {};
		return { ...base, definition: parsed };
	}

	async function runValidate(): Promise<boolean> {
		const { value, error } = parseEditText();
		syntaxError = error;
		if (error || value === undefined) {
			validationState = 'invalid';
			validationIssues = [];
			return false;
		}
		const target =
			drawerKind === 'node' || drawerKind === 'trigger'
				? value
				: (value as Record<string, unknown>)?.definition ?? value;
		const issues = await validateTemplateDefinition(drawerKind, target);
		validationIssues = issues;
		validationState = issues.length === 0 ? 'valid' : 'invalid';
		return issues.length === 0;
	}

	async function runSave(): Promise<void> {
		const valid = await runValidate();
		if (!valid) {
			toasts.error('Template invalid', 'Fix the reported issues first.');
			return;
		}
		const { value } = parseEditText();
		saveBusy = true;
		try {
			if (drawerId === null) {
				const payload = value as Record<string, unknown>;
				const id = createId.trim() || slugify(createName);
				if (!id) throw new Error('Template id is required');
				const savedId = await saveTemplate(drawerKind, null, {
					...payload,
					id,
					name: createName.trim() || id,
				});
				toasts.success('Template created');
				await reload();
				await openDetail({ id: savedId || id, kind: drawerKind });
			} else {
				await saveTemplate(drawerKind, drawerId, editedPayload(value));
				toasts.success('Template saved');
				editMode = false;
				await reload();
				await loadDetail(drawerKind, drawerId);
			}
		} catch (e) {
			toasts.error(
				'Save failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			saveBusy = false;
		}
	}

	function slugify(name: string): string {
		return name
			.trim()
			.toLowerCase()
			.replace(/[^a-z0-9]+/g, '-')
			.replace(/^-+|-+$/g, '');
	}

	async function runClone(row: Template): Promise<void> {
		try {
			const newId = await cloneTemplate(row.id, row.kind, `${row.name} (copy)`);
			toasts.success('Template cloned');
			await reload();
			await openDetail({ ...row, id: newId });
		} catch (e) {
			toasts.error(
				'Clone failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function runExport(): Promise<void> {
		if (!drawerId) return;
		try {
			await exportTemplate(drawerKind, drawerId, detail?.raw);
			toasts.success('Template exported');
		} catch (e) {
			toasts.error(
				'Export failed',
				e instanceof Error ? e.message : undefined,
			);
		}
	}

	async function runDelete(): Promise<void> {
		if (!drawerId) return;
		try {
			await deleteTemplate(drawerKind, drawerId);
			toasts.success('Template deleted');
			drawerOpen = false;
			await reload();
		} catch (e) {
			toasts.error(
				'Delete failed',
				e instanceof Error ? e.message : undefined,
			);
		} finally {
			deleteArmed = false;
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
			await openDetail({ id: newId, kind: importKind });
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

	const kindOptions = [
		{ value: 'node', label: 'Node' },
		{ value: 'trigger', label: 'Trigger' },
		{ value: 'agent', label: 'Agent' },
		{ value: 'workflow', label: 'Workflow' },
	];
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

<Sheet
	bind:open={drawerOpen}
	title={drawerId === null
		? 'New template'
		: (detail?.template.name ?? 'Template detail')}
	width="32rem"
>
	{#if drawerId !== null && detailLoading}
		<Skeleton lines={6} />
	{:else if drawerId !== null && detailError}
		<ErrorState
			title="Template detail failed to load"
			description={detailError}
			onretry={() => {
				if (drawerId) void loadDetail(drawerKind, drawerId);
			}}
		/>
	{:else}
		<div class="space-y-3">
			{#if drawerId === null}
				<div class="grid grid-cols-2 gap-2">
					<Select
						value={drawerKind}
						options={kindOptions}
						size="sm"
						placeholder="Kind"
						onchange={(value) => (drawerKind = value as TemplateKind)}
					/>
					<input
						bind:value={createId}
						placeholder="Template id"
						class="h-7 rounded-md border border-input bg-card px-2.5 text-small"
					/>
				</div>
				<input
					bind:value={createName}
					placeholder="Template name"
					class="h-8 w-full rounded-md border border-input bg-card px-2.5 text-body"
				/>
			{:else if detail}
				<div class="flex flex-wrap items-center gap-1.5 text-caption">
					<StatusBadge status="active" size="sm" dot={false} />
					<span class="text-muted-foreground">{detail.template.kind}</span>
					<span class="text-muted-foreground">·</span>
					<span class="text-muted-foreground">
						version {detail.version ?? '—'}
					</span>
					<span class="text-muted-foreground">·</span>
					<span class="text-muted-foreground">
						{formatNumber(detail.template.usage)} uses
					</span>
				</div>
				{#if detail.template.description}
					<p class="text-caption text-muted-foreground">
						{detail.template.description}
					</p>
				{/if}
			{/if}

			{#if editMode || drawerId === null}
				<div class="flex items-center justify-between">
					<span class="text-caption font-medium">Definition JSON</span>
					{#if validationState === 'valid'}
						<Badge variant="success">Valid</Badge>
					{:else if validationState === 'invalid'}
						<Badge variant="danger">Invalid</Badge>
					{/if}
				</div>
				<Textarea
					bind:value={editText}
					placeholder={'{\n  "name": "my-template",\n  …\n}'}
					class="min-h-64 font-mono text-small"
				/>
				{#if syntaxError}
					<p class="rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5 text-caption text-destructive">
						JSON syntax: {syntaxError}
					</p>
				{/if}
				{#if validationIssues.length > 0}
					<ul class="space-y-1 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5">
						{#each validationIssues as issue, index (index)}
							<li class="text-caption text-destructive">{issue}</li>
						{/each}
					</ul>
				{/if}
				<div class="flex items-center gap-2">
					<Button
						variant="outline"
						size="sm"
						onclick={() => void runValidate()}
					>
						Validate
					</Button>
					<Button size="sm" disabled={saveBusy} onclick={() => void runSave()}>
						{saveBusy ? 'Saving…' : drawerId === null ? 'Create' : 'Save'}
					</Button>
					{#if drawerId !== null}
						<Button
							variant="ghost"
							size="sm"
							onclick={() => {
								editMode = false;
								syntaxError = null;
								validationState = 'idle';
								validationIssues = [];
							}}
						>
							Cancel
						</Button>
					{/if}
				</div>
			{:else if detail}
				{@const current = detail}
				<div>
					<span class="text-caption font-medium">Definition preview</span>
					<pre
						class="mt-1 max-h-72 overflow-auto rounded-md border border-border bg-muted/40 p-2 font-mono text-micro">{current.definitionJson}</pre>
				</div>
				<div class="flex flex-wrap items-center gap-2">
					<Button
						variant="outline"
						size="sm"
						onclick={() => {
							editMode = true;
							editText =
								drawerKind === 'node' || drawerKind === 'trigger'
									? JSON.stringify(current.raw, null, 2)
									: current.definitionJson;
							syntaxError = null;
							validationState = 'idle';
							validationIssues = [];
						}}
					>
						<Icon name="pencil" size={13} />
						Edit
					</Button>
					<Button variant="outline" size="sm" onclick={() => void runExport()}>
						<Icon name="download" size={13} />
						Export
					</Button>
					{#if deleteArmed}
						<Button size="sm" onclick={() => void runDelete()}>
							Confirm delete
						</Button>
						<Button
							variant="ghost"
							size="sm"
							onclick={() => (deleteArmed = false)}
						>
							Keep
						</Button>
					{:else}
						<Button
							variant="ghost"
							size="sm"
							onclick={() => (deleteArmed = true)}
						>
							Delete
						</Button>
					{/if}
				</div>
			{/if}
		</div>
	{/if}
	{#snippet footer()}
		{#if detail}
			{@const snapshot = detail}
			<Button
				variant="outline"
				size="sm"
				onclick={() => {
					drawerOpen = false;
					void runClone(snapshot.template);
				}}
			>
				Clone as new
			</Button>
		{/if}
	{/snippet}
</Sheet>

<Dialog
	bind:open={importOpen}
	title="Import template"
	description="Node and trigger templates import from JSON text."
>
	<Select
		value={importKind}
		options={[
			{ value: 'node', label: 'Node' },
			{ value: 'trigger', label: 'Trigger' },
		]}
		size="sm"
		placeholder="Kind"
		class="mb-2 w-40"
		onchange={(value) => (importKind = value as 'node' | 'trigger')}
	/>
	<Textarea
		bind:value={importText}
		placeholder={'{\n  "id": "my-template",\n  …\n}'}
		class="min-h-48 font-mono text-small"
	/>
	{#if importError}
		<p class="mt-2 rounded-md border border-destructive/40 bg-destructive/10 px-2 py-1.5 text-caption text-destructive">
			{importError}
		</p>
	{/if}
	{#snippet footer()}
		<div class="flex items-center justify-end gap-2">
			<Button variant="ghost" size="sm" onclick={() => (importOpen = false)}>
				Cancel
			</Button>
			<Button size="sm" disabled={importBusy} onclick={() => void runImport()}>
				{importBusy ? 'Importing…' : 'Import'}
			</Button>
		</div>
	{/snippet}
</Dialog>
