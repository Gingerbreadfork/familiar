// Stand-in data so the page can be reviewed in a browser without the app or the daemon.
import type { ConfigView, Hosts, Snapshot } from './types';

const snapshot: Snapshot = {
	version: '0.1.0',
	configPath: '/home/you/.config/familiar/config.toml',
	conflicts: [],
	devices: [
		{
			path: '/io/github/gingerbreadfork/Familiar/devices/d1A2B3C4D',
			name: 'MX Master 4',
			unitId: '1A2B3C4D',
			model: 'B042',
			online: true,
			batteryPercent: 90,
			batteryStatus: 'discharging',
			settings: {
				dpi: '1000',
				haptic: '100',
				force: '4630',
				wheel: 'ratchet',
				smartshift: '10',
				torque: '75',
				'wheel-invert': 'off',
				'thumb-invert': 'off'
			},
			configured: {},
			applyErrors: [],
			lastApplied: Math.floor(Date.now() / 1000) - 40,
			reconnectMs: 52
		}
	]
};

const config: ConfigView = {
	path: snapshot.configPath,
	buttons: {
		back: { kind: 'direct', action: 'key:KEY_LEFTALT+KEY_LEFT' },
		haptic: { kind: 'gestures', gestures: { tap: 'key:KEY_LEFTMETA', up: 'key:KEY_VOLUMEUP', down: 'key:KEY_VOLUMEDOWN' } }
	},
	smooth: true,
	speed: 1.5,
	thumb: { mode: 'volume', speed: 1, tap: 'key:KEY_PLAYPAUSE' }
};

const hosts: Hosts = {
	current: 1,
	hosts: [
		{ channel: 1, paired: true, name: 'desktop' },
		{ channel: 2, paired: true, name: 'work-laptop' },
		{ channel: 3, paired: false, name: '' }
	]
};

const waveforms = [
	'sharp-state-change', 'damp-state-change', 'sharp-collision', 'damp-collision', 'subtle-collision',
	'happy-alert', 'angry-alert', 'completed', 'square', 'wave', 'firework', 'mad', 'knock', 'jingle', 'ringing'
];

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	await new Promise((r) => setTimeout(r, 60));
	switch (cmd) {
		case 'snapshot':
			// Open the page with ?stopped to see the window when the daemon is not running.
			if (new URLSearchParams(location.search).has('stopped')) return null as T;
			return structuredClone(snapshot) as T;
		case 'config':
			return structuredClone(config) as T;
		case 'hosts':
			return structuredClone(hosts) as T;
		case 'waveforms':
			return waveforms as T;
		case 'set_setting': {
			const d = snapshot.devices[0];
			d.settings[args!.name as string] = args!.value as string;
			return structuredClone(d.settings) as T;
		}
		case 'set_button': {
			const b = args!.binding as ConfigView['buttons'][string] | null;
			if (b) config.buttons[args!.button as string] = b;
			else delete config.buttons[args!.button as string];
			return undefined as T;
		}
		case 'set_scroll':
			config.smooth = args!.smooth as boolean;
			config.speed = args!.speed as number;
			return undefined as T;
		case 'set_thumb':
			config.thumb = args!.thumb as ConfigView['thumb'];
			return undefined as T;
		case 'check_action':
			return args!.action as T;
		case 'rename_host': {
			const h = hosts.hosts.find((x) => x.channel === args!.channel);
			if (h) h.name = args!.name as string;
			return args!.name as T;
		}
		default:
			return undefined as T;
	}
}
