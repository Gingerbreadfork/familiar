<script lang="ts">
	import { PARTS } from '../actions';
	import { app } from '../state.svelte';

	const GROUPS = [
		{ side: 'top', title: 'On top' },
		{ side: 'under', title: 'Underneath' }
	] as const;
</script>

<div class="overview">
	<header>
		<h1>What each part does</h1>
		<p class="where">Pick a part on the mouse, or from the list. Changes apply as you make them.</p>
	</header>

	{#each GROUPS as g (g.side)}
		<section class="group">
			<h2 class="section-title">{g.title}</h2>
			<ul>
				{#each PARTS.filter((p) => p.side === g.side) as p (p.id)}
					<li>
						<button
							class={['row', app.hovered === p.id && 'lit']}
							onclick={() => app.open(p.id)}
							onpointerenter={() => (app.hovered = p.id)}
							onpointerleave={() => (app.hovered = null)}
							onfocus={() => (app.hovered = p.id)}
							onblur={() => (app.hovered = null)}
						>
							<span class="name">{p.name}</span>
							<span class="does">{app.summary(p.id)}</span>
							<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M6 3l5 5-5 5" /></svg>
						</button>
					</li>
				{/each}
			</ul>
		</section>
	{/each}
</div>

<style>
	.overview {
		display: grid;
		gap: 1.4rem;
		align-content: start;
	}

	header {
		display: grid;
		gap: 0.4rem;
	}

	h1 {
		font-size: var(--t-h1);
	}

	.where {
		color: var(--muted);
	}

	.group {
		gap: 0.5rem;
	}

	ul {
		list-style: none;
		margin: 0 -0.6rem;
		padding: 0;
		display: grid;
	}

	.row {
		display: grid;
		grid-template-columns: 1fr auto;
		grid-template-areas: 'name arrow' 'does arrow';
		align-items: center;
		column-gap: 1rem;
		width: 100%;
		padding: 0.5rem 0.6rem;
		border: 0;
		border-radius: 8px;
		background: none;
		text-align: left;
		cursor: pointer;
		transition: background 0.12s;
	}

	.row.lit {
		background: var(--raised);
		box-shadow: var(--shadow);
	}

	.name {
		grid-area: name;
		font-weight: 500;
	}

	.does {
		grid-area: does;
		font-size: var(--t-sm);
		color: var(--muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	svg {
		grid-area: arrow;
		color: var(--muted);
		opacity: 0;
		transition: opacity 0.12s;
	}

	.lit svg {
		opacity: 1;
	}

	path {
		fill: none;
		stroke: currentColor;
		stroke-width: 1.8;
		stroke-linecap: round;
		stroke-linejoin: round;
	}
</style>
