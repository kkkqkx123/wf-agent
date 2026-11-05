<script lang="ts">
	import DropdownMenu from '#lib/ui/components/DropdownMenu.svelte';
	import Icon from '#lib/ui/icons/Icon.svelte';
	import { getLocale, setLocale, SUPPORTED_LOCALES, type Locale, t } from '#lib/i18n';

	// Endonyms: a language is named in itself, so these are never translated.
	const LABELS: Record<Locale, string> = { en: 'English', zh: '中文' };

	let current = $state(getLocale());

	const items = $derived(
		SUPPORTED_LOCALES.map((code) => ({
			id: code,
			label: LABELS[code],
			selected: code === current,
			onselect: () => {
				current = code;
				setLocale(code);
			},
		})),
	);
</script>

<DropdownMenu {items} label={t('common.language')}>
	{#snippet trigger()}
		<span
			class="inline-flex items-center gap-1 rounded-md px-2 py-1 text-small text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
		>
			<span lang={current}>{LABELS[current]}</span>
			<Icon name="chevron-down" size={13} />
		</span>
	{/snippet}
</DropdownMenu>
