<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import StatusBadge from '@wf-agent/ui/components/StatusBadge.svelte';
	import type { Approval } from '$lib/types/models';
	import { formatDateTime } from '$lib/utils/format';

	let {
		approvals,
		ondecide,
	}: {
		approvals: Approval[];
		ondecide: (approval: Approval, granted: boolean) => void;
	} = $props();
</script>

<div class="space-y-2">
	{#each approvals as approval (approval.id)}
		<Card title={approval.title}>
			{#snippet actions()}
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
							Approve
						</Button>
						<Button
							variant="outline"
							size="sm"
							onclick={() => ondecide(approval, false)}
						>
							<Icon name="x" size={13} />
							Reject
						</Button>
					</div>
				{:else}
					<span class="text-caption text-muted-foreground">
						Resolved · {approval.status}
					</span>
				{/if}
			{/snippet}
		</Card>
	{/each}
</div>
