<script lang="ts">
	import type { Component } from 'svelte';

	let {
		icon,
		title,
		description,
		href,
		onclick,
		featured = false
	}: {
		icon: Component;
		title: string;
		description: string;
		href?: string;
		onclick?: () => void;
		featured?: boolean;
	} = $props();
	const Icon = icon;
</script>

{#if href}
	<a {href} class:featured class="action-card">
		<Icon class="size-5" />
		<span><strong>{title}</strong><small>{description}</small></span>
	</a>
{:else}
	<button type="button" class:featured class="action-card" {onclick}>
		<Icon class="size-5" />
		<span><strong>{title}</strong><small>{description}</small></span>
	</button>
{/if}

<style>
	.action-card {
		display: grid;
		grid-template-columns: auto 1fr;
		gap: 0.9rem;
		width: 100%;
		align-items: start;
		border-radius: 0.9rem;
		background: color-mix(in oklch, var(--card) 82%, transparent);
		padding: 1rem;
		text-align: left;
		color: var(--muted-foreground);
		transition:
			background-color 160ms ease,
			color 160ms ease,
			transform 160ms ease;
	}

	.action-card:hover {
		background: color-mix(in oklch, var(--accent) 45%, var(--card));
		color: var(--foreground);
		transform: translateY(-2px);
	}

	.action-card:focus-visible {
		outline: 2px solid var(--ring);
		outline-offset: 3px;
	}
	.action-card strong,
	.action-card small {
		display: block;
	}
	.action-card strong {
		color: var(--foreground);
		font-size: 0.95rem;
	}
	.action-card small {
		margin-top: 0.25rem;
		font-size: 0.8125rem;
		line-height: 1.35;
	}
	.featured {
		background: color-mix(in oklch, var(--primary) 18%, var(--card));
		color: var(--primary);
	}
</style>
