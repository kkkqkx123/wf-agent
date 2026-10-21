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
	} from '#lib/shortcuts/registry';
	import { t } from '#lib/i18n';

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
				message = t('layout.shortcuts.invalidBinding', {
					error: validation.error ?? t('layout.shortcuts.unknownError'),
				});
				refresh();
				return;
			}
			const conflicts = findShortcutConflicts(candidate).filter(
				(conflict) =>
					conflict.type === 'hotkey' && !conflict.registration.id.includes(id),
			);
			setShortcutOverride(
				id,
				candidate === action?.defaultKeys ? null : candidate,
			);
			message = conflicts.length
				? t('layout.shortcuts.savedWithConflict', {
						action: action?.label ?? id,
					})
				: t('layout.shortcuts.remapped', {
						action: action?.label ?? id,
						keys: candidate,
					});
			refresh();
		},
		onCancel: () => {
			recordingId = null;
		},
		onClear: () => {
			if (!recordingId) return;
			const id = recordingId;
			clearShortcutOverride(id);
			message = t('layout.shortcuts.restored', { action: id });
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
		message = t('layout.shortcuts.disabledMessage', { action: id });
		refresh();
	}

	function restore(id: string): void {
		clearShortcutOverride(id);
		message = t('layout.shortcuts.restored', { action: id });
		refresh();
	}
</script>

<Card
	title={t('layout.shortcuts.title')}
	description={t('layout.shortcuts.description')}
>
	<div class="space-y-1">
		{#each rows as row (row.id)}
			<div class="flex items-center justify-between gap-3 rounded px-2 py-1.5">
				<div class="min-w-0">
					<p class="truncate text-sm" class:opacity-50={row.disabled}>
						{row.label}
					</p>
					<p class="text-caption text-muted-foreground">{row.scope}</p>
				</div>
				<div class="flex shrink-0 items-center gap-2">
					{#if recordingId === row.id}
						<kbd class="animate-pulse rounded border px-2 py-0.5 text-xs"
							>{t('layout.shortcuts.recording')}</kbd
						>
					{:else}
						<kbd
							class="rounded border px-2 py-0.5 text-xs"
							class:opacity-50={row.disabled}
						>
							{row.disabled ? t('layout.shortcuts.disabled') : row.keys}
						</kbd>
					{/if}
					<Button
						size="sm"
						variant="outline"
						onclick={() => startRecording(row.id)}
					>
						<Icon name="pencil" size={12} />
						{t('layout.shortcuts.record')}
					</Button>
					{#if row.disabled || row.keys !== (SHORTCUT_ACTIONS.find((a) => a.id === row.id)?.defaultKeys ?? '')}
						<Button size="sm" variant="ghost" onclick={() => restore(row.id)}
							>{t('layout.shortcuts.reset')}</Button
						>
					{:else}
						<Button size="sm" variant="ghost" onclick={() => disable(row.id)}
							>{t('layout.shortcuts.disable')}</Button
						>
					{/if}
				</div>
			</div>
		{/each}
	</div>
	{#if recordingId}
		<p class="mt-2 text-caption text-muted-foreground">
			{t('layout.shortcuts.pressHint')}
		</p>
	{/if}
	{#if message}
		<p class="mt-2 text-caption text-muted-foreground">{message}</p>
	{/if}
</Card>
