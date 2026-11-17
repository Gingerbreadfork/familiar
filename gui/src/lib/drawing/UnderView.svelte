<script lang="ts" module>
	import type { PartId } from '../actions';

	export const UNDER_CENTERS: Partial<Record<PartId, [number, number]>> = {
		sensor: [302, 612],
		'easy-switch': [304, 800],
		power: [156, 610]
	};
</script>

<script lang="ts">
	import { app } from '../state.svelte';
	import { UNDER } from './geometry';

	interface Props {
		live?: boolean;
	}

	let { live = true }: Props = $props();

	const SCREWS: [number, number][] = [
		[303, 103],
		[80, 517],
		[587, 530],
		[66, 722],
		[576, 872],
		[303, 970]
	];
	const CHANNELS: { n: number; x: number; y: number }[] = [
		{ n: 1, x: 258, y: 772 },
		{ n: 2, x: 303, y: 757 },
		{ n: 3, x: 348, y: 772 }
	];

	const PICKS: { id: PartId; d: string }[] = [
		{ id: 'sensor', d: UNDER.ring },
		{ id: 'easy-switch', d: 'M 236 740 C 236 724 372 724 372 740 L 372 790 C 372 870 236 870 236 790 Z' },
		{ id: 'power', d: 'M 126 574 C 126 548 186 548 186 574 L 186 646 C 186 672 126 672 126 646 Z' }
	];

	const lit = (id: PartId) => app.hovered === id || app.selected === id;
	let current = $derived(app.ready ? (app.hosts?.current ?? 0) : 0);
	let on = $derived(app.device?.online ?? false);
</script>

<svg viewBox="-40 -30 852 1190" class={['under', live && 'live']} aria-hidden="true">
	<defs>
		<linearGradient id="u-body" x1="0.9" y1="0" x2="0.1" y2="1">
			<stop offset="0" style:stop-color="var(--m-mid)" />
			<stop offset="1" style:stop-color="var(--m-under)" />
		</linearGradient>
		<linearGradient id="u-foot" x1="0" y1="0" x2="1" y2="1">
			<stop offset="0" style:stop-color="var(--m-foot)" />
			<stop offset="1" style:stop-color="var(--m-seam)" />
		</linearGradient>
		<radialGradient id="u-lens" cx="0.4" cy="0.35" r="0.7">
			<stop offset="0" style:stop-color="var(--m-steel-3)" />
			<stop offset="1" style:stop-color="var(--m-seam)" />
		</radialGradient>
		<filter id="u-blur" x="-20%" y="-20%" width="140%" height="140%">
			<feGaussianBlur stdDeviation="20" />
		</filter>
		<clipPath id="u-clip"><path d={UNDER.body} /></clipPath>
	</defs>

	<path class="shadow" d={UNDER.body} transform="translate(-14 24)" filter="url(#u-blur)" />
	<path d={UNDER.body} fill="url(#u-body)" />
	<g clip-path="url(#u-clip)">
		<path class="rim" d={UNDER.body} />
	</g>
	<path class="edge" d={UNDER.body} />

	<!-- Battery door at the front. -->
	<rect class="door" x="238" y="60" width="134" height="80" rx="16" />

	{#each [UNDER.footTop, UNDER.footSide, UNDER.footBottom] as d (d)}
		<path {d} fill="url(#u-foot)" />
	{/each}

	{#each SCREWS as [x, y] (`${x},${y}`)}
		<circle class="screw" cx={x} cy={y} r="14" />
		<path class="screw-x" d={`M ${x - 6} ${y} H ${x + 6} M ${x} ${y - 6} V ${y + 6}`} />
	{/each}

	<text class="model" x="303" y="442" text-anchor="middle">{app.device?.name ?? 'MX Master 4'}</text>

	<!-- Sensor. -->
	<path class="ring" d={UNDER.ring} />
	<ellipse class="window" cx="302" cy="612" rx="42" ry="52" />
	<ellipse cx="302" cy="612" rx="20" ry="28" fill="url(#u-lens)" />
	<ellipse class="glint" cx="295" cy="600" rx="5" ry="7" />

	<!-- Power switch: the green shows when it is on. -->
	<rect class="track" x="136" y="574" width="40" height="72" rx="20" />
	<rect class={['green', on && 'on']} x="140" y="612" width="32" height="30" rx="15" />
	<rect class="slider" x="140" y={on ? 578 : 612} width="32" height="32" rx="16" />

	<!-- Easy-Switch: a light for each computer, lit for the current one. -->
	{#each CHANNELS as c (c.n)}
		<text class="channel" x={c.x} y={c.y - 22} text-anchor="middle">{c.n}</text>
		<circle class={['chan', current === c.n && 'here']} cx={c.x} cy={c.y} r="6" />
	{/each}
	<circle class="es-well" cx="304" cy="826" r="31" />
	<circle class="es" cx="304" cy="826" r="22" />

	{#if live}
		{#each PICKS as p (p.id)}
			<path
				class={['pick', lit(p.id) && 'lit', app.selected === p.id && 'chosen']}
				d={p.d}
				role="presentation"
				onpointerenter={() => (app.hovered = p.id)}
				onpointerleave={() => (app.hovered = null)}
				onclick={() => app.open(p.id)}
			/>
		{/each}
	{/if}
</svg>

<style>
	svg {
		display: block;
		width: 100%;
		height: 100%;
		overflow: visible;
	}

	.shadow {
		fill: var(--m-shadow);
		opacity: 0.3;
	}

	.rim {
		fill: none;
		stroke: var(--m-hi);
		stroke-width: 30;
		opacity: 0.6;
	}

	.edge {
		fill: none;
		stroke: var(--m-edge);
		stroke-width: 1.6;
	}

	.door {
		fill: none;
		stroke: var(--m-edge);
		stroke-width: 2;
	}

	.screw {
		fill: var(--m-foot);
	}

	.screw-x {
		stroke: var(--m-steel-3);
		stroke-width: 2.5;
	}

	.model {
		font-family: var(--sans);
		font-size: 38px;
		font-weight: 400;
		fill: var(--m-steel-3);
	}

	.ring {
		fill: var(--m-foot);
	}

	.window {
		fill: var(--m-steel-2);
		stroke: var(--m-steel-3);
		stroke-width: 3;
	}

	.glint {
		fill: var(--m-glint);
		opacity: 0.5;
	}

	.track {
		fill: var(--m-foot);
	}

	.green {
		fill: var(--m-steel-3);
	}

	.green.on {
		fill: var(--led);
	}

	.slider {
		fill: var(--m-hi);
		stroke: var(--m-edge);
		stroke-width: 1.5;
		transition: y 0.25s ease;
	}

	.channel {
		font-family: var(--narrow);
		font-size: 24px;
		font-weight: 500;
		fill: var(--m-steel-3);
	}

	.chan {
		fill: var(--m-steel-3);
		transition: fill 0.3s;
	}

	.chan.here {
		fill: #fff;
		filter: drop-shadow(0 0 7px rgb(255 255 255 / 0.9));
	}

	.es-well {
		fill: var(--m-lo);
	}

	.es {
		fill: var(--m-hi);
		stroke: var(--m-edge);
		stroke-width: 2;
	}

	.pick {
		fill: transparent;
		stroke: transparent;
		stroke-width: 3;
		cursor: pointer;
		transition:
			fill 0.15s,
			stroke 0.15s;
	}

	.pick.lit {
		fill: var(--m-pick);
		stroke: var(--ink);
	}

	.pick.chosen {
		stroke-width: 4;
	}
</style>
