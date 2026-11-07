/**
 * Plain-text clipboard write with a legacy fallback. The async Clipboard
 * API only exists in secure contexts and can additionally reject when the
 * embedding page grants no clipboard permission (preview hosting, plain
 * HTTP origins, sandboxed iframes). The textarea plus execCommand path
 * keeps those environments working as long as the call runs off a user
 * gesture. Returns whether the text reached the clipboard.
 */
export async function copyPlainText(text: string): Promise<boolean> {
	if (!text) return false;
	try {
		if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
			await navigator.clipboard.writeText(text);
			return true;
		}
	} catch {
		// Fall through to the legacy path below.
	}
	return legacyCopy(text);
}

/** Synchronous textarea copy for environments without the Clipboard API. */
function legacyCopy(text: string): boolean {
	try {
		if (typeof document === 'undefined' || !document.body) return false;
		const area = document.createElement('textarea');
		area.value = text;
		area.setAttribute('readonly', '');
		area.style.position = 'fixed';
		area.style.top = '0';
		area.style.opacity = '0';
		document.body.appendChild(area);
		area.select();
		area.setSelectionRange(0, area.value.length);
		const ok = document.execCommand('copy');
		document.body.removeChild(area);
		return ok;
	} catch {
		return false;
	}
}
