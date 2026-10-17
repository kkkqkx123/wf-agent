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
	// the route's live subscription, which reloads this tab.
	let { approvals }: { approvals: Approval[] } = $props();

	// ── dialogs ──
	let rejectOpen = $state(false);
	let rejectTarget = $state<Approval | null>(null);
	let rejectReason = $state('');

	async function decideApproval(
		approval: Approval,
		granted: boolean,
	): Promise<void> {
		try {
			if (granted) {
				await approveApproval(approval.id);
				toasts.success('Approval granted');
			} else {
				rejectTarget = approval;
				rejectOpen = true;
				rejectReason = '';
				return;
			}
			// Approving merges into a feature ref, which also changes the
			// file workspace.
			checkpointStore.bump('approvals');
			checkpointStore.bump('files');
		} catch (e) {
			console.error('Failed to resolve approval:', e);
			toasts.error('Approval decision failed');
		}
	}

	async function runReject(): Promise<void> {
		const target = rejectTarget;
		if (!target) return;
		try {
			await rejectApproval(target.id, rejectReason || undefined);
			rejectOpen = false;
			rejectTarget = null;
			rejectReason = '';
			toasts.warning('Approval rejected');
			checkpointStore.bump('approvals');
		} catch (e) {
			console.error('Failed to reject approval:', e);
			toasts.error('Approval rejection failed');
		}
	}

	async function decideBatch(
		targets: Approval[],
		granted: boolean,
	): Promise<void> {
		checkpointStore.beginAction(granted ? 'batch-approve' : 'batch-reject');
		let decided = 0;
		let failed = 0;
		try {
			for (const approval of targets) {
				try {
					if (granted) await approveApproval(approval.id);
					else await rejectApproval(approval.id);
					decided += 1;
				} catch (e) {
					failed += 1;
					console.error('Batch decision failed:', e);
				}
			}
			if (failed === 0) {
				toasts.success(
					`${granted ? 'Approved' : 'Rejected'} ${decided} approval(s)`,
				);
			} else {
				toasts.warning(
					`Decided ${decided}, ${failed} failed`,
					'Re-run the failed items individually.',
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
