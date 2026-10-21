<script lang="ts">
	import { SvelteSet } from 'svelte/reactivity';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import Input from '@wf-agent/ui/components/Input.svelte';
	import Select from '@wf-agent/ui/components/Select.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import type { Approval } from '#lib/types/models';
	import { formatDateTime } from '#lib/utils/format';
	import { t } from '#lib/i18n';

	let {
		approvals,
		progress = null,
		ondecide,
		onbatch,
	}: {
		approvals: Approval[];
		progress?: string | null;
		ondecide: (approval: Approval, granted: boolean) => void;
		onbatch: (approvals: Approval[], granted: boolean) => void;
	} = $props();

	const statusOptions = $derived([
		{ value: 'all', label: t('domain.approvals.allStatuses') },
		{ value: 'pending', label: t('domain.approvals.pending') },
		{ value: 'approved', label: t('domain.approvals.approved') },
		{ value: 'rejected', label: t('domain.approvals.rejected') },
	]);

	let statusFilter = $state('all');
	let actorFilter = $state('');
	let selected = new SvelteSet<string>();

	const visible = $derived(
		approvals.filter((approval) => {
			if (statusFilter !== 'all' && approval.status !== statusFilter)
				return false;
			const needle = actorFilter.trim().toLowerCase();
			if (needle && !approval.requester.toLowerCase().includes(needle))
				return false;
			return true;
		}),
	);

	// Only pending approvals are decidable, so only they can be selected.
	const selectable = $derived(visible.filter((a) => a.status === 'pending'));
	const selectedPending = $derived(
		selectable.filter((a) => selected.has(a.id)),
	);

	function toggle(id: string): void {
		if (selected.has(id)) selected.delete(id);
		else selected.add(id);
	}

	function toggleAll(): void {
		selected.clear();
		if (!(selectable.length > 0 && selectedPending.length === selectable.length)) {
			for (const item of selectable) selected.add(item.id);
		}
	}

	function batch(granted: boolean): void {
		const targets = selectedPending;
		if (targets.length === 0) return;
		onbatch(targets, granted);
		selected.clear();
	}
</script>

<Card
	title={t('domain.approvals.title')}
	description={t('domain.approvals.description')}
>
	{#snippet actions()}
		{#if selectedPending.length > 0}
			<Button size="sm" onclick={() => batch(true)}>
				<Icon name="check" size={13} />
				{t('domain.toolCall.approve')} {selectedPending.length}
			</Button>
			<Button variant="outline" size="sm" onclick={() => batch(false)}>
				<Icon name="x" size={13} />
				{t('domain.toolCall.reject')} {selectedPending.length}
			</Button>
		{/if}
	{/snippet}

	<div class="mb-3 grid gap-2 sm:grid-cols-2">
		<Select
			label={t('domain.approvals.status')}
			size="sm"
			value={statusFilter}
			options={statusOptions}
			onchange={(value) => (statusFilter = value)}
		/>
		<Input
			label={t('domain.approvals.requester')}
			size="sm"
			bind:value={actorFilter}
			placeholder={t('domain.approvals.filterRequester')}
		/>
	</div>

	{#if progress}
		<p class="mb-2 text-caption text-muted-foreground">{progress}…</p>
	{/if}

	{#if selectable.length > 0}
		<label
			class="mb-2 flex items-center gap-2 text-caption text-muted-foreground"
		>
			<input
				type="checkbox"
				checked={selectedPending.length === selectable.length}
				onchange={toggleAll}
			/>
			{t('domain.approvals.selectAllPending', { count: selectable.length })}
		</label>
	{/if}

	<div class="space-y-2">
		{#each visible as approval (approval.id)}
			<Card title={approval.title}>
				{#snippet actions()}
					{#if approval.status === 'pending'}
						<input
							type="checkbox"
							aria-label={t('domain.approvals.selectApproval')}
							checked={selected.has(approval.id)}
							onchange={() => toggle(approval.id)}
						/>
					{/if}
					<StatusBadge status={approval.status} size="sm" />
				{/snippet}
				<p class="text-caption">{approval.detail}</p>
				<div
					class="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-micro text-muted-foreground"
				>
					<span>{approval.kind}</span>
					<span>{approval.requester}</span>
					<span>{formatDateTime(approval.requestedAt)}</span>
					<span class="font-mono">{approval.executionId}</span>
				</div>
				{#snippet footer()}
					{#if approval.status === 'pending'}
						<div class="flex items-center gap-2">
							<Button size="sm" onclick={() => ondecide(approval, true)}>
								<Icon name="check" size={13} />
								{t('domain.toolCall.approve')}
							</Button>
							<Button
								variant="outline"
								size="sm"
								onclick={() => ondecide(approval, false)}
							>
								<Icon name="x" size={13} />
								{t('domain.toolCall.reject')}
							</Button>
						</div>
					{:else}
						<span class="text-caption text-muted-foreground">
							{t('domain.approvals.resolvedAs', { status: approval.status })}
						</span>
					{/if}
				{/snippet}
			</Card>
		{:else}
			<p class="text-caption text-muted-foreground">
				{t('domain.approvals.noMatch')}
			</p>
		{/each}
	</div>
</Card>
