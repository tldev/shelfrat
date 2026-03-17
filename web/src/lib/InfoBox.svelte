<script lang="ts">
	import type { Snippet } from 'svelte';

	let { title, children }: { title: string; children: Snippet } = $props();
</script>

<aside class="info-box">
	<h3>{title}</h3>
	{@render children()}
</aside>

<style>
	.info-box {
		width: 26rem;
		flex-shrink: 0;
		border: 1px solid var(--border);
		padding: 1rem;
		font-size: 0.8rem;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.info-box h3 {
		font-size: 0.7rem;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--fg-muted);
		margin: 0;
	}

	.info-box :global(p) {
		margin: 0;
		color: var(--fg-muted);
		line-height: 1.4;
	}

	.info-box :global(code) {
		font-size: 0.75rem;
	}

	.info-box :global(dl) {
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.35rem;
	}

	.info-box :global(dt) {
		font-size: 0.7rem;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.info-box :global(dd) {
		margin: 0 0 0.25rem;
	}

	.info-box :global(a) {
		color: var(--fg);
	}

	@media (max-width: 640px) {
		.info-box {
			width: auto;
		}
	}
</style>
