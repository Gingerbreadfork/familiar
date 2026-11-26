// Looking for a newer release and installing it, for copies that came from a package.

import { inApp } from './api';

/** How long the feed may take to answer, and the package to arrive, in milliseconds. */
const CHECK_TIMEOUT = 30_000;
const DOWNLOAD_TIMEOUT = 15 * 60_000;

export interface UpdateHandle {
	version: string;
	/** Downloads and installs it, reporting download progress from 0 to 1. */
	install(onProgress: (fraction: number) => void): Promise<void>;
}

/** The newer release, if there is one. */
export async function checkForUpdate(): Promise<UpdateHandle | null> {
	if (!inApp) return null;
	const { check } = await import('@tauri-apps/plugin-updater');
	const update = await check({ timeout: CHECK_TIMEOUT });
	if (!update) return null;
	return {
		version: update.version,
		async install(onProgress) {
			let total = 0;
			let got = 0;
			await update.downloadAndInstall(
				(event) => {
					if (event.event === 'Started') {
						total = event.data.contentLength ?? 0;
					} else if (event.event === 'Progress') {
						got += event.data.chunkLength;
						if (total > 0) onProgress(Math.min(1, got / total));
					} else if (event.event === 'Finished') {
						onProgress(1);
					}
				},
				{ timeout: DOWNLOAD_TIMEOUT }
			);
		}
	};
}

/** Closes the window and opens it again, so the new version is the one running. */
export async function relaunch(): Promise<void> {
	if (!inApp) return;
	const { relaunch } = await import('@tauri-apps/plugin-process');
	await relaunch();
}

/** The version of the window itself. */
export async function appVersion(): Promise<string> {
	if (!inApp) return '';
	const { getVersion } = await import('@tauri-apps/api/app');
	return getVersion();
}
