/**
 * Single entry point for translations.
 *
 * Messages are compiled by Paraglide from `messages/{locale}.json` into typed
 * functions under `#paraglide`, so a wrong key is a type error and unused
 * messages are tree-shaken away. Components import from here rather than from
 * the generated directory directly, which keeps the key type defined in one
 * place and gives data-driven call sites a message function instead of a
 * string key.
 */
import { m } from '#paraglide/messages.js';
import {
	getLocale,
	locales,
	setLocale as paraglideSetLocale,
} from '#paraglide/runtime.js';

export type Locale = (typeof locales)[number];

export const SUPPORTED_LOCALES: readonly Locale[] = locales;
export const DEFAULT_LOCALE: Locale = 'en';

export type MessageKey = keyof typeof m;

/**
 * Value type expected by a single message. Paraglide compiles each message into
 * a function whose first argument carries its placeholders, so this resolves to
 * that argument type instead of a loose record.
 */
export type MessageValues<K extends MessageKey> = Parameters<(typeof m)[K]>[0];

/** Renders one message in the active locale. */
export function t<K extends MessageKey>(
	key: K,
	values?: MessageValues<K>,
): string {
	return (m[key] as (values?: unknown) => string)(values);
}

/**
 * The message function for a key, for data-driven call sites such as navigation
 * entries and option lists.
 */
export function message(key: MessageKey): () => string {
	return () => (t as (key: MessageKey, ...args: unknown[]) => string)(key);
}

export function setLocale(
	locale: Locale,
	options?: { reload?: boolean },
): ReturnType<typeof paraglideSetLocale> {
	return paraglideSetLocale(locale, options);
}

export { getLocale };

/** Active locale, for `Intl` formatting outside components. */
export function currentLocale(): Locale {
	return getLocale() as Locale;
}
