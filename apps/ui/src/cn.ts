type ClassValue =
	| string
	| number
	| null
	| undefined
	| false
	| ClassValue[]
	| Record<string, boolean | null | undefined>;

/**
 * Joins conditional class names and keeps the last declaration of any
 * duplicated utility group. Duplicate groups are collapsed from the right so
 * caller overrides win over component defaults.
 */
export function cn(...values: ClassValue[]): string {
	const out: string[] = [];

	const walk = (value: ClassValue): void => {
		if (!value && value !== 0) return;
		if (typeof value === 'string' || typeof value === 'number') {
			const text = String(value).trim();
			if (text) out.push(text);
			return;
		}
		if (Array.isArray(value)) {
			value.forEach(walk);
			return;
		}
		for (const [key, enabled] of Object.entries(value)) {
			if (enabled) walk(key);
		}
	};

	values.forEach(walk);

	const seen = new Map<string, string>();
	for (const group of out.join(' ').split(/\s+/)) {
		const variant = group.split(':');
		const tail = variant[variant.length - 1];
		const dot = tail.indexOf('-');
		const base = dot === -1 ? tail : tail.slice(0, dot);
		// Variant prefixes stay part of the key so `hover:bg-card` never
		// collapses into `bg-card`.
		const prefix = variant.slice(0, -1).join(':');
		seen.set(prefix ? `${prefix}|${base}` : base, group);
	}

	return [...seen.values()].join(' ');
}
