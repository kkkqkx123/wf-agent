<script lang="ts">
	import Icon from '$lib/components/icons/Icon.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Switch from '$lib/components/ui/Switch.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import PageHeader from '$lib/components/layout/PageHeader.svelte';
	import {
		preferences,
		type Density,
		type ThemeMode,
	} from '$lib/stores/preferences.svelte';
	import { behavior } from '$lib/stores/behavior.svelte';
	import { toasts } from '$lib/stores/toast.svelte';
	import { cn } from '$lib/utils/cn';

	const SECTIONS = [
		{ id: 'appearance', label: 'Appearance', icon: 'sun' },
		{ id: 'execution', label: 'Execution defaults', icon: 'activity' },
		{ id: 'notifications', label: 'Notifications', icon: 'bell' },
		{ id: 'workspace', label: 'Workspace', icon: 'sliders' },
	] as const;

	let section = $state<(typeof SECTIONS)[number]['id']>('appearance');

	const THEME_OPTIONS = [
		{ value: 'light', label: 'Light' },
		{ value: 'dark', label: 'Dark' },
		{ value: 'system', label: 'Follow system' },
	];

	const DENSITY_OPTIONS = [
		{ value: 'compact', label: 'Compact' },
		{ value: 'default', label: 'Default' },
		{ value: 'comfortable', label: 'Comfortable' },
	];

	const PAGE_SIZE_OPTIONS = [
		{ value: '25', label: '25' },
		{ value: '50', label: '50' },
		{ value: '100', label: '100' },
	];

	async function saveSettings(): Promise<void> {
		await behavior.save();
		if (behavior.error) toasts.error(behavior.error);
		else toasts.success('Preferences saved');
	}

	async function restoreSettings(): Promise<void> {
		await behavior.restoreDefaults();
		if (behavior.error) toasts.error(behavior.error);
		else toasts.success('Defaults restored');
	}
</script>

<div class="flex h-full min-h-0 flex-col">
	<PageHeader
		title="Settings"
		description="Appearance, execution defaults and notification behaviour."
	>
		{#snippet actions()}
			<Button
				variant="outline"
				size="sm"
				disabled={behavior.saving}
				onclick={() => void restoreSettings()}
			>
				<Icon name="history" size={13} />
				Restore defaults
			</Button>
			<Button
				size="sm"
				disabled={behavior.saving}
				onclick={() => void saveSettings()}
			>
				<Icon name="check" size={13} />
				Save
			</Button>
		{/snippet}
	</PageHeader>

	<div
		class="grid min-h-0 flex-1 gap-3 overflow-hidden px-4 pb-4 lg:grid-cols-[14rem_minmax(0,1fr)]"
	>
		<nav class="space-y-1 overflow-y-auto">
			{#each SECTIONS as item (item.id)}
				<button
					type="button"
					onclick={() => (section = item.id)}
					class={cn(
						'flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-body transition-colors',
						section === item.id
							? 'bg-accent font-medium text-accent-foreground'
							: 'text-muted-foreground hover:bg-accent/60 hover:text-foreground',
					)}
				>
					<Icon name={item.icon} size={15} />
					<span class="truncate">{item.label}</span>
				</button>
			{/each}
		</nav>

		<div class="min-h-0 overflow-y-auto pb-2">
			{#if section === 'appearance'}
				<div class="space-y-3">
					<Card
						title="Theme"
						description="Applied before first paint and persisted locally."
					>
						<Select
							value={preferences.theme}
							options={THEME_OPTIONS}
							placeholder="Theme"
							class="w-56"
							onchange={(value) => preferences.setTheme(value as ThemeMode)}
						/>
						<div class="mt-3 flex flex-wrap gap-2">
							{#each THEME_OPTIONS as option (option.value)}
								<Button
									variant={preferences.theme === option.value
										? 'secondary'
										: 'outline'}
									size="sm"
									onclick={() =>
										preferences.setTheme(option.value as ThemeMode)}
								>
									<Icon
										name={option.value === 'light'
											? 'sun'
											: option.value === 'dark'
												? 'moon'
												: 'monitor'}
										size={13}
									/>
									{option.label}
								</Button>
							{/each}
						</div>
					</Card>

					<Card
						title="Type density"
						description="Scales interface type without changing layout units."
					>
						<Select
							value={preferences.density}
							options={DENSITY_OPTIONS}
							placeholder="Density"
							class="w-56"
							onchange={(value) => preferences.setDensity(value as Density)}
						/>
						<p class="mt-2 text-caption text-muted-foreground">
							Current scale {preferences.fontScale.toFixed(2)}×
						</p>
					</Card>
				</div>
			{:else if section === 'execution'}
				<div class="space-y-3">
					<p class="text-caption text-muted-foreground">
						These defaults are stored in the server preference document, so they
						follow the user across browsers. Press Save to write them.
					</p>
					{#if behavior.error}
						<p
							class="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-1.5 text-caption text-destructive"
						>
							{behavior.error}
						</p>
					{/if}
					<Card
						title="Paging"
						description="Requested page size for every offset-paginated list."
					>
						<Select
							value={String(behavior.pageSize)}
							options={PAGE_SIZE_OPTIONS}
							placeholder="Page size"
							class="w-40"
							onchange={(value) => (behavior.pageSize = Number(value))}
						/>
					</Card>
					<Card title="Live updates">
						<Switch
							checked={behavior.autoRefresh}
							label="Auto refresh lists on live events"
							onchange={(checked) => (behavior.autoRefresh = checked)}
						/>
						<Switch
							checked={behavior.streamFollow}
							label="Prepend streamed events"
							class="mt-2"
							onchange={(checked) => (behavior.streamFollow = checked)}
						/>
					</Card>
				</div>
			{:else if section === 'notifications'}
				<div class="space-y-3">
					<Card title="Toasts" description="Feed the toast stack with samples.">
						<div class="flex gap-2">
							<Button
								variant="outline"
								size="sm"
								onclick={() => toasts.success('Saved', 'Preference applied')}
							>
								Test success toast
							</Button>
							<Button
								variant="outline"
								size="sm"
								onclick={() => toasts.error('Blocked', 'Retry may be required')}
							>
								Test error toast
							</Button>
						</div>
					</Card>
					<Card title="Accessibility">
						<Switch
							checked={behavior.reduceMotion}
							label="Always reduce motion"
							onchange={(checked) => (behavior.reduceMotion = checked)}
						/>
						<p class="mt-2 text-caption text-muted-foreground">
							The OS reduced-motion preference is honoured automatically.
						</p>
					</Card>
				</div>
			{:else}
				<div class="space-y-3">
					<Card title="Shell">
						<Switch
							checked={preferences.sidebarCollapsed}
							label="Collapse sidebar to icon rail"
							onchange={() => preferences.toggleSidebar()}
						/>
						<Switch
							checked={preferences.inspectorPinned}
							label="Keep inspector docked on wide screens"
							class="mt-2"
							onchange={() => preferences.toggleInspectorPinned()}
						/>
					</Card>
					<Card title="Sidebar width" description="Persisted between sessions.">
						<div class="flex items-center gap-3">
							<input
								type="range"
								min="200"
								max="360"
								step="8"
								value={preferences.sidebarWidth}
								oninput={(event) =>
									preferences.setSidebarWidth(
										Number((event.currentTarget as HTMLInputElement).value),
									)}
								class="w-full accent-primary"
							/>
							<span
								class="w-12 shrink-0 text-right text-caption tabular-nums text-muted-foreground"
							>
								{preferences.sidebarWidth}px
							</span>
						</div>
					</Card>
				</div>
			{/if}
		</div>
	</div>
</div>
