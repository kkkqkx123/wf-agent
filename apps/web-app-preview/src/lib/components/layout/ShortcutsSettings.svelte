<script lang="ts">
	import { createHotkeyRecorder } from '@tanstack/svelte-hotkeys';
	import Button from '@wf-agent/ui/components/Button.svelte';
	import Card from '@wf-agent/ui/components/Card.svelte';
	import Icon from '@wf-agent/ui/icons/Icon.svelte';
	import {
		SHORTCUT_ACTIONS,
		clearShortcutOverride,
		findShortcutConflicts,
		resolvedShortcutRows,
		setShortcutOverride,
		validateShortcut,
	} from '$lib/shortcuts/registry';

	interface Row {
		id: string;
		label: string;
		scope: string;
		keys: string;
		disabled: boolean;
	}

	let rows = $state<Row[]>(readRows());
	let recordingId = $state<string | null>(null);
	let message = $state('');

	function readRows(): Row[] {
		return resolvedShortcutRows().map((row) => ({
			id: row.action.id,
			label: row.action.label,
			scope: row.action.scope,
			keys: row.keys,
			disabled: row.disabled,
		}));
	}

	function refresh(): void {
		rows = readRows();
	}

	const recorder = createHotkeyRecorder({
		onRecord: (hotkey) => {
			if (!recordingId) return;
			const id = recordingId;
			recordingId = null;
			const action = SHORTCUT_ACTIONS.find((entry) => entry.id === id);
			const candidate = String(hotkey);
			const validation = validateShortcut(candidate);
			if (!validation.valid) {
				message = `Invalid binding: ${validation.error ?? 'unknown error'}`;
				refresh();
				return;
			}
			const conflicts = findShortcutConflicts(candidate).filter(
				(conflict) => conflict.type === 'hotkey' && !conflict.registration.id.includes(id),
			);
			setShortcutOverride(id, candidate === action?.defaultKeys ? null : candidate);
			message = conflicts.length
				? `${action?.label ?? id} saved, but it conflicts with an active binding.`
				: `${action?.label ?? id} remapped to ${candidate}.`;
			refresh();
		},
		onCancel: () => {
			recordingId = null;
		},
		onClear: () => {
			if (!recordingId) return;
			clearShortcutOverride(recordingId);
			message = `${recordingId} restored to its default binding.`;
			recordingId = null;
			refresh();
		},
	});

	function startRecording(id: string): void {
		message = '';
		recordingId = id;
		recorder.startRecording();
	}

	function disable(id: string): void {
		clearShortcutOverride(id);
		setShortcutOverride(id, null);
		message = `${id} disabled.`;
		refresh();
	}

	function restore(id: string): void {
		clearShortcutOverride(id);
		message = `${id} restored to its default binding.`;
		refresh();
	}
</script>

<Card
	title="Keyboard shortcuts"
	description="Record a new key combination or disable a binding. Stored locally in this browser."
>
	<div class="space-y-1">
		{#each rows as row (row.id)}
			<div class="flex items-center justify-between gap-3 rounded px-2 py-1.5">
				<div class="min-w-0">
					<p class="truncate text-sm" class:opacity-50={row.disabled}>{row.label}</p>
					<p class="text-caption text-muted-foreground">{row.scope}</p>
				</div>
				<div class="flex shrink-0 items-center gap-2">
					{#if recordingId === row.id}
						<kbd class="animate-pulse rounded border px-2 py-0.5 text-xs">Recording… Esc to cancel</kbd>
					{:else}
						<kbd class="rounded border px-2 py-0.5 text-xs" class:opacity-50={row.disabled}>
							{row.disabled ? 'Disabled' : row.keys}
						</kbd>
					{/if}
					<Button size="sm" variant="outline" onclick={() => startRecording(row.id)}>
						<Icon name="pencil" size={12} />
						Record
					</Button>
					{#if row.disabled || row.keys !== (SHORTCUT_ACTIONS.find((a) => a.id === row.id)?.defaultKeys ?? '')}
						<Button size="sm" variant="ghost" onclick={() => restore(row.id)}>Reset</Button>
					{:else}
						<Button size="sm" variant="ghost" onclick={() => disable(row.id)}>Disable</Button>
					{/if}
				</div>
			</div>
		{/each}
	</div>
	{#if recordingId}
		<p class="mt-2 text-caption text-muted-foreground">
			Press any key combination. Backspace/Delete clears the binding; Escape cancels.
		</p>
	{/if}
	{#if message}
		<p class="mt-2 text-caption text-muted-foreground">{message}</p>
	{/if}
</Card>
