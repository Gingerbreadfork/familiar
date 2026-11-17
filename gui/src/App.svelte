<script lang="ts">
	import Stage from './lib/drawing/Stage.svelte';
	import AboutSheet from './lib/sheets/AboutSheet.svelte';
	import BatterySheet from './lib/sheets/BatterySheet.svelte';
	import ButtonSheet from './lib/sheets/ButtonSheet.svelte';
	import ComputersSheet from './lib/sheets/ComputersSheet.svelte';
	import HapticSheet from './lib/sheets/HapticSheet.svelte';
	import Overview from './lib/sheets/Overview.svelte';
	import SensorSheet from './lib/sheets/SensorSheet.svelte';
	import ThumbSheet from './lib/sheets/ThumbSheet.svelte';
	import WheelSheet from './lib/sheets/WheelSheet.svelte';
	import { app } from './lib/state.svelte';
	import mark from './assets/familiar.svg';

	let d = $derived(app.device);
	let reachable = $derived(!!app.snapshot && !!d);
	let charging = $derived(app.charging);

	app.start();
</script>

<div class="window">
	<header class="strip">
		<button class="brand" onclick={() => app.open(null)}>
			<img src={mark} alt="" width="22" height="22" />
			Familiar
		</button>

		{#if d}
			<div class="device" aria-live="polite">
				<span class={['led', d.online && 'on']} aria-hidden="true"></span>
				<span class="name">{d.name}</span>
				<span class="state">{d.online ? 'Connected' : 'Asleep'}</span>
				{#if d.online && d.batteryPercent}
					<button class="battery" onclick={() => app.open('power')} aria-label={`Battery ${d.batteryPercent}%`}>
						<span class={['cell', d.batteryPercent <= 15 && 'low']} aria-hidden="true">
							<span class="level" style:width={`${d.batteryPercent}%`}></span>
						</span>
						{d.batteryPercent}%{#if charging}, charging{/if}
					</button>
				{/if}
			</div>
		{/if}

		<button class={['about', app.about && 'on']} onclick={() => (app.about = !app.about)}>About</button>
	</header>

	<main class={['bench', !reachable && 'resting']}>
		<Stage live={reachable && app.loaded} />

		<aside class="sheet">
			{#if app.problem}
				<p class="problem" role="alert">
					{app.problem}
					<button onclick={() => (app.problem = null)}>Dismiss</button>
				</p>
			{/if}
			{#if app.configProblem}
				<p class="problem" role="alert">
					The settings file has a mistake, so changes cannot be saved until it is fixed: {app.configProblem}
				</p>
			{/if}
			{#if d?.applyErrors.length}
				<div class="problem">
					<p>Some settings did not reach the mouse:</p>
					<ul>
						{#each d.applyErrors as e (e)}<li>{e}</li>{/each}
					</ul>
				</div>
			{/if}

			{#if !app.loaded}
				<!-- Nothing until the first answer, so the window does not flash a warning. -->
			{:else if app.about}
				<AboutSheet />
			{:else if !app.snapshot}
				<div class="empty">
					<h1>Familiar is not running</h1>
					<p>The background service keeps your mouse set up. Start it and this window picks it up straight away.</p>
					<pre><code>systemctl --user start familiard</code></pre>
				</div>
			{:else if !d}
				<div class="empty">
					<h1>No mouse yet</h1>
					<p>Switch the mouse on, or move it to wake it. It shows up here as soon as it connects.</p>
				</div>
			{:else if app.selected === 'wheel'}
				<WheelSheet />
			{:else if app.selected === 'thumb-wheel'}
				<ThumbSheet />
			{:else if app.selected === 'haptic'}
				<HapticSheet />
			{:else if app.selected === 'sensor'}
				<SensorSheet />
			{:else if app.selected === 'easy-switch'}
				<ComputersSheet />
			{:else if app.selected === 'power'}
				<BatterySheet />
			{:else if app.selected === 'smartshift' || app.selected === 'gesture' || app.selected === 'forward' || app.selected === 'back'}
				{#key app.selected}
					<ButtonSheet button={app.selected} />
				{/key}
			{:else}
				<Overview />
			{/if}
		</aside>
	</main>
</div>

<style>
	.window {
		display: grid;
		grid-template-rows: auto 1fr;
		height: 100%;
	}

	.strip {
		display: flex;
		align-items: center;
		gap: 1.25rem;
		height: 3.25rem;
		padding: 0 1rem 0 1.1rem;
		border-bottom: 1px solid var(--line);
	}

	.brand {
		display: inline-flex;
		align-items: center;
		gap: 0.55rem;
		padding: 0.25rem 0.4rem 0.25rem 0.2rem;
		border: 0;
		border-radius: 8px;
		background: none;
		font-family: var(--narrow);
		font-weight: 600;
		font-size: 1.1rem;
		cursor: pointer;
	}

	.brand img {
		border-radius: 6px;
	}

	.device {
		display: flex;
		align-items: center;
		gap: 0.55rem;
		margin-left: auto;
		font-size: var(--t-sm);
	}

	.led {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--line-strong);
	}

	.led.on {
		background: var(--led);
		box-shadow: 0 0 7px var(--led-glow);
	}

	.name {
		font-family: var(--narrow);
		font-weight: 600;
		font-size: var(--t-body);
	}

	.state {
		color: var(--muted);
	}

	.battery {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		margin-left: 0.5rem;
		padding: 0.2rem 0.45rem;
		border: 0;
		border-radius: 6px;
		background: none;
		color: var(--muted);
		cursor: pointer;
	}

	.battery:hover,
	.about:hover {
		color: var(--ink);
	}

	/* A battery drawn as a cell with a terminal nub. */
	.cell {
		position: relative;
		width: 22px;
		height: 11px;
		padding: 1.5px;
		border: 1.2px solid currentColor;
		border-radius: 3px;
	}

	.cell::after {
		content: '';
		position: absolute;
		right: -4px;
		top: 2.5px;
		width: 2px;
		height: 4px;
		border-radius: 0 1px 1px 0;
		background: currentColor;
	}

	.level {
		display: block;
		height: 100%;
		border-radius: 1px;
		background: var(--ink);
	}

	.cell.low .level {
		background: var(--danger);
	}

	.about {
		padding: 0.3rem 0.55rem;
		border: 0;
		border-radius: 6px;
		background: none;
		color: var(--muted);
		font-size: var(--t-sm);
		cursor: pointer;
	}

	.device + .about {
		margin-left: 0.25rem;
	}

	.strip > .about:only-of-type,
	.brand + .about {
		margin-left: auto;
	}

	.about.on {
		color: var(--ink);
		background: var(--surface);
		box-shadow: var(--press);
	}

	.bench {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 26rem;
		min-height: 0;
		/* A soft pool of light under the mouse, like a desk lamp. */
		background: radial-gradient(ellipse 60% 70% at 38% 48%, var(--raised), transparent 70%);
	}

	.sheet {
		overflow-y: auto;
		padding: 1.6rem 1.75rem 2.5rem;
		background: var(--surface);
		border-left: 1px solid var(--line);
		display: grid;
		gap: 1rem;
		align-content: start;
	}

	.problem {
		color: var(--danger);
		border-left: 3px solid var(--danger);
		padding: 0.2rem 0 0.2rem 0.75rem;
		font-size: var(--t-sm);
	}

	.problem ul {
		margin: 0.25rem 0 0;
		padding-left: 1.1rem;
	}

	.problem button {
		margin-left: 0.4rem;
		background: none;
		border: 0;
		color: inherit;
		text-decoration: underline;
		cursor: pointer;
	}

	.empty {
		display: grid;
		gap: 0.8rem;
		margin-top: 10vh;
	}

	.empty h1 {
		font-size: var(--t-h1);
	}

	.empty p {
		color: var(--muted);
	}

	pre {
		margin: 0;
		padding: 0.7rem 0.9rem;
		border-radius: 8px;
		background: var(--shell);
		box-shadow: var(--press);
		user-select: text;
	}

	@media (max-width: 1000px) {
		.bench {
			grid-template-columns: minmax(0, 1fr) 22rem;
		}
	}
</style>
