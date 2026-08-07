import { vi } from 'vitest';

/**
 * Silences `console.error` for the current test, for code paths that are
 * *expected* to log. `restoreMocks` puts the real one back afterwards.
 */
export function silenceConsoleError(): void {
  vi.spyOn(console, 'error').mockImplementation(() => {});
}
