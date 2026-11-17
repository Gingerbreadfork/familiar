<script lang="ts">
	import TopView, { CENTERS } from './TopView.svelte';
	import UnderView, { UNDER_CENTERS } from './UnderView.svelte';
	import { part, type PartId } from '../actions';
	import { app } from '../state.svelte';

	interface Props {
		/** False draws the mouse dimmed and still, for when it cannot be reached. */
		live?: boolean;
	}

	let { live = true }: Props = $props();

	// Each view's drawing space, matching the viewBox of its SVG.
	const SPACE = {
		top: { x: -40, y: -30, w: 790, h: 1190 },
		under: { x: -40, y: -30, w: 852, h: 1190 }
	};

	let space = $derived(SPACE[app.view]);
	let tag = $derived.by(() => {
		const id = app.hovered as PartId | null;
		if (!live || !id || part(id).side !== app.view) return null;
		const at = (app.view === 'top' ? CENTERS : UNDER_CENTERS)[id];
		if (!at) return null;
		return {
			id,
			left: `${((at[0] - space.x) / space.w) * 100}%`,
			top: `${((at[1] - space.y) / space.h) * 100}%`,
			flip: (at[0] - space.x) / space.w < 0.45
		};
	});

	/** Turns the drawing over: a quarter turn out, then a quarter turn in. */
	function turn(_node: Element, { delay = 0, out = false }: { delay?: number; out?: boolean }) {
		return {
			delay,
			duration: 220,
			css: (t: number) => {
				const angle = (1 - t) * 90 * (out ? -1 : 1);
				return `transform: perspective(1600px) rotateY(${angle}deg); opacity: ${0.4 + 0.6 * t}`;
			}
		};
	}
</script>

<div class={['stage', !live && 'still']}>
	<div class="bench">
		{#key app.view}
			<figure
				class="figure"
				style:aspect-ratio={`${space.w} / ${space.h}`}
				in:turn={{ delay: 220 }}
				out:turn={{ out: true }}
			>
				{#if app.view === 'top'}
					<TopView {live} />
				{:else}
					<UnderView {live} />
				{/if}
				{#if tag}
					<div class={['tag', tag.flip && 'left']} style:left={tag.left} style:top={tag.top} aria-hidden="true">
						<span class="tag-name">{part(tag.id).name}</span>
						<span class="tag-does">{app.summary(tag.id)}</span>
					</div>
				{/if}
			</figure>
		{/key}
	</div>

	<div class="turn" role="group" aria-label="Side of the mouse">
		<button class={[app.view === 'top' && 'on']} aria-pressed={app.view === 'top'} onclick={() => (app.view = 'top')}>
			Top
		</button>
		<button
			class={[app.view === 'under' && 'on']}
			aria-pressed={app.view === 'under'}
			onclick={() => (app.view = 'under')}
		>
			Underneath
		</button>
	</div>
</div>

<style>
	.stage {
		position: relative;
		display: grid;
		grid-template-rows: 1fr auto;
		min-height: 0;
		height: 100%;
		padding: 1.5rem 1rem 1.25rem;
	}

	.bench {
		position: relative;
		display: grid;
		place-items: center;
		min-height: 0;
	}

	.figure {
		grid-area: 1 / 1;
		position: relative;
		margin: 0;
		height: 100%;
		max-height: 100%;
		max-width: 100%;
		backface-visibility: hidden;
	}

	.still .figure {
		opacity: 0.35;
		filter: saturate(0);
	}

	.tag {
		position: absolute;
		display: grid;
		gap: 0.05rem;
		padding: 0.45rem 0.7rem;
		border-radius: 8px;
		background: var(--ink);
		color: var(--shell);
		white-space: nowrap;
		pointer-events: none;
		transform: translate(2.5rem, -50%);
		box-shadow: 0 6px 18px rgb(0 0 0 / 0.25);
	}

	.tag.left {
		transform: translate(calc(-100% - 2.5rem), -50%);
	}

	.tag-name {
		font-family: var(--narrow);
		font-weight: 600;
	}

	.tag-does {
		font-size: var(--t-sm);
		opacity: 0.75;
	}

	.turn {
		justify-self: center;
		display: flex;
		gap: 3px;
		padding: 3px;
		border-radius: 999px;
		background: var(--shell);
		box-shadow: var(--press);
	}

	.turn button {
		padding: 0.35rem 1rem;
		border: 0;
		border-radius: 999px;
		background: none;
		color: var(--muted);
		font-size: var(--t-sm);
		cursor: pointer;
	}

	.turn button.on {
		background: var(--raised);
		color: var(--ink);
		box-shadow: var(--shadow);
		font-weight: 500;
	}
</style>
