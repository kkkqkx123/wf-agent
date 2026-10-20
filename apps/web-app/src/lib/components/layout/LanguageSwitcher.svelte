<script lang="ts">
	import {
		getLocale,
		setLocale,
		SUPPORTED_LOCALES,
		type Locale,
		t,
	} from '$lib/i18n';

	// Endonyms: a language is named in itself, so these are never translated.
	const LABELS: Record<Locale, string> = { en: 'English', zh: '中文' };

	let current = $state(getLocale());
</script>

<div
	class="flex items-center gap-1 text-micro"
	role="group"
	aria-label={t('common.language')}
>
	{#each SUPPORTED_LOCALES as code (code)}
		<button
			type="button"
			lang={code}
			aria-pressed={current === code}
			class="rounded px-2 py-0.5 transition-colors {current === code
				? 'bg-muted font-medium text-foreground'
				: 'text-muted-foreground hover:text-foreground'}"
			onclick={() => {
				current = code;
				setLocale(code);
			}}
		>
			{LABELS[code]}
		</button>
	{/each}
</div>
