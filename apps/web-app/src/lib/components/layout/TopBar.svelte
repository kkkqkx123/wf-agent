<script lang="ts">
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import IconButton from '#lib/ui/components/IconButton.svelte';
	import Button from '#lib/ui/components/Button.svelte';
	import { navItemFor } from '#lib/config/navigation';
	import { preferences, type ThemeMode } from '#lib/stores/preferences.svelte';
	import { ui } from '#lib/stores/ui.svelte';
	import { resolvedTheme } from '#lib/stores/theme.svelte';
	import { cn } from '#lib/ui/cn';
	import LanguageSwitcher from './LanguageSwitcher.svelte';
	import { t } from '#lib/i18n';

	const activeItem = $derived(navItemFor(page.url.pathname));
	const theme = $derived(resolvedTheme());

	const THEME_ICON: Record<ThemeMode, 'sun' | 'moon' | 'monitor'> = {
		light: 'sun',
		dark: 'moon',
		system: 'monitor',
	};

	function cycleTheme(): void {
		const order: ThemeMode[] = ['light', 'dark', 'system'];
		const next = order[(order.indexOf(preferences.theme) + 1) % order.length];
		preferences.setTheme(next);
	}
</script>

<header
	class="frost flex h-12 shrink-0 items-center gap-2 border-b border-border px-3"
>
	<IconButton
		icon="menu"
		label={t('layout.topbar.toggleNavigation')}
		class="lg:hidden"
		onclick={() => ui.toggleMobileNav()}
	/>

	<nav
		aria-label={t('layout.topbar.breadcrumb')}
		class="flex min-w-0 items-center gap-1.5 text-body"
	>
		<a
			href={resolve('/')}
			class="truncate text-muted-foreground hover:text-foreground">wf-agent</a
		>
		{#if activeItem}
			<Icon
				name="chevron-right"
				size={13}
				class="shrink-0 text-muted-foreground/60"
			/>
			<span class="truncate font-medium text-foreground"
				>{activeItem.label()}</span
			>
		{/if}
	</nav>

	<div class="ml-auto flex items-center gap-1.5">
		<Button
			variant="outline"
			size="sm"
			class="hidden gap-2 md:inline-flex"
			onclick={() => ui.setCommandOpen(true)}
		>
			<Icon name="search" size={13} />
			<span>{t('common.search')}</span>
			<kbd
				class="ml-1 rounded border border-border bg-muted px-1 font-mono text-[0.625rem] text-muted-foreground"
			>
				⌘K
			</kbd>
		</Button>

		<IconButton
			icon="command"
			label={t('layout.topbar.commandPalette')}
			onclick={() => ui.toggleCommand()}
		/>
		<IconButton
			icon={THEME_ICON[preferences.theme]}
			label={t('layout.topbar.cycleTheme')}
			onclick={cycleTheme}
		/>
		<IconButton
			icon="panel-right"
			label={t('layout.topbar.toggleInspector')}
			active={preferences.inspectorPinned}
			onclick={() => preferences.toggleInspectorPinned()}
		/>
		<span
			class={cn(
				'ml-1 hidden items-center gap-1.5 rounded-full border border-border px-2 py-0.5 text-micro text-muted-foreground sm:flex',
			)}
			title={t('layout.topbar.currentTheme')}
		>
			<span class="h-1.5 w-1.5 rounded-full bg-success"></span>
			{theme}
		</span>
		<LanguageSwitcher />
	</div>
</header>
