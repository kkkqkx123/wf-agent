<script lang="ts">
	import DropdownMenu from '#lib/ui/components/DropdownMenu.svelte';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import { preferences, type ThemeMode } from '#lib/stores/preferences.svelte';
	import { t, type MessageKey } from '#lib/i18n';

	const THEME_ICON: Record<ThemeMode, 'sun' | 'moon' | 'monitor'> = {
		light: 'sun',
		dark: 'moon',
		system: 'monitor',
	};

	const THEME_LABEL: Record<ThemeMode, MessageKey> = {
		light: 'layout.topbar.themeLight',
		dark: 'layout.topbar.themeDark',
		system: 'layout.topbar.themeSystem',
	};

	const MODES: ThemeMode[] = ['light', 'dark', 'system'];

	const items = $derived(
		MODES.map((mode) => ({
			id: mode,
			label: t(THEME_LABEL[mode]),
			selected: preferences.theme === mode,
			onselect: () => preferences.setTheme(mode),
		})),
	);
</script>

<DropdownMenu {items} label={t('layout.topbar.theme')}>
	{#snippet trigger()}
		<span
			class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-small text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
		>
			<Icon name={THEME_ICON[preferences.theme]} size={14} />
			<span>{t(THEME_LABEL[preferences.theme])}</span>
			<Icon name="chevron-down" size={13} />
		</span>
	{/snippet}
</DropdownMenu>
