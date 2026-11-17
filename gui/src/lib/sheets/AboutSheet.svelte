<script lang="ts">
	import Sheet from './Sheet.svelte';
	import { api } from '../api';
	import { app } from '../state.svelte';

	function ago(unix: number): string {
		if (!unix) return 'not yet';
		const s = Math.max(0, Math.floor(Date.now() / 1000) - unix);
		if (s < 60) return 'just now';
		if (s < 3600) return `${Math.floor(s / 60)} min ago`;
		return `${Math.floor(s / 3600)} h ago`;
	}
</script>

<Sheet
	title="Familiar"
	where="Looks after your mouse in the background. Settings live in a plain text file, nothing leaves this computer, and the mouse keeps working if Familiar stops."
>
	{#if app.snapshot}
		<dl class="group">
			<dt>Service</dt>
			<dd>familiard {app.snapshot.version}</dd>
			<dt>Settings file</dt>
			<dd><code>{app.snapshot.configPath}</code></dd>
			{#if app.device}
				<dt>Last applied</dt>
				<dd>
					{ago(app.device.lastApplied)}{#if app.device.reconnectMs}, {app.device.reconnectMs} ms after waking{/if}
				</dd>
			{/if}
		</dl>

		{#if app.snapshot.conflicts.length}
			<p class="warn">
				{app.snapshot.conflicts.join(' and ')}
				{app.snapshot.conflicts.length === 1 ? 'is' : 'are'} also running and may change your settings back. Quit
				{app.snapshot.conflicts.length === 1 ? 'it' : 'them'} to let Familiar look after the mouse.
			</p>
		{/if}

		<div>
			<button class="cap" onclick={() => app.attempt(() => api.reload())}>Apply the settings file again</button>
		</div>
	{/if}

	<section class="group">
		<h2 class="section-title">If something is stuck</h2>
		<p class="quiet">
			<code>familiar undivert</code> puts every button back as it came. <code>familiar diagnostics</code> writes a report
			you can read and attach to a bug report. It is never sent anywhere.
		</p>
	</section>

	<p class="small">Free software under the MIT license. Not made by or affiliated with Logitech.</p>
</Sheet>

<style>
	dl {
		grid-template-columns: max-content 1fr;
		gap: 0.5rem 1.25rem;
		margin: 0;
	}

	dt {
		color: var(--muted);
	}

	dd {
		margin: 0;
		min-width: 0;
	}

	code {
		font-size: 0.9em;
		overflow-wrap: anywhere;
		user-select: text;
	}

	.warn {
		color: var(--danger);
	}

	.quiet,
	.small {
		color: var(--muted);
	}

	.small {
		font-size: var(--t-sm);
	}
</style>
