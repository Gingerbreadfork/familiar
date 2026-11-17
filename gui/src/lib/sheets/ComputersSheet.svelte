<script lang="ts">
	import Sheet from './Sheet.svelte';
	import { api } from '../api';
	import { app } from '../state.svelte';

	let path = $derived(app.ready ? (app.device?.path ?? null) : null);

	async function rename(channel: number, name: string) {
		if (!path) return;
		const p = path;
		let stored = name;
		const ok = await app.attempt(async () => (stored = await api.renameHost(p, channel, name)), 'easy-switch');
		if (ok && app.hosts) {
			app.hosts = {
				...app.hosts,
				hosts: app.hosts.hosts.map((h) => (h.channel === channel ? { ...h, name: stored } : h))
			};
		}
	}

	async function switchTo(channel: number) {
		if (path) await app.attempt(() => api.switchHost(path, channel), 'easy-switch');
	}
</script>

<Sheet
	id="easy-switch"
	title="Computers"
	where="The mouse remembers three computers. The button underneath moves between them, and its lights show which one it is on."
>
	{#if !path}
		<p class="quiet">The mouse is asleep. Move it to see its computers.</p>
	{:else if app.hosts}
		<ol class="hosts">
			{#each app.hosts.hosts as h (h.channel)}
				{@const here = h.channel === app.hosts.current}
				<li class={[here && 'here']}>
					<span class="light" aria-hidden="true"><span class="lamp"></span>{h.channel}</span>
					<div class="about">
						<input
							aria-label={`Name of computer ${h.channel}`}
							value={h.name}
							placeholder={h.paired ? 'Unnamed' : 'Not paired yet'}
							maxlength="24"
							disabled={!h.paired}
							onchange={(e) => rename(h.channel, e.currentTarget.value)}
						/>
						<span class="state">
							{#if here}
								This computer
							{:else if h.paired}
								Paired
							{:else}
								Press the button underneath until light {h.channel} shows, hold it until it blinks, then pair from
								that computer.
							{/if}
						</span>
					</div>
					{#if !here && h.paired}
						<button class="cap small" onclick={() => switchTo(h.channel)}>Switch to it</button>
					{/if}
				</li>
			{/each}
		</ol>
		<p class="quiet">Switching sends the mouse to that computer straight away.</p>
	{/if}
</Sheet>

<style>
	.quiet {
		color: var(--muted);
		font-size: var(--t-sm);
	}

	.hosts {
		list-style: none;
		margin: 0;
		padding: 0;
		display: grid;
		gap: 0.5rem;
	}

	li {
		display: grid;
		grid-template-columns: auto 1fr auto;
		gap: 0.9rem;
		align-items: center;
		padding: 0.75rem 0.85rem;
		border-radius: 10px;
		background: var(--shell);
		box-shadow: var(--press);
	}

	li.here {
		background: var(--raised);
		box-shadow: var(--shadow);
	}

	/* The channel light and its number, as printed underneath. */
	.light {
		display: grid;
		justify-items: center;
		gap: 0.2rem;
		width: 1.5rem;
		font-family: var(--narrow);
		font-weight: 600;
		font-size: var(--t-sm);
		color: var(--muted);
		order: -1;
	}

	.lamp {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--line-strong);
	}

	.here .lamp {
		background: #fff;
		box-shadow:
			0 0 0 1px var(--line-strong),
			0 0 8px rgb(255 255 255 / 0.9);
	}

	.here .light {
		color: var(--ink);
	}

	.about {
		display: grid;
		gap: 0.1rem;
		min-width: 0;
	}

	input {
		font-family: var(--narrow);
		font-weight: 600;
		font-size: var(--t-lead);
		border: 0;
		box-shadow: none;
		background: none;
		padding: 0;
		min-height: 0;
		width: 100%;
		border-bottom: 1px dashed transparent;
		border-radius: 0;
	}

	input:hover:not(:disabled),
	input:focus {
		border-bottom-color: var(--line-strong);
	}

	input:disabled {
		color: var(--muted);
	}

	.state {
		font-size: var(--t-sm);
		color: var(--muted);
	}
</style>
