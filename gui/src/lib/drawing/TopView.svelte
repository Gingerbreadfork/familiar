<script lang="ts" module>
	import type { PartId } from '../actions';

	// Hand-drawn parts, in the same 707 by 1104 space as the traced outlines.
	export const SHAPES = {
		slot: 'M 449 167 C 451 159 474 161 506 177 C 529 206 552 268 561 332 C 563 345 556 351 546 348 C 525 344 509 340 502 330 C 486 280 466 225 451 182 C 449 176 448 171 449 167 Z',
		face: 'M 450 168 C 466 170 476 179 484 190 C 503 222 514 276 516 334 L 500 334 C 484 283 464 226 448 180 Z',
		gesture: 'M 264.3 390.7 C 262.2 380.9 287.6 375.5 289.7 385.3 L 307.7 471.3 C 309.8 481.1 284.4 486.5 282.3 476.7 Z',
		forward: 'M 297 500 C 292 520 300 585 312 598 C 322 606 338 600 340 588 C 342 560 335 515 325 500 C 316 490 300 492 297 500 Z',
		back: 'M 315 603 C 312 625 322 690 336 708 C 346 716 360 708 362 696 C 364 668 356 625 345 608 C 337 598 318 596 315 603 Z',
		thumb: 'M 335 480 C 356 474 386 471 406 474 L 450 648 C 425 656 396 662 371 666 Z',
		nub: 'M 371 662 C 373 700 386 735 410 745 C 431 751 452 739 456 713 C 459 691 455 668 450 648 C 425 656 396 662 371 666 Z',
		housing:
			'M 327 482 C 350 466 392 462 413 468 C 432 540 452 610 461 660 C 469 702 463 742 438 755 C 412 766 381 752 368 722 C 352 650 337 560 327 482 Z'
	};

	/** Where to anchor a part's label and ripple. */
	export const CENTERS: Partial<Record<PartId, [number, number]>> = {
		wheel: [506, 256],
		smartshift: [572, 470],
		haptic: [190, 690],
		'thumb-wheel': [392, 568],
		gesture: [285, 430],
		forward: [318, 548],
		back: [338, 655]
	};
</script>

<script lang="ts">
	import { app } from '../state.svelte';
	import { TOP } from './geometry';

	interface Props {
		/** Parts that answer the pointer. False draws the mouse as a still picture. */
		live?: boolean;
	}

	let { live = true }: Props = $props();

	// The two knurled rims of the main wheel, drawn as dashed strokes inside the slot.
	const RIMS = [
		{ d: 'M 474 182 C 498 208 512 260 516 340', w: 24 },
		{ d: 'M 498 186 C 524 213 539 265 543 340', w: 21 }
	];

	// Knurls on the thumb wheel run along its axis, so they are spaced across its width.
	const lerp = (a: [number, number], b: [number, number], t: number): [number, number] => [
		a[0] + (b[0] - a[0]) * t,
		a[1] + (b[1] - a[1]) * t
	];
	const TL: [number, number] = [335, 480];
	const TR: [number, number] = [406, 474];
	const BL: [number, number] = [371, 666];
	const BR: [number, number] = [450, 648];
	const GROOVE = 0.46;
	const KNURLS = Array.from({ length: 15 }, (_, i) => {
		const t = 0.06 + (i * 0.88) / 14;
		const top = lerp(TL, TR, t);
		const bottom = lerp(BL, BR, t);
		const g1 = lerp(top, bottom, GROOVE - 0.03);
		const g2 = lerp(top, bottom, GROOVE + 0.05);
		return { top, g1, g2, bottom };
	});
	const grooveA = lerp(TL, BL, GROOVE);
	const grooveB = lerp(TR, BR, GROOVE);

	// Concentric ripples on the haptic panel.
	const RINGS = Array.from({ length: 26 }, (_, i) => 22 + i * 13);

	const PICKS: { id: PartId; d: string; pad?: boolean }[] = [
		{ id: 'haptic', d: TOP.panel },
		{ id: 'wheel', d: SHAPES.slot, pad: true },
		{ id: 'smartshift', d: TOP.shift, pad: true },
		{ id: 'thumb-wheel', d: SHAPES.housing },
		{ id: 'gesture', d: SHAPES.gesture, pad: true },
		{ id: 'forward', d: SHAPES.forward, pad: true },
		{ id: 'back', d: SHAPES.back, pad: true }
	];

	const lit = (id: PartId) => app.hovered === id || app.selected === id;
	const pulsed = (id: PartId) =>
		app.pulse.control === id || (id === 'wheel' && app.pulse.control === 'middle');

	let led = $derived(app.device?.online ? (app.device.batteryPercent > 0 && app.device.batteryPercent <= 15 ? 'low' : 'on') : 'off');
</script>

<svg viewBox="-40 -30 790 1190" class={['top', live && 'live']} aria-hidden="true">
	<defs>
		<linearGradient id="t-body" x1="0.1" y1="0" x2="0.9" y2="1">
			<stop offset="0" style:stop-color="var(--m-hi)" />
			<stop offset="0.5" style:stop-color="var(--m-mid)" />
			<stop offset="1" style:stop-color="var(--m-lo)" />
		</linearGradient>
		<radialGradient id="t-sheen" cx="0.5" cy="0.18" r="0.5">
			<stop offset="0" style:stop-color="var(--m-glint)" stop-opacity="0.7" />
			<stop offset="1" style:stop-color="var(--m-glint)" stop-opacity="0" />
		</radialGradient>
		<radialGradient id="t-panel" cx="0.6" cy="0.25" r="0.85">
			<stop offset="0" style:stop-color="var(--m-hi)" />
			<stop offset="1" style:stop-color="var(--m-panel)" />
		</radialGradient>
		<linearGradient id="t-pill" x1="0" y1="0" x2="1" y2="0.2">
			<stop offset="0" style:stop-color="var(--m-hi)" />
			<stop offset="1" style:stop-color="var(--m-lo)" />
		</linearGradient>
		<linearGradient id="t-steel" x1="0" y1="0.55" x2="1" y2="0.45">
			<stop offset="0" style:stop-color="var(--m-steel-4)" />
			<stop offset="0.22" style:stop-color="var(--m-steel-2)" />
			<stop offset="0.4" style:stop-color="var(--m-steel-1)" />
			<stop offset="0.62" style:stop-color="var(--m-steel-2)" />
			<stop offset="0.86" style:stop-color="var(--m-steel-3)" />
			<stop offset="1" style:stop-color="var(--m-steel-4)" />
		</linearGradient>
		<linearGradient id="t-face" x1="0" y1="0" x2="1" y2="0.3">
			<stop offset="0" style:stop-color="var(--m-steel-1)" />
			<stop offset="0.55" style:stop-color="var(--m-steel-2)" />
			<stop offset="1" style:stop-color="var(--m-steel-4)" />
		</linearGradient>
		<filter id="t-blur-l" x="-20%" y="-20%" width="140%" height="140%">
			<feGaussianBlur stdDeviation="22" />
		</filter>
		<filter id="t-blur-m" x="-20%" y="-20%" width="140%" height="140%">
			<feGaussianBlur stdDeviation="9" />
		</filter>
		<filter id="t-blur-s" x="-50%" y="-10%" width="200%" height="120%">
			<feGaussianBlur stdDeviation="8" />
		</filter>
		<clipPath id="t-clip-body"><path d={TOP.body} /></clipPath>
		<clipPath id="t-clip-panel"><path d={TOP.panel} /></clipPath>
		<clipPath id="t-clip-slot"><path d={SHAPES.slot} /></clipPath>
	</defs>

	<!-- Contact shadow on the desk. -->
	<path class="shadow" d={TOP.body} transform="translate(18 26)" filter="url(#t-blur-l)" />

	<path class="body" d={TOP.body} />
	<g clip-path="url(#t-clip-body)">
		<rect x="-40" y="-30" width="790" height="1190" fill="url(#t-sheen)" />
		<!-- Shade the palm side and the far rim, catch light on the near edge. -->
		<path class="rim-dark" d={TOP.body} filter="url(#t-blur-l)" />
		<path class="rim-light" d={TOP.body} transform="translate(-10 -12)" filter="url(#t-blur-m)" />
		<path class="crease" d="M 303 36 C 296 120 296 250 300 352" filter="url(#t-blur-s)" />
	</g>
	<path class="edge" d={TOP.body} />

	<!-- Seams between the main buttons and the palm. -->
	<path class="seam soft" d="M 418 32 C 428 78 440 124 451 164" />
	<path class="seam" d="M 547 347 C 550 367 552 388 554 404" />
	<path class="seam" d="M 412 470 C 413 451 419 438 437 433 C 505 423 600 405 668 389" />
	<path class="seam faint" d="M 303 36 C 296 120 296 250 300 352" />

	<!-- Main wheel. -->
	<path class="slot" d={SHAPES.slot} />
	<g clip-path="url(#t-clip-slot)">
		<path d={SHAPES.face} fill="url(#t-face)" />
		{#each RIMS as r (r.d)}
			<path class="rim-base" d={r.d} style:stroke-width={r.w} />
			<path class="rim-teeth" d={r.d} style:stroke-width={r.w - 4} />
		{/each}
		<path class="rim-groove" d="M 487 185 C 512 211 527 263 530 340" />
	</g>
	<path class="slot-lip" d={SHAPES.slot} />

	<!-- Shift button and status light. -->
	<path class="key" d={TOP.shift} />
	<ellipse class={['led', led]} cx="590" cy="545" rx="5" ry="7" transform="rotate(-14 590 545)" />

	<!-- Haptic panel. -->
	<path d={TOP.panel} fill="url(#t-panel)" />
	<g clip-path="url(#t-clip-panel)">
		{#each RINGS as r (r)}
			<circle class="ring" cx="214" cy="540" {r} />
		{/each}
	</g>
	<path class="panel-seam" d={TOP.panel} />
	{#each { length: 8 }, i (i)}
		<circle
			class="dot"
			cx={214 + 16 * Math.cos((i * Math.PI) / 4)}
			cy={540 + 16 * Math.sin((i * Math.PI) / 4)}
			r="3"
		/>
	{/each}

	<!-- Side buttons. -->
	<path class="pill" d={SHAPES.gesture} />
	<path class="pill" d={SHAPES.forward} />
	<path class="pill" d={SHAPES.back} />

	<!-- Thumb wheel in its housing. -->
	<path class="housing" d={SHAPES.housing} />
	<path class="nub" d={SHAPES.nub} />
	<path d={SHAPES.thumb} fill="url(#t-steel)" />
	{#each KNURLS as k, i (i)}
		<line class="knurl" x1={k.top[0]} y1={k.top[1]} x2={k.g1[0]} y2={k.g1[1]} />
		<line class="knurl" x1={k.g2[0]} y1={k.g2[1]} x2={k.bottom[0]} y2={k.bottom[1]} />
	{/each}
	<path
		class="groove"
		d={`M ${grooveA[0]} ${grooveA[1]} L ${grooveB[0]} ${grooveB[1]}`}
	/>
	<path class="thumb-edge" d={SHAPES.thumb} />

	<!-- What the pointer can pick. -->
	{#if live}
		{#each PICKS as p (p.id)}
			<path
				class={['pick', p.pad && 'pad', lit(p.id) && 'lit', app.selected === p.id && 'chosen']}
				d={p.d}
				role="presentation"
				onpointerenter={() => (app.hovered = p.id)}
				onpointerleave={() => (app.hovered = null)}
				onclick={() => app.open(p.id)}
			/>
		{/each}
		{#each PICKS as p (p.id)}
			{#if pulsed(p.id) && CENTERS[p.id]}
				{#key app.pulse.n}
					{#each [0, 1, 2] as k (k)}
						<circle
							class="ripple"
							style:animation-delay={`${k * 140}ms`}
							cx={CENTERS[p.id]![0]}
							cy={CENTERS[p.id]![1]}
							r="14"
						/>
					{/each}
				{/key}
			{/if}
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
		opacity: 0.35;
	}

	.body {
		fill: url(#t-body);
	}

	.rim-dark {
		fill: none;
		stroke: var(--m-lo);
		stroke-width: 70;
		opacity: 0.55;
		transform: translate(16px, 10px);
	}

	.rim-light {
		fill: none;
		stroke: var(--m-glint);
		stroke-width: 10;
		opacity: 0.6;
	}

	.crease {
		fill: none;
		stroke: var(--m-glint);
		stroke-width: 26;
		opacity: 0.45;
	}

	.edge {
		fill: none;
		stroke: var(--m-edge);
		stroke-width: 1.5;
	}

	.seam {
		fill: none;
		stroke: var(--m-seam);
		stroke-width: 2.6;
		stroke-linecap: round;
	}

	.seam.soft {
		stroke-width: 1.8;
		opacity: 0.55;
	}

	.seam.faint {
		stroke-width: 1.2;
		opacity: 0.25;
	}

	.slot {
		fill: var(--m-seam);
	}

	.rim-base {
		fill: none;
		stroke: var(--m-steel-3);
	}

	.rim-teeth {
		fill: none;
		stroke: var(--m-steel-1);
		stroke-dasharray: 1.7 2.3;
	}

	.rim-groove {
		fill: none;
		stroke: var(--m-seam);
		stroke-width: 2.5;
	}

	.slot-lip {
		fill: none;
		stroke: var(--m-seam);
		stroke-width: 2;
	}

	.key {
		fill: url(#t-pill);
		stroke: var(--m-seam);
		stroke-width: 2.2;
	}

	.led {
		fill: var(--m-lo);
		transition: fill 0.4s;
	}

	.led.on {
		fill: var(--led);
		filter: drop-shadow(0 0 6px var(--led-glow));
	}

	.led.low {
		fill: #e5484d;
	}

	.ring {
		fill: none;
		stroke: var(--m-ring);
		stroke-width: 4;
	}

	.panel-seam {
		fill: none;
		stroke: var(--m-seam);
		stroke-width: 3.4;
		stroke-linejoin: round;
	}

	.dot {
		fill: var(--m-lo);
	}

	.pill {
		fill: url(#t-pill);
		stroke: var(--m-seam);
		stroke-width: 2.4;
	}

	.housing {
		fill: var(--m-mid);
		stroke: var(--m-edge);
		stroke-width: 1.2;
		opacity: 0.9;
	}

	.nub {
		fill: url(#t-pill);
		stroke: var(--m-edge);
		stroke-width: 1.4;
	}

	.knurl {
		stroke: var(--m-steel-4);
		stroke-width: 1.1;
		opacity: 0.4;
	}

	.groove {
		stroke: var(--m-steel-3);
		stroke-width: 7;
		stroke-linecap: butt;
	}

	.thumb-edge {
		fill: none;
		stroke: var(--m-seam);
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

	.pick.pad {
		stroke-width: 22;
		stroke-linejoin: round;
	}

	.pick.lit {
		fill: var(--m-pick);
		stroke: var(--ink);
		stroke-width: 3.5;
	}

	.pick.chosen {
		stroke-width: 4;
		filter: drop-shadow(0 0 10px var(--m-pick));
	}

	.ripple {
		fill: none;
		stroke: var(--ink);
		stroke-width: 3;
		opacity: 0;
		transform-box: fill-box;
		transform-origin: center;
		animation: ripple 1.1s ease-out forwards;
		pointer-events: none;
	}

	@keyframes ripple {
		from {
			transform: scale(1);
			opacity: 0.9;
		}
		to {
			transform: scale(7);
			opacity: 0;
		}
	}
</style>
