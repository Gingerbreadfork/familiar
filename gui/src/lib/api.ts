import type { BindingView, ConfigView, Hosts, Snapshot, ThumbView } from './types';
import * as mock from './mock';

/** True inside the app window; false when the page is opened in a browser for review. */
export const inApp = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	if (!inApp) return mock.call<T>(cmd, args);
	const { invoke } = await import('@tauri-apps/api/core');
	return invoke<T>(cmd, args);
}

export const api = {
	snapshot: () => call<Snapshot | null>('snapshot'),
	config: () => call<ConfigView>('config'),
	setSetting: (path: string, name: string, value: string) =>
		call<Record<string, string>>('set_setting', { path, name, value }),
	playHaptic: (path: string, waveform: string) => call<void>('play_haptic', { path, waveform }),
	hosts: (path: string) => call<Hosts>('hosts', { path }),
	switchHost: (path: string, channel: number) => call<void>('switch_host', { path, channel }),
	renameHost: (path: string, channel: number, name: string) =>
		call<string>('rename_host', { path, channel, name }),
	reload: () => call<void>('reload'),
	setButton: (button: string, binding: BindingView | null) => call<void>('set_button', { button, binding }),
	setScroll: (smooth: boolean, speed: number) => call<void>('set_scroll', { smooth, speed }),
	setThumb: (thumb: ThumbView | null) => call<void>('set_thumb', { thumb }),
	checkAction: (action: string) => call<string>('check_action', { action }),
	waveforms: () => call<string[]>('waveforms')
};

/** Calls `onChange` whenever the daemon reports a change. Returns a function that stops listening. */
export async function onDaemonChange(onChange: () => void): Promise<() => void> {
	if (!inApp) return () => {};
	const { listen } = await import('@tauri-apps/api/event');
	return listen('familiar-changed', onChange);
}
