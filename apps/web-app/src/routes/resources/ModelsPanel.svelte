<script lang="ts">
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import DataTable from '@wf-agent/ui/components/DataTable.svelte';
	import { t } from '#lib/i18n';
	import type { Column } from '@wf-agent/ui/components/table';
	import type { ModelProfile, Provider } from '#lib/types/models';
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
			text: (profile) =>
				profile.cost === null ? '—' : `$${profile.cost.toFixed(2)}`,
			cellClass: 'tabular-nums text-caption',
		},
	];
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
					<li
						class="flex flex-wrap items-center justify-between gap-2 py-2 first:pt-0"
					>
						<div class="min-w-0">
							<p class="truncate text-body">{provider.name}</p>
							<p class="truncate font-mono text-micro text-muted-foreground">
								{provider.baseUrl}
							</p>
						</div>
						<div class="flex shrink-0 items-center gap-2">
							<span class="text-caption text-muted-foreground"
								>{provider.models} models</span
							>
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
			<Badge variant="info" class="text-[0.625rem]">default</Badge>
		{/if}
	</span>
{/snippet}

{#snippet profileStatusCell(profile: ModelProfile)}
	<StatusBadge status={profile.status} size="sm" />
{/snippet}
