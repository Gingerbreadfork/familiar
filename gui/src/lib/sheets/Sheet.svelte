<script lang="ts">
	import type { Snippet } from 'svelte';
	import { part, type PartId } from '../actions';
	import { app } from '../state.svelte';

	interface Props {
		id?: PartId;
		title?: string;
		where?: string;
		children: Snippet;
	}

	let { id, title, where, children }: Props = $props();
	let info = $derived(id ? part(id) : null);
</script>

<article class="sheet">
	<button class="back" onclick={() => app.open(null)}>
		<svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M10 3 5 8l5 5" /></svg>
		All parts
	</button>
	<header>
		<h1>{title ?? info?.name}</h1>
		{#if where ?? info?.where}<p class="where">{where ?? info?.where}</p>{/if}
	</header>
	{@render children()}
</article>

<style>
	.sheet {
		display: grid;
		gap: 1.4rem;
		align-content: start;
	}

	.back {
		justify-self: start;
		display: inline-flex;
		align-items: center;
		gap: 0.3rem;
		padding: 0.2rem 0.4rem 0.2rem 0.1rem;
		margin-left: -0.1rem;
		border: 0;
		border-radius: 6px;
		background: none;
		color: var(--muted);
		font-size: var(--t-sm);
		cursor: pointer;
	}

	.back:hover {
		color: var(--ink);
	}

	.back path {
		fill: none;
		stroke: currentColor;
		stroke-width: 1.8;
		stroke-linecap: round;
		stroke-linejoin: round;
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
</style>
