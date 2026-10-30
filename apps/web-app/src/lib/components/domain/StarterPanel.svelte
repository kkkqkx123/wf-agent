<script lang="ts">
	import Badge from '#lib/ui/components/Badge.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import Card from '#lib/ui/components/Card.svelte';
	import EmptyState from '#lib/ui/components/EmptyState.svelte';
	import ErrorState from '#lib/ui/components/ErrorState.svelte';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Input from '#lib/ui/components/Input.svelte';
	import Select from '#lib/ui/components/Select.svelte';
	import Skeleton from '#lib/ui/components/Skeleton.svelte';
	import Dialog from '#lib/ui/components/Dialog.svelte';
	import JsonEditor from '#lib/ui/components/JsonEditor.svelte';
	import Switch from '#lib/ui/components/Switch.svelte';
	import StarterEditDialog from '#lib/components/domain/StarterEditDialog.svelte';
	import {
		createStarter,
		deleteStarter,
		exportStarter,
		importStarter,
		listStarters,
		setStarterEnabled,
		updateStarter,
		type ChatStarter,
		type StarterInput,
	} from '#lib/services/starters';
	import { toasts } from '#lib/stores/toast.svelte';
	import { formatNumber } from '#lib/utils/format';
	import { t } from '#lib/i18n';

	let starters = $state<ChatStarter[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	let query = $state('');
	let localeFilter = $state('all');

	let editOpen = $state(false);
	let editing = $state<ChatStarter | null>(null);
	let editBusy = $state(false);
	let editError = $state<string | null>(null);

	let importOpen = $state(false);
	let importText = $state('');
	let importError = $state<string | null>(null);
	let importBusy = $state(false);

	let deleteArmed = $state<string | null>(null);
	let rowBusy = $state<string | null>(null);

	async function reload(): Promise<void> {
		loading = true;
		error = null;
		try {
			// Admin view: every locale, disabled rows included.
			starters = await listStarters({ includeDisabled: true });
		} catch (e) {
			error = e instanceof Error ? e.message : t('page.starters.loadFailed');
			starters = [];
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void reload();
	});

	const filtered = $derived(
		starters.filter((starter) => {
			if (localeFilter !== 'all' && starter.locale !== localeFilter) return false;
			const q = query.trim().toLowerCase();
			if (q === '') return true;
			return (
				starter.title.toLowerCase().includes(q) ||
				starter.content.toLowerCase().includes(q) ||
				starter.tags.some((tag) => tag.toLowerCase().includes(q))
			);
		}),
	);

	function startCreate(): void {
		editing = null;
		editError = null;
		editOpen = true;
	}

	function startEdit(starter: ChatStarter): void {
		editing = starter;
		editError = null;
		editOpen = true;
	}

	async function saveStarter(input: StarterInput): Promise<void> {
		editBusy = true;
		editError = null;
		try {
			if (editing) await updateStarter(editing.id, input);
			else await createStarter(input);
			toasts.success(t('page.starters.saved'));
			editOpen = false;
			await reload();
		} catch (e) {
			editError = e instanceof Error ? e.message : t('page.starters.saveFailed');
		} finally {
			editBusy = false;
		}
	}

	async function toggleEnabled(starter: ChatStarter, enabled: boolean): Promise<void> {
		rowBusy = starter.id;
		try {
			await setStarterEnabled(starter.id, enabled);
			await reload();
		} catch (e) {
			toasts.error(t('page.starters.toggleFailed'), e instanceof Error ? e.message : undefined);
		} finally {
			rowBusy = null;
		}
	}

	async function removeStarter(starter: ChatStarter): Promise<void> {
		if (deleteArmed !== starter.id) {
			deleteArmed = starter.id;
			return;
		}
		deleteArmed = null;
		rowBusy = starter.id;
		try {
			await deleteStarter(starter.id);
			toasts.success(t('page.starters.deleted'));
			await reload();
		} catch (e) {
			toasts.error(t('page.starters.deleteFailed'), e instanceof Error ? e.message : undefined);
		} finally {
			rowBusy = null;
		}
	}

	async function runExport(starter: ChatStarter): Promise<void> {
		try {
			const json = await exportStarter(starter.id);
			const blob = new Blob([json], { type: 'application/json' });
			const url = URL.createObjectURL(blob);
			const anchor = document.createElement('a');
			anchor.href = url;
			anchor.download = `chat-starter-${starter.id}.json`;
			anchor.click();
			URL.revokeObjectURL(url);
		} catch (e) {
			toasts.error(t('page.starters.exportFailed'), e instanceof Error ? e.message : undefined);
		}
	}

	async function runImport(): Promise<void> {
		importError = null;
		if (importText.trim() === '') {
			importError = t('page.starters.importEmpty');
			return;
		}
		importBusy = true;
		try {
			await importStarter(importText);
			toasts.success(t('page.starters.imported'));
			importOpen = false;
			importText = '';
			await reload();
		} catch (e) {
			importError = e instanceof Error ? e.message : t('page.starters.importFailed');
		} finally {
			importBusy = false;
		}
	}
</script>

<div class="flex min-h-0 flex-1 flex-col">
	<div class="flex flex-wrap items-center gap-2 px-4 py-3">
		<Input
			bind:value={query}
			size="sm"
			placeholder={t('page.starters.searchPlaceholder')}
			class="w-56"
		/>
		<Select
			bind:value={localeFilter}
			size="sm"
			class="w-32"
			options={[
				{ value: 'all', label: t('page.starters.localeAll') },
				{ value: 'en', label: 'English' },
				{ value: 'zh', label: '中文' },
			]}
		/>
		<div class="ml-auto flex items-center gap-2">
			<IconButton icon="refresh" label={t('common.refresh')} onclick={() => void reload()} />
			<Button variant="outline" size="sm" onclick={() => (importOpen = true)}>
				<Icon name="upload" size={13} />
				{t('page.templates.import')}
			</Button>
			<Button size="sm" onclick={startCreate}>
				<Icon name="plus" size={13} />
				{t('page.starters.newStarter')}
			</Button>
		</div>
	</div>

	<div class="min-h-0 flex-1 overflow-y-auto px-4 pb-3">
		{#if loading}
			<Skeleton lines={5} class="rounded-lg border border-border bg-card p-4" />
		{:else if error}
			<ErrorState
				title={t('page.starters.loadFailed')}
				description={error}
				onretry={() => void reload()}
				class="rounded-lg border border-border bg-card"
			/>
		{:else if filtered.length === 0}
			<EmptyState
				icon="sparkles"
				title={t('page.starters.noMatch')}
				description={t('page.starters.noMatchDesc')}
				class="rounded-lg border border-border bg-card"
			/>
		{:else}
			<div class="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
				{#each filtered as starter (starter.id)}
					<Card title={starter.title}>
						{#snippet actions()}
							{#if starter.isSystem}
								<Badge variant="warning" class="text-[0.625rem]">
									{t('page.starters.builtin')}
								</Badge>
							{/if}
							<Switch
								checked={starter.enabled}
								label={t('page.starters.enable', { name: starter.title })}
								hideLabel
								disabled={rowBusy === starter.id}
								onchange={(checked) => void toggleEnabled(starter, checked)}
							/>
						{/snippet}
						<p class="line-clamp-3 text-caption text-muted-foreground">
							{starter.content}
						</p>
						<div class="mt-2 flex flex-wrap items-center gap-1.5">
							<span
								class="inline-flex items-center gap-1 rounded border border-border px-1.5 py-0.5 font-mono text-micro text-muted-foreground"
							>
								{starter.locale}
							</span>
							{#if starter.isDefault}
								<Badge variant="outline" class="text-[0.625rem]">
									{t('page.starters.defaultBadge')}
								</Badge>
							{/if}
							{#each starter.tags as tag (tag)}
								<Badge variant="outline" class="text-[0.625rem]">{tag}</Badge>
							{/each}
						</div>
						{#snippet footer()}
							<div class="flex items-center justify-between">
								<span class="tabular-nums text-micro text-muted-foreground">
									{formatNumber(starter.usageCount)}
									{t('page.starters.uses')}
								</span>
								<div class="flex items-center gap-1">
									<Button variant="ghost" size="sm" onclick={() => void runExport(starter)}>
										{t('page.starters.export')}
									</Button>
									{#if !starter.isSystem}
										<Button variant="ghost" size="sm" onclick={() => startEdit(starter)}>
											{t('page.starters.edit')}
										</Button>
										<Button
											variant={deleteArmed === starter.id ? 'destructive' : 'ghost'}
											size="sm"
											disabled={rowBusy === starter.id}
											onclick={() => void removeStarter(starter)}
										>
											{deleteArmed === starter.id
												? t('page.starters.confirmDelete')
												: t('common.delete')}
										</Button>
									{/if}
								</div>
							</div>
						{/snippet}
					</Card>
				{/each}
			</div>
		{/if}
	</div>
</div>

<StarterEditDialog
	bind:open={editOpen}
	starter={editing}
	busy={editBusy}
	error={editError}
	onsave={(input) => void saveStarter(input)}
/>

<Dialog
	bind:open={importOpen}
	title={t('page.starters.importTitle')}
	description={t('page.starters.importDesc')}
>
	<JsonEditor bind:value={importText} label={t('page.starters.importLabel')} minHeight="12rem" />
	{#if importError}
		<p class="mt-2 text-caption text-destructive">{importError}</p>
	{/if}
	{#snippet footer()}
		<div class="flex items-center justify-end gap-2">
			<Button variant="ghost" size="sm" onclick={() => (importOpen = false)}>
				{t('common.cancel')}
			</Button>
			<Button size="sm" disabled={importBusy} onclick={() => void runImport()}>
				{t('page.templates.import')}
			</Button>
		</div>
	{/snippet}
</Dialog>
