<script lang="ts">
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Dialog from '@wf-agent/ui/components/Dialog.svelte';
	import Textarea from '@wf-agent/ui/components/Textarea.svelte';
	import ApprovalsPanel from '$lib/components/domain/checkpoints/ApprovalsPanel.svelte';
	import { approveApproval, rejectApproval } from '$lib/services/checkpoints';
	import type { Approval } from '$lib/types/models';
	import { toasts } from '$lib/stores/toast.svelte';
	import { checkpoints as checkpointStore } from '$lib/stores/checkpoints.svelte';

	// No reload prop: the refetch is driven by `bump('approvals')` notifying
	// the route's live subscription, which reloads this tab. `onremove` drops
	// rows optimistically before the refetch lands.
	let {
		approvals,
		onremove,
	}: {
		approvals: Approval[];
		onremove: (ids: string[]) => void;
	} = $props();

	// ── dialogs ──
	let rejectOpen = $state(false);
	let rejectTarget = $state<Approval | null>(null);
	let rejectReason = $state('');

	async function decideApproval(
		approval: Approval,
		granted: boolean,
	): Promise<void> {
		if (!granted) {
			rejectTarget = approval;
			rejectOpen = true;
			rejectReason = '';
			return;
		}
		// Optimistic: drop the row now; the bump refetch reconciles and a
		// failure restores it.
		onremove([approval.id]);
		try {
			await approveApproval(approval.id);
			toasts.success('Approval granted');
			// Approving merges into a feature ref, which also changes the
			// file workspace.
			checkpointStore.bump('approvals');
			checkpointStore.bump('files');
		} catch (e) {
			console.error('Failed to resolve approval:', e);
			toasts.error(
				'Approval decision failed',
				e instanceof Error ? e.message : undefined,
			);
			checkpointStore.bump('approvals');
		}
	}

	async function runReject(): Promise<void> {
		const target = rejectTarget;
		if (!target) return;
		rejectOpen = false;
		rejectTarget = null;
		onremove([target.id]);
		try {
			await rejectApproval(target.id, rejectReason || undefined);
			rejectReason = '';
			toasts.warning('Approval rejected');
			checkpointStore.bump('approvals');
		} catch (e) {
			console.error('Failed to reject approval:', e);
			toasts.error(
				'Approval rejection failed',
				e instanceof Error ? e.message : undefined,
			);
			checkpointStore.bump('approvals');
		}
	}

	async function decideBatch(
		targets: Approval[],
		granted: boolean,
	): Promise<void> {
		checkpointStore.beginAction(granted ? 'batch-approve' : 'batch-reject');
		onremove(targets.map((approval) => approval.id));
		let decided = 0;
		const failures: string[] = [];
		try {
			for (const approval of targets) {
				try {
					if (granted) await approveApproval(approval.id);
					else await rejectApproval(approval.id);
					decided += 1;
				} catch (e) {
					const reason = e instanceof Error ? e.message : 'request failed';
					failures.push(`${approval.requester || approval.id}: ${reason}`);
					console.error('Batch decision failed:', e);
				}
				checkpointStore.beginAction(
					`${granted ? 'batch-approve' : 'batch-reject'} ${decided + failures.length}/${targets.length}`,
				);
			}
			if (failures.length === 0) {
				toasts.success(
					`${granted ? 'Approved' : 'Rejected'} ${decided} approval(s)`,
				);
			} else {
				toasts.warning(
					`Decided ${decided}, ${failures.length} failed`,
					failures.slice(0, 3).join('; '),
				);
			}
			checkpointStore.bump('approvals');
			if (granted) checkpointStore.bump('files');
		} finally {
			checkpointStore.endAction();
		}
	}
</script>

<ApprovalsPanel
	{approvals}
	progress={checkpointStore.action}
	ondecide={(approval: Approval, granted: boolean) =>
		void decideApproval(approval, granted)}
	onbatch={(targets: Approval[], granted: boolean) =>
		void decideBatch(targets, granted)}
/>

<Dialog
	bind:open={rejectOpen}
	title="Reject approval"
	description="Rejecting rolls the approval partition back to its baseline. Only the pending reference is removed; history is untouched."
	onclose={() => (rejectTarget = null)}
>
	<Textarea
		label="Reason (optional)"
		bind:value={rejectReason}
		placeholder="Why is this change unacceptable? What should change next?"
		rows={3}
	/>
	{#snippet footer()}
		<Button variant="ghost" size="sm" onclick={() => (rejectOpen = false)}>
			Cancel
		</Button>
		<Button size="sm" onclick={() => void runReject()}>Reject</Button>
	{/snippet}
</Dialog>
