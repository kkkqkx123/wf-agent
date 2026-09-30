import { browser } from '$app/environment';

/**
 * Local first-paint appearance track: theme, density, and shell geometry
 * under the `wf-ui-preferences` key. Applied before first paint so the UI
 * never flashes; cross-device behavior lives in the server preference
 * document (`services/preferences.ts`) instead.
 */

export type ThemeMode = 'light' | 'dark' | 'system';
export type Density = 'compact' | 'default' | 'comfortable';
export type MinimapMode = 'auto' | 'on' | 'off';

export const SIDEBAR_WIDTH_MIN = 200;
export const SIDEBAR_WIDTH_MAX = 360;

/** Inspector width presets, cycled in order by the split view header. */
export const INSPECTOR_WIDTH_STEPS = [
	{ width: 280, label: 'narrow' },
	{ width: 360, label: 'default' },
	{ width: 460, label: 'wide' },
] as const;

export const INSPECTOR_WIDTH_MIN = 240;
export const INSPECTOR_WIDTH_MAX = 640;
export const INSPECTOR_WIDTH_DEFAULT = 360;

const STORAGE_KEY = 'wf-ui-preferences';

interface PersistedPreferences {
	theme?: ThemeMode;
	density?: Density;
	sidebarCollapsed?: boolean;
	sidebarWidth?: number;
	inspectorWidth?: number;
	inspectorPinned?: boolean;
	minimapMode?: MinimapMode;
}

const DENSITY_SCALE: Record<Density, number> = {
	compact: 0.94,
	default: 1,
	comfortable: 1.06,
};

/** Corner-radius scale per density; type keeps its own `--font-scale`. */
const DENSITY_SPACING_SCALE: Record<Density, number> = {
	compact: 0.92,
	default: 1,
	comfortable: 1.08,
};

function read(): PersistedPreferences {
	if (!browser) return {};
	try {
		const raw = localStorage.getItem(STORAGE_KEY);
		return raw ? (JSON.parse(raw) as PersistedPreferences) : {};
	} catch {
		return {};
	}
}

class PreferencesStore {
	theme = $state<ThemeMode>('system');
	density = $state<Density>('default');
	sidebarCollapsed = $state(false);
	sidebarWidth = $state(240);
	inspectorWidth = $state(INSPECTOR_WIDTH_DEFAULT);
	inspectorPinned = $state(false);
	minimapMode = $state<MinimapMode>('auto');

	constructor() {
		const stored = read();
		if (
			stored.theme === 'light' ||
			stored.theme === 'dark' ||
			stored.theme === 'system'
		) {
			this.theme = stored.theme;
		}
		if (
			stored.density === 'compact' ||
			stored.density === 'default' ||
			stored.density === 'comfortable'
		) {
			this.density = stored.density;
		}
		if (typeof stored.sidebarCollapsed === 'boolean') {
			this.sidebarCollapsed = stored.sidebarCollapsed;
		}
		if (
			typeof stored.sidebarWidth === 'number' &&
			Number.isFinite(stored.sidebarWidth)
		) {
			this.sidebarWidth = clampSidebarWidth(stored.sidebarWidth);
		}
		if (
			typeof stored.inspectorWidth === 'number' &&
			Number.isFinite(stored.inspectorWidth)
		) {
			this.inspectorWidth = clampInspectorWidth(stored.inspectorWidth);
		}
		if (typeof stored.inspectorPinned === 'boolean') {
			this.inspectorPinned = stored.inspectorPinned;
		}
		if (
			stored.minimapMode === 'auto' ||
			stored.minimapMode === 'on' ||
			stored.minimapMode === 'off'
		) {
			this.minimapMode = stored.minimapMode;
		}
	}

	get fontScale(): number {
		return DENSITY_SCALE[this.density];
	}

	get spacingScale(): number {
		return DENSITY_SPACING_SCALE[this.density];
	}

	setTheme(mode: ThemeMode): void {
		this.theme = mode;
		this.persist();
	}

	setDensity(density: Density): void {
		this.density = density;
		this.persist();
	}

	toggleSidebar(): void {
		this.sidebarCollapsed = !this.sidebarCollapsed;
		this.persist();
	}

	setSidebarWidth(width: number): void {
		this.sidebarWidth = clampSidebarWidth(width);
		this.persist();
	}

	/** Preset name for the current inspector width, used for the button label. */
	get inspectorWidthLabel(): string {
		return (
			INSPECTOR_WIDTH_STEPS.find((step) => step.width === this.inspectorWidth)
				?.label ?? `${Math.round(this.inspectorWidth)}px`
		);
	}

	setInspectorWidth(width: number): void {
		this.inspectorWidth = clampInspectorWidth(width);
		this.persist();
	}

	/** Steps through the width presets, wrapping back to the first one. */
	cycleInspectorWidth(): void {
		const current = INSPECTOR_WIDTH_STEPS.findIndex(
			(step) => step.width === this.inspectorWidth,
		);
		const next =
			INSPECTOR_WIDTH_STEPS[(current + 1) % INSPECTOR_WIDTH_STEPS.length] ??
			INSPECTOR_WIDTH_STEPS[1];
		this.setInspectorWidth(next.width);
	}

	toggleInspectorPinned(): void {
		this.inspectorPinned = !this.inspectorPinned;
		this.persist();
	}

	setMinimapMode(mode: MinimapMode): void {
		this.minimapMode = mode;
		this.persist();
	}

	private persist(): void {
		if (!browser) return;
		try {
			localStorage.setItem(
				STORAGE_KEY,
				JSON.stringify({
					theme: this.theme,
					density: this.density,
					sidebarCollapsed: this.sidebarCollapsed,
					sidebarWidth: this.sidebarWidth,
					inspectorWidth: this.inspectorWidth,
					inspectorPinned: this.inspectorPinned,
					minimapMode: this.minimapMode,
				} satisfies PersistedPreferences),
			);
		} catch {
			// Storage may be unavailable; in-memory preferences still work.
		}
	}
}

function clampSidebarWidth(width: number): number {
	return Math.min(
		SIDEBAR_WIDTH_MAX,
		Math.max(SIDEBAR_WIDTH_MIN, Math.round(width)),
	);
}

function clampInspectorWidth(width: number): number {
	return Math.min(
		INSPECTOR_WIDTH_MAX,
		Math.max(INSPECTOR_WIDTH_MIN, Math.round(width)),
	);
}

export const preferences = new PreferencesStore();
