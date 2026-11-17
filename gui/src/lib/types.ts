export interface DeviceSnapshot {
	path: string;
	name: string;
	unitId: string;
	model: string;
	online: boolean;
	batteryPercent: number;
	batteryStatus: string;
	settings: Record<string, string>;
	configured: Record<string, string>;
	applyErrors: string[];
	lastApplied: number;
	reconnectMs: number;
}

export interface Snapshot {
	version: string;
	configPath: string;
	conflicts: string[];
	devices: DeviceSnapshot[];
}

export type BindingView =
	| { kind: 'direct'; action: string }
	| { kind: 'gestures'; gestures: Record<string, string> };

export interface ThumbView {
	mode: string;
	speed: number;
	tap: string | null;
}

export interface ConfigView {
	path: string;
	buttons: Record<string, BindingView>;
	smooth: boolean;
	speed: number;
	thumb: ThumbView | null;
}

export interface Host {
	channel: number;
	paired: boolean;
	name: string;
}

export interface Hosts {
	current: number;
	hosts: Host[];
}
