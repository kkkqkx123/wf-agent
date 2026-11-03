<script lang="ts">
	import Card from '#lib/ui/components/Card.svelte';
	import Badge from '#lib/ui/components/Badge.svelte';
	import StatusBadge from '#lib/ui/components/StatusBadge.svelte';
	import EmptyState from '#lib/ui/components/EmptyState.svelte';
	import DataTable from '#lib/ui/components/DataTable.svelte';
	import { t } from '#lib/i18n';
	import type { Column } from '#lib/ui/components/table';
	import type { ModelProfile, Provider } from '#lib/types/models';
	import type { ProviderModel } from '#lib/services/resources';
	import { listProviderModels } from '#lib/services/resources';
	import { formatNumber } from '#lib/utils/format';

	interface Props {
		modelProfiles: ModelProfile[];
		providers: Provider[];
	}

	let { modelProfiles, providers }: Props = $props();

	const profileColumns: Column<ModelProfile>[] = [
		{
			key: 'name',
			header: t('page.resources.models.columnProfile'),
			cell: profileNameCell,
		},
		{
			key: 'provider',
			header: t('page.resources.models.columnProvider'),
			text: (profile) => profile.provider,
			cellClass: 'text-caption text-muted-foreground',
		},
		{
			key: 'model',
			header: t('page.resources.models.columnModel'),
			text: (profile) => profile.model,
			cellClass: 'font-mono text-caption',
		},
		{
			key: 'status',
			header: t('page.resources.models.columnStatus'),
			cell: profileStatusCell,
		},
		{
			key: 'requests',
			header: t('page.resources.models.columnRequests'),
			align: 'right',
			text: (profile) => formatNumber(profile.requests),
			cellClass: 'tabular-nums text-caption',
		},
		{
			key: 'tokens',
			header: t('page.resources.models.columnTokens'),
			align: 'right',
			text: (profile) => formatNumber(profile.tokens),
			cellClass: 'tabular-nums text-caption text-muted-foreground',
		},
		{
			key: 'cost',
			header: t('page.resources.models.columnCost'),
			align: 'right',
			text: (profile) => (profile.cost === null ? '—' : `$${profile.cost.toFixed(2)}`),
			cellClass: 'tabular-nums text-caption',
		},
	];

	const modelColumns: Column<ProviderModel>[] = [
		{
			key: 'id',
			header: t('page.resources.models.columnModel'),
			text: (model) => model.id,
			cellClass: 'font-mono text-caption',
		},
		{
			key: 'name',
			header: t('page.resources.models.columnProfile'),
			text: (model) => model.name ?? '—',
			cellClass: 'text-caption',
		},
		{
			key: 'context_window_size',
			header: t('page.resources.models.columnContextWindow'),
			align: 'right',
			text: (model) =>
				model.context_window_size === null || model.context_window_size === undefined
					? '—'
					: formatNumber(model.context_window_size),
			cellClass: 'tabular-nums text-caption text-muted-foreground',
		},
		{
			key: 'max_output_tokens',
			header: t('page.resources.models.columnMaxOutput'),
			align: 'right',
			text: (model) =>
				model.max_output_tokens === null || model.max_output_tokens === undefined
					? '—'
					: formatNumber(model.max_output_tokens),
			cellClass: 'tabular-nums text-caption text-muted-foreground',
		},
	];

	let selectedProviderId = $state<string>('');
	let discoveredModels = $state<ProviderModel[]>([]);

	$effect(() => {
		if (providers.length > 0 && !providers.some((provider) => provider.id === selectedProviderId)) {
			selectedProviderId = providers[0].id;
		}
	});
	let loadingModels = $state(false);
	let modelsError = $state<string | null>(null);
	let refreshing = $state(false);
	let selectedProvider = $derived(providers.find((p) => p.id === selectedProviderId));
	let discoveryUnsupported = $derived(
		selectedProvider !== undefined &&
			(selectedProvider.discoveryKind === 'disabled' ||
				selectedProvider.discoveryKind === 'unknown'),
	);
	let missingBaseUrl = $derived(
		selectedProvider !== undefined &&
			!discoveryUnsupported &&
			selectedProvider.baseUrl.trim().length === 0,
	);

	async function loadModels(refresh = false): Promise<void> {
		if (!selectedProviderId) {
			discoveredModels = [];
			return;
		}
		loadingModels = true;
		modelsError = null;
		try {
			discoveredModels = await listProviderModels(selectedProviderId, { refresh });
		} catch (error) {
			modelsError = error instanceof Error ? error.message : t('page.resources.loadFailed');
			discoveredModels = [];
		} finally {
			loadingModels = false;
			refreshing = false;
		}
	}

	async function refreshModels(): Promise<void> {
		refreshing = true;
		await loadModels(true);
	}

	$effect(() => {
		void selectedProviderId;
		void loadModels(false);
	});
</script>

<div class="space-y-3">
	<Card title={t('page.resources.models.title')} bodyClass="p-0">
		<DataTable
			columns={profileColumns}
			rows={modelProfiles}
			rowKey={(profile) => profile.id}
			emptyTitle={t('page.resources.models.noProfiles')}
			emptyDescription={t('page.resources.models.noProfilesDesc')}
			virtualize={false}
		/>
	</Card>

	<Card title={t('page.resources.models.discovered')}>
		{#if providers.length === 0}
			<EmptyState
				icon="database"
				title={t('page.resources.models.noProviders')}
				description={t('page.resources.models.noProvidersDesc')}
				class="py-6"
			/>
		{:else}
			<div class="mb-3 flex flex-wrap items-center gap-2">
				<label class="text-caption text-muted-foreground" for="model-provider-select">
					{t('page.resources.models.selectProvider')}
				</label>
				<select
					id="model-provider-select"
					class="rounded-md border border-border bg-background px-2 py-1 text-caption"
					bind:value={selectedProviderId}
				>
					{#each providers as provider (provider.id)}
						<option value={provider.id}>{provider.name}</option>
					{/each}
				</select>
				<button
					type="button"
					class="rounded-md border border-border px-2 py-1 text-caption hover:bg-muted disabled:opacity-50"
					disabled={loadingModels || refreshing}
					onclick={refreshModels}
				>
					{refreshing ? '…' : t('page.resources.models.refresh')}
				</button>
			</div>

			{#if discoveryUnsupported}
				<p class="text-caption text-muted-foreground">
					{t('page.resources.models.discoveryDisabled')}
				</p>
			{:else if missingBaseUrl}
				<p class="text-caption text-muted-foreground">
					{t('page.resources.models.missingBaseUrl')}
				</p>
			{:else if loadingModels}
				<p class="text-caption text-muted-foreground">
					{t('page.resources.models.noDiscoveredDesc')}
				</p>
			{:else if modelsError}
				<p class="text-caption text-destructive">{modelsError}</p>
			{:else if discoveredModels.length === 0}
				<EmptyState
					icon="database"
					title={t('page.resources.models.noDiscovered')}
					description={t('page.resources.models.noDiscoveredDesc')}
					class="py-6"
				/>
			{:else}
				<DataTable
					columns={modelColumns}
					rows={discoveredModels}
					rowKey={(model) => model.id}
					virtualize={false}
				/>
			{/if}
		{/if}
	</Card>

	<Card title={t('page.resources.models.providers')}>
		{#if providers.length === 0}
			<EmptyState
				icon="database"
				title={t('page.resources.models.noProviders')}
				description={t('page.resources.models.noProvidersDesc')}
				class="py-6"
			/>
		{:else}
			<ul class="divide-y divide-border">
				{#each providers as provider (provider.id)}
					<li class="flex flex-wrap items-center justify-between gap-2 py-2 first:pt-0">
						<div class="min-w-0">
							<p class="truncate text-body">{provider.name}</p>
							<p class="truncate font-mono text-micro text-muted-foreground">
								{provider.baseUrl}
							</p>
						</div>
						<div class="flex shrink-0 items-center gap-2">
							<span class="text-caption text-muted-foreground">{provider.models} models</span>
							<StatusBadge status={provider.status} size="sm" />
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</Card>
</div>

{#snippet profileNameCell(profile: ModelProfile)}
	<span class="flex items-center gap-1.5">
		<span class="truncate">{profile.name}</span>
		{#if profile.isDefault}
			<Badge variant="info">default</Badge>
		{/if}
	</span>
{/snippet}

{#snippet profileStatusCell(profile: ModelProfile)}
	<StatusBadge status={profile.status} size="sm" />
{/snippet}
