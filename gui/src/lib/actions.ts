// Turning action strings like `key:KEY_LEFTALT+KEY_LEFT` into words, and back.

import type { BindingView } from './types';

/** Buttons that can be given actions, by their config names. */
export type ButtonId = 'middle' | 'smartshift' | 'gesture' | 'forward' | 'back' | 'haptic';

/** What each button does with nothing configured. */
export const NORMALLY: Record<ButtonId, string> = {
	middle: 'Middle click',
	smartshift: 'Switches the wheel between ratchet and free spin',
	gesture: 'Nothing until you give it something to do',
	forward: 'Forward',
	back: 'Back',
	haptic: 'Mouse button 7, which most apps ignore'
};

/** Every part of the mouse the window can show, on top or underneath. */
export type PartId =
	| 'wheel'
	| 'smartshift'
	| 'thumb-wheel'
	| 'gesture'
	| 'forward'
	| 'back'
	| 'haptic'
	| 'sensor'
	| 'easy-switch'
	| 'power';

export interface PartInfo {
	id: PartId;
	name: string;
	side: 'top' | 'under';
	/** Where to find it, for someone holding the mouse. */
	where: string;
}

export const PARTS: PartInfo[] = [
	{ id: 'wheel', name: 'Scroll wheel', side: 'top', where: 'The steel wheel under your index finger. It also clicks.' },
	{ id: 'smartshift', name: 'Shift button', side: 'top', where: 'The small square button behind the wheel.' },
	{ id: 'haptic', name: 'Haptic panel', side: 'top', where: 'The rippled pad your thumb rests on. It presses in and taps back.' },
	{ id: 'thumb-wheel', name: 'Thumb wheel', side: 'top', where: 'The knurled wheel just above your thumb.' },
	{ id: 'gesture', name: 'Gesture button', side: 'top', where: 'The side button nearest the front.' },
	{ id: 'forward', name: 'Forward', side: 'top', where: 'The middle side button, beside the thumb wheel.' },
	{ id: 'back', name: 'Back', side: 'top', where: 'The side button nearest your palm.' },
	{ id: 'sensor', name: 'Sensor', side: 'under', where: 'The eye underneath. It sets how far the pointer moves.' },
	{ id: 'easy-switch', name: 'Easy-Switch', side: 'under', where: 'The round button underneath, with a light for each computer.' },
	{ id: 'power', name: 'Battery', side: 'under', where: 'The power switch underneath. Charge over USB-C at the front.' }
];

export const part = (id: PartId) => PARTS.find((p) => p.id === id)!;

export const GESTURES = [
	{ key: 'tap', label: 'Click' },
	{ key: 'double', label: 'Double-click' },
	{ key: 'hold', label: 'Press and hold' },
	{ key: 'up', label: 'Hold and move up' },
	{ key: 'down', label: 'Hold and move down' },
	{ key: 'left', label: 'Hold and move left' },
	{ key: 'right', label: 'Hold and move right' }
] as const;

export const DIAGONALS = [
	{ key: 'up-left', label: 'Hold and move up and left' },
	{ key: 'up-right', label: 'Hold and move up and right' },
	{ key: 'down-left', label: 'Hold and move down and left' },
	{ key: 'down-right', label: 'Hold and move down and right' }
] as const;

const KEY_WORDS: Record<string, string> = {
	CTRL: 'Ctrl',
	SHIFT: 'Shift',
	ALT: 'Alt',
	ALTGR: 'AltGr',
	SUPER: 'Super',
	LEFTCTRL: 'Ctrl',
	RIGHTCTRL: 'Ctrl',
	LEFTSHIFT: 'Shift',
	RIGHTSHIFT: 'Shift',
	LEFTALT: 'Alt',
	RIGHTALT: 'AltGr',
	LEFTMETA: 'Super',
	RIGHTMETA: 'Super',
	PAGEUP: 'Page Up',
	PAGEDOWN: 'Page Down',
	VOLUMEUP: 'Volume up',
	VOLUMEDOWN: 'Volume down',
	MUTE: 'Mute',
	MICMUTE: 'Microphone mute',
	PLAYPAUSE: 'Play or pause',
	NEXTSONG: 'Next track',
	PREVIOUSSONG: 'Previous track',
	STOPCD: 'Stop',
	BRIGHTNESSUP: 'Brightness up',
	BRIGHTNESSDOWN: 'Brightness down',
	SYSRQ: 'Print Screen',
	ESC: 'Esc',
	BACKSPACE: 'Backspace',
	DELETE: 'Delete',
	ENTER: 'Enter',
	SPACE: 'Space',
	TAB: 'Tab',
	DOT: '.',
	COMMA: ',',
	SLASH: '/',
	MINUS: '-',
	EQUAL: '=',
	CALC: 'Calculator',
	ZOOM: 'Zoom'
};

function keyWord(evdev: string): string {
	const k = evdev.toUpperCase().replace(/^(KEY|BTN)_/, '');
	if (KEY_WORDS[k]) return KEY_WORDS[k];
	if (/^F\d+$/.test(k) || k.length === 1) return k;
	return k.charAt(0) + k.slice(1).toLowerCase();
}

const MOUSE_WORDS: Record<string, string> = {
	left: 'Left click',
	right: 'Right click',
	middle: 'Middle click',
	back: 'Back',
	forward: 'Forward'
};

/** A short, plain description of an action string. */
export function describe(action: string | undefined | null): string {
	if (!action || action === 'none') return 'Nothing';
	if (action === 'smartshift:toggle') return 'Switch the wheel between ratchet and free spin';
	const [kind, arg = ''] = action.split(/:(.*)/s);
	switch (kind) {
		case 'key':
			return arg.split('+').map(keyWord).join(' + ');
		case 'press':
			return `${arg.split('+').map(keyWord).join(' + ')}, pressed once`;
		case 'mouse':
			return MOUSE_WORDS[arg] ?? arg;
		case 'shell':
			return `Run ${arg}`;
		case 'haptic':
			return `Buzz: ${waveformName(arg).toLowerCase()}`;
		case 'dpi':
			return `Pointer at ${arg} DPI`;
		case 'dpi-shift':
			return `${arg} DPI while held`;
		case 'dpi-cycle':
			return `Cycle ${arg.split(',').join(', ')} DPI`;
		case 'host':
			return `Switch to computer ${arg}`;
		default:
			return action;
	}
}

/** A one-line summary of what a button does. */
export function summarize(b: BindingView | undefined | null, button: ButtonId): string {
	if (!b) return NORMALLY[button];
	if (b.kind === 'direct') return describe(b.action);
	const g = b.gestures;
	if (g.toggle) return `${describe(g.toggle)}, toggled on and off`;
	const others = Object.keys(g).filter((k) => k !== 'tap').length;
	const more = others === 1 ? 'one gesture' : `${others} gestures`;
	if (g.tap) return others ? `${describe(g.tap)} on click, plus ${more}` : `${describe(g.tap)} on click`;
	return others ? `Nothing on click, ${more}` : 'Nothing';
}

export function waveformName(w: string): string {
	const s = w.replace(/-/g, ' ');
	return s.charAt(0).toUpperCase() + s.slice(1);
}

/** The kinds of action offered in the picker, in the order they are listed. */
export const KINDS = [
	{ id: 'none', label: 'Nothing' },
	{ id: 'key', label: 'Keyboard shortcut' },
	{ id: 'media', label: 'Media or volume key' },
	{ id: 'mouse', label: 'Mouse button' },
	{ id: 'dpi-shift', label: 'Slow the pointer while held' },
	{ id: 'dpi-cycle', label: 'Cycle pointer speeds' },
	{ id: 'dpi', label: 'Set pointer speed' },
	{ id: 'smartshift', label: 'Switch wheel ratchet and free spin' },
	{ id: 'host', label: 'Switch to another computer' },
	{ id: 'haptic', label: 'Buzz the mouse' },
	{ id: 'shell', label: 'Run a command' }
] as const;

export type Kind = (typeof KINDS)[number]['id'];

export const MEDIA = [
	{ id: 'volumeup', label: 'Volume up', evdev: 'KEY_VOLUMEUP' },
	{ id: 'volumedown', label: 'Volume down', evdev: 'KEY_VOLUMEDOWN' },
	{ id: 'mute', label: 'Mute', evdev: 'KEY_MUTE' },
	{ id: 'micmute', label: 'Microphone mute', evdev: 'KEY_MICMUTE' },
	{ id: 'playpause', label: 'Play or pause', evdev: 'KEY_PLAYPAUSE' },
	{ id: 'next', label: 'Next track', evdev: 'KEY_NEXTSONG' },
	{ id: 'previous', label: 'Previous track', evdev: 'KEY_PREVIOUSSONG' }
] as const;

/** Splits an action string into the picker's kind and value. */
export function split(action: string | undefined | null): { kind: Kind; value: string } {
	if (!action || action === 'none') return { kind: 'none', value: '' };
	if (action === 'smartshift:toggle') return { kind: 'smartshift', value: '' };
	const [kind, arg = ''] = action.split(/:(.*)/s);
	if (kind === 'key' || kind === 'press') {
		const media = MEDIA.find((m) => m.evdev === arg);
		if (media) return { kind: 'media', value: media.id };
		return { kind: 'key', value: arg };
	}
	return { kind: kind as Kind, value: arg };
}

/** Builds an action string from the picker's kind and value, or null when incomplete. */
export function join(kind: Kind, value: string): string | null {
	const v = value.trim();
	switch (kind) {
		case 'none':
			return 'none';
		case 'smartshift':
			return 'smartshift:toggle';
		case 'media':
			return v ? `key:${v}` : null;
		default:
			return v ? `${kind}:${v}` : null;
	}
}

const CODE_NAMES: Record<string, string> = {
	Space: 'space',
	Enter: 'enter',
	NumpadEnter: 'enter',
	Escape: 'esc',
	Tab: 'tab',
	Backspace: 'backspace',
	Delete: 'delete',
	Insert: 'insert',
	Home: 'home',
	End: 'end',
	PageUp: 'pageup',
	PageDown: 'pagedown',
	ArrowUp: 'up',
	ArrowDown: 'down',
	ArrowLeft: 'left',
	ArrowRight: 'right',
	PrintScreen: 'print',
	Minus: 'minus',
	Equal: 'equal',
	Comma: 'comma',
	Period: 'dot',
	Slash: 'slash',
	Semicolon: 'semicolon',
	Quote: 'apostrophe',
	Backquote: 'grave',
	BracketLeft: 'leftbrace',
	BracketRight: 'rightbrace',
	Backslash: 'backslash'
};

const MODIFIER_CODES: Record<string, string> = {
	ControlLeft: 'ctrl',
	ControlRight: 'ctrl',
	ShiftLeft: 'shift',
	ShiftRight: 'shift',
	AltLeft: 'alt',
	AltRight: 'altgr',
	MetaLeft: 'super',
	MetaRight: 'super'
};

/** The key name for a physical key, or null for keys this program cannot send. */
export function keyName(code: string): string | null {
	if (/^Key[A-Z]$/.test(code)) return code.slice(3).toLowerCase();
	if (/^Digit\d$/.test(code)) return code.slice(5);
	if (/^F\d{1,2}$/.test(code)) return code.toLowerCase();
	return CODE_NAMES[code] ?? null;
}

export function modifierName(code: string): string | null {
	return MODIFIER_CODES[code] ?? null;
}

/** The modifiers a key event says are held, AltGr told apart from Alt. */
export function eventModifiers(e: KeyboardEvent): string[] {
	const altgr = e.getModifierState('AltGraph');
	return [
		e.ctrlKey && !altgr && 'ctrl',
		e.shiftKey && 'shift',
		e.altKey && !altgr && 'alt',
		altgr && 'altgr',
		e.metaKey && 'super'
	].filter(Boolean) as string[];
}
