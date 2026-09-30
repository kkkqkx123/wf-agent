<script lang="ts">
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Badge from '@wf-agent/ui/components/Badge.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import EmptyState from '@wf-agent/ui/components/EmptyState.svelte';
	import DataTable from '@wf-agent/ui/components/DataTable.svelte';
	import type { Column } from '@wf-agent/ui/components/table';
	import type { ModelProfile, Provider } from '$lib/types/models';
	import { formatNumber } from '$lib/utils/format';

	interface Props {
		modelProfiles: ModelProfile[];
		providers: Provider[];
	}

	let { modelProfiles, providers }: Props = $props();

	const profileColumns: Column<ModelProfile>[] = [
		{ key: 'name', header: 'Profile', cell: profileNameCell },
		{
			key: 'provider',
			header: 'Provider',
			text: (profile) => profile.provider,
			cellClass: 'text-caption text-muted-foreground',
		},
		{
			key: 'model',
			header: 'Model',
			text: (profile) => profile.model,
			cellClass: 'font-mono text-caption',
		},
		{ key: 'status', header: 'Status', cell: profileStatusCell },
		{
			key: 'requests',
			header: 'Requests',
			align: 'right',
			text: (profile) => formatNumber(profile.requests),
			cellClass: 'tabular-nums text-caption',
		},
		{
			key: 'tokens',
			header: 'Tokens',
			align: 'right',
			text: (profile) => formatNumber(profile.tokens),
			cellClass: 'tabular-nums text-caption text-muted-foreground',
		},
		{
			key: 'cost',
			header: 'Cost',
			align: 'right',
			text: (profile) =>
				profile.cost === null ? '—' : `$${profile.cost.toFixed(2)}`,
			cellClass: 'tabular-nums text-caption',
		},
	];
</script>

<div class="space-y-3">
	<Card title="Model profiles" bodyClass="p-0">
		<DataTable
			columns={profileColumns}
			rows={modelProfiles}
			rowKey={(profile) => profile.id}
			emptyTitle="No model profiles"
			emptyDescription="Profiles appear here once a provider exposes them."
			virtualize={false}
		/>
	</Card>

	<Card title="Providers">
		{#if providers.length === 0}
			<EmptyState
				icon="database"
				title="No providers configured"
				description="Providers appear here once a model backend is connected."
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
