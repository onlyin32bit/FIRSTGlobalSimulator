<script lang="ts">
	import { page } from '$app/state';
	import { IconMenu2 } from '@tabler/icons-svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Sheet from '$lib/components/ui/sheet';
	import { activeDocsItem, docsNav } from './docs-nav';

	let { children } = $props();
	let mobileNavigationOpen = $state(false);

	const activeItem = $derived(activeDocsItem(page.url.pathname));
</script>

<svelte:head>
	<meta name="theme-color" content="#7f2c25" />
</svelte:head>

<div class="min-h-screen bg-background text-foreground">
	<header class="sticky top-0 z-30 border-b border-border/80 bg-background/90 backdrop-blur">
		<div class="mx-auto flex h-14 max-w-screen-2xl items-center gap-3 px-4 sm:px-6">
			<Sheet.Root bind:open={mobileNavigationOpen}>
				<Sheet.Trigger>
					{#snippet child({ props })}
						<Button variant="ghost" size="icon-sm" class="lg:hidden" {...props}>
							<IconMenu2 />
							<span class="sr-only">Open documentation navigation</span>
						</Button>
					{/snippet}
				</Sheet.Trigger>
				<Sheet.Content side="left" class="w-[18rem] p-0" showCloseButton={false}>
					<div class="border-b border-border px-5 py-4">
						<a
							class="font-semibold text-primary"
							href="/docs"
							onclick={() => (mobileNavigationOpen = false)}>FIRST Global Simulator</a
						>
					</div>
					<nav class="p-4" aria-label="Documentation navigation">
						{#each docsNav as section}
							<p
								class="px-2 pt-4 pb-2 text-xs font-semibold tracking-wider text-muted-foreground uppercase"
							>
								{section.title}
							</p>
							{#each section.items as item}
								<a
									class="block rounded-md px-2 py-2 text-sm transition-colors {page.url.pathname ===
									item.href
										? 'bg-primary/15 font-medium text-primary'
										: 'text-muted-foreground hover:bg-muted hover:text-foreground'}"
									href={item.href}
									onclick={() => (mobileNavigationOpen = false)}>{item.title}</a
								>
							{/each}
						{/each}
					</nav>
				</Sheet.Content>
			</Sheet.Root>

			<a class="text-sm font-semibold text-primary sm:text-base" href="/">FIRST Global Simulator</a>
			<span class="hidden h-5 border-l border-border sm:block"></span>
			<span class="hidden text-sm text-muted-foreground sm:block">Documentation</span>
			<nav class="ml-auto flex items-center gap-1" aria-label="Site navigation">
				<a
					class="rounded-md px-2.5 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
					href="/">Home</a
				>
				<a
					class="rounded-md px-2.5 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
					href="/sponsor">Sponsor</a
				>
				<Button href="/dashboard" size="sm" class="hidden sm:inline-flex">Enter simulator</Button>
			</nav>
		</div>
	</header>

	<div class="mx-auto grid max-w-screen-2xl grid-cols-1 lg:grid-cols-[15rem_minmax(0,1fr)_12rem]">
		<aside
			class="sticky top-14 hidden h-[calc(100vh-3.5rem)] overflow-y-auto border-r border-border px-4 py-6 lg:block"
		>
			<nav aria-label="Documentation navigation">
				{#each docsNav as section}
					<p
						class="px-2 pt-4 pb-2 text-xs font-semibold tracking-wider text-muted-foreground uppercase"
					>
						{section.title}
					</p>
					{#each section.items as item}
						<a
							class="block rounded-md px-2 py-2 text-sm transition-colors {page.url.pathname ===
							item.href
								? 'bg-primary/15 font-medium text-primary'
								: 'text-muted-foreground hover:bg-muted hover:text-foreground'}"
							href={item.href}>{item.title}</a
						>
					{/each}
				{/each}
			</nav>
		</aside>

		<main class="min-w-0 px-5 py-10 sm:px-8 lg:px-12 lg:py-14">
			<article class="docs-prose mx-auto max-w-3xl">
				{@render children()}
			</article>
		</main>

		<aside
			class="sticky top-14 hidden h-[calc(100vh-3.5rem)] overflow-y-auto border-l border-border px-5 py-10 xl:block"
		>
			<p class="text-xs font-semibold tracking-wider text-muted-foreground uppercase">
				On this page
			</p>
			<nav class="mt-3 border-l border-border" aria-label="On this page">
				{#each activeItem.headings as heading}
					<a
						class="-ml-px block border-l px-3 py-1 text-sm text-muted-foreground transition-colors hover:border-primary hover:text-foreground"
						href={`#${heading.id}`}>{heading.title}</a
					>
				{/each}
			</nav>
		</aside>
	</div>
</div>

<style>
	:global(.docs-prose) {
		color: var(--foreground);
	}

	:global(.docs-prose h1) {
		margin: 0;
		font-size: clamp(2.25rem, 5vw, 3.5rem);
		font-weight: 700;
		line-height: 1.1;
		letter-spacing: -0.04em;
	}

	:global(.docs-prose h2) {
		margin-top: 3.5rem;
		margin-bottom: 1rem;
		font-size: 1.5rem;
		font-weight: 650;
		letter-spacing: -0.02em;
		scroll-margin-top: 5rem;
	}

	:global(.docs-prose h3) {
		margin-top: 2rem;
		margin-bottom: 0.75rem;
		font-size: 1.125rem;
		font-weight: 600;
		scroll-margin-top: 5rem;
	}

	:global(.docs-prose p),
	:global(.docs-prose li) {
		color: var(--muted-foreground);
		line-height: 1.75;
	}

	:global(.docs-prose p) {
		margin-top: 1rem;
	}

	:global(.docs-prose ul),
	:global(.docs-prose ol) {
		margin-top: 1rem;
		padding-left: 1.4rem;
	}

	:global(.docs-prose ul) {
		list-style: disc;
	}
	:global(.docs-prose ol) {
		list-style: decimal;
	}
	:global(.docs-prose li + li) {
		margin-top: 0.5rem;
	}
	:global(.docs-prose a) {
		color: var(--primary);
		text-decoration: underline;
		text-underline-offset: 4px;
	}
	:global(.docs-prose code) {
		border: 1px solid var(--border);
		border-radius: 0.3rem;
		background: color-mix(in oklch, var(--muted) 65%, transparent);
		padding: 0.1rem 0.3rem;
		font-size: 0.85em;
		color: var(--foreground);
	}
	:global(.docs-prose pre) {
		margin-top: 1.25rem;
		overflow-x: auto;
		border: 1px solid var(--border);
		border-radius: 0.75rem;
		background: color-mix(in oklch, var(--card) 90%, black);
		padding: 1rem;
	}
	:global(.docs-prose pre code) {
		border: 0;
		background: transparent;
		padding: 0;
	}
	:global(.docs-prose blockquote) {
		margin-top: 1.5rem;
		border-left: 3px solid var(--primary);
		background: color-mix(in oklch, var(--primary) 10%, transparent);
		padding: 1rem;
		color: var(--muted-foreground);
	}
</style>
