import { browser } from '$app/environment';

export type ThemeMode = 'light' | 'dark' | 'system';
export type Density = 'compact' | 'default' | 'comfortable';

export const SIDEBAR_WIDTH_MIN = 200;
export const SIDEBAR_WIDTH_MAX = 360;
export const INSPECTOR_WIDTH_MIN = 260;
export const INSPECTOR_WIDTH_MAX = 620;

const STORAGE_KEY = 'wf-ui-preferences';

interface PersistedPreferences {
	theme?: ThemeMode;
	density?: Density;
	sidebarCollapsed?: boolean;
	sidebarWidth?: number;
	inspectorPinned?: boolean;
	inspectorWidth?: number;
}

const DENSITY_SCALE: Record<Density, number> = {
	compact: 0.94,
	default: 1,
	comfortable: 1.06,
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
	inspectorPinned = $state(true);
	inspectorWidth = $state(360);

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
		if (typeof stored.inspectorPinned === 'boolean') {
			this.inspectorPinned = stored.inspectorPinned;
		}
		if (
			typeof stored.inspectorWidth === 'number' &&
			Number.isFinite(stored.inspectorWidth)
		) {
			this.inspectorWidth = clampInspectorWidth(stored.inspectorWidth);
		}
	}

	get fontScale(): number {
		return DENSITY_SCALE[this.density];
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

	toggleInspectorPinned(): void {
		this.inspectorPinned = !this.inspectorPinned;
		this.persist();
	}

	setInspectorWidth(width: number): void {
		this.inspectorWidth = clampInspectorWidth(width);
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
					inspectorPinned: this.inspectorPinned,
					inspectorWidth: this.inspectorWidth,
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
